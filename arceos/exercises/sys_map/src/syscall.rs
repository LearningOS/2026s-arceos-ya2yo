#![allow(dead_code)]

use core::ffi::{c_void, c_char, c_int};
use axhal::arch::TrapFrame;
use axhal::trap::{register_trap_handler, SYSCALL};
use axerrno::LinuxError;
use axtask::current;
use axtask::TaskExtRef;
use axhal::paging::MappingFlags;
use arceos_posix_api as api;
use memory_addr::{PAGE_SIZE_4K, VirtAddrRange};

const SYS_IOCTL: usize = 29;
const SYS_OPENAT: usize = 56;
const SYS_CLOSE: usize = 57;
const SYS_READ: usize = 63;
const SYS_WRITE: usize = 64;
const SYS_WRITEV: usize = 66;
const SYS_EXIT: usize = 93;
const SYS_EXIT_GROUP: usize = 94;
const SYS_SET_TID_ADDRESS: usize = 96;
const SYS_MMAP: usize = 222;

const AT_FDCWD: i32 = -100;

/// Macro to generate syscall body
///
/// It will receive a function which return Result<_, LinuxError> and convert it to
/// the type which is specified by the caller.
/// 该宏接收两个参数：
/// 1. $fn: 函数标识符（用于日志打印函数名）
/// 2. $stmt: 具体业务代码块（系统调用的核心实现逻辑）
#[macro_export]
macro_rules! syscall_body {
    ($fn: ident, $($stmt: tt)*) => {{
        // 1. 执行具体的业务逻辑块
        // 这里使用了一个“立即执行闭包 (IIFE)”，并将返回类型强制指定为 axerrno::LinuxResult<_>。
        // 这样做的好处是：在 $($stmt)* 代码块内部可以直接使用 `?` 操作符来处理错误。
        #[allow(clippy::redundant_closure_call)]
        let res = (|| -> axerrno::LinuxResult<_> { $($stmt)* })();
         // 2. 自动日志打印
        // 根据执行结果 res 分级打印日志，方便调试。
        match res {
            // 如果成功 (Ok) 或者返回 EAGAIN (资源暂时不可用，通常在非阻塞 IO 中常见，不视为严重错误)
            // 使用 debug! 级别打印，格式类似于 "sys_read => Ok(10)"
            Ok(_) | Err(axerrno::LinuxError::EAGAIN) => debug!(concat!(stringify!($fn), " => {:?}"),  res),
            // 如果发生了真正的错误 (如 EPERM, ENOENT 等)
            // 使用 info! 级别打印，以便开发者在控制台更容易注意到失败的系统调用
            Err(_) => info!(concat!(stringify!($fn), " => {:?}"), res),
        }
        // 3. 返回值转换 (符合 Linux ABI 规范)
        // Linux 系统调用约定：成功返回正数或 0，失败返回负的错误码（-Errno）。
        match res {
            Ok(v) => v as _,
            // 失败时：获取错误对象 e 对应的错误码数字（如 EBADF 是 9），
            // 然后取其负数（-9）并转换类型返回。
            Err(e) => {
                -e.code() as _
            }
        }
    }};
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// permissions for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapProt: i32 {
        /// Page can be read.
        const PROT_READ = 1 << 0;
        /// Page can be written.
        const PROT_WRITE = 1 << 1;
        /// Page can be executed.
        const PROT_EXEC = 1 << 2;
    }
}

impl From<MmapProt> for MappingFlags {
    fn from(value: MmapProt) -> Self {
        let mut flags = MappingFlags::USER;
        if value.contains(MmapProt::PROT_READ) {
            flags |= MappingFlags::READ;
        }
        if value.contains(MmapProt::PROT_WRITE) {
            flags |= MappingFlags::WRITE;
        }
        if value.contains(MmapProt::PROT_EXEC) {
            flags |= MappingFlags::EXECUTE;
        }
        flags
    }
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// flags for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapFlags: i32 {
        /// Share changes
        const MAP_SHARED = 1 << 0;
        /// Changes private; copy pages on write.
        const MAP_PRIVATE = 1 << 1;
        /// Map address must be exactly as requested, no matter whether it is available.
        const MAP_FIXED = 1 << 4;
        /// Don't use a file.
        const MAP_ANONYMOUS = 1 << 5;
        /// Don't check for reservations.
        const MAP_NORESERVE = 1 << 14;
        /// Allocation is for a stack.
        const MAP_STACK = 0x20000;
    }
}

#[register_trap_handler(SYSCALL)]
fn handle_syscall(tf: &TrapFrame, syscall_num: usize) -> isize {
    ax_println!("handle_syscall [{}] ...", syscall_num);
    let ret = match syscall_num {
         SYS_IOCTL => sys_ioctl(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _) as _,
        SYS_SET_TID_ADDRESS => sys_set_tid_address(tf.arg0() as _),
        SYS_OPENAT => sys_openat(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _, tf.arg3() as _),
        SYS_CLOSE => sys_close(tf.arg0() as _),
        SYS_READ => sys_read(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITE => sys_write(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITEV => sys_writev(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_EXIT_GROUP => {
            ax_println!("[SYS_EXIT_GROUP]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_EXIT => {
            ax_println!("[SYS_EXIT]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_MMAP => sys_mmap(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2() as _,
            tf.arg3() as _,
            tf.arg4() as _,
            tf.arg5() as _,
        ),
        _ => {
            ax_println!("Unimplemented syscall: {}", syscall_num);
            -LinuxError::ENOSYS.code() as _
        }
    };
    ret
}

#[allow(unused_variables)]
fn sys_mmap(
    addr: *mut usize,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    _offset: isize,
) -> isize {
    syscall_body!(sys_mmap,{
        if length ==0 {
            return Err(LinuxError::EINVAL);
        }
        let length=(length+PAGE_SIZE_4K-1)&!(PAGE_SIZE_4K-1);
        let prot=MmapProt::from_bits(prot).ok_or(LinuxError::EINVAL)?;
        let flags=MmapFlags::from_bits(flags).ok_or(LinuxError::EINVAL)?;
        let mut mapping_flags=prot.into();
        mapping_flags|=MappingFlags::WRITE;
        let cur=current();
        let mut aspace=cur.task_ext().aspace.lock();
        let mut start_vddr=addr as usize;
        if flags.contains(MmapFlags::MAP_FIXED)&&start_vddr%PAGE_SIZE_4K!=0 {
            return Err(LinuxError::EINVAL);
        }
        if !flags.contains(MmapFlags::MAP_FIXED)&&start_vddr==0{
            start_vddr=aspace.find_free_area(0x2000_0000.into(),length,VirtAddrRange::new(0x2000_0000.into(), usize::MAX.into())).unwrap().as_usize();
        }
        aspace.map_alloc(start_vddr.into(), length, mapping_flags, true)?;
        drop(aspace);
        if fd!=-1 {
            let read_len=api::sys_read(fd, start_vddr as *mut c_void, length);
            if read_len<0{
                debug!("mmap file read failed: {}",read_len);
            }
        }
        Ok(start_vddr)
    })
}

fn sys_openat(dfd: c_int, fname: *const c_char, flags: c_int, mode: api::ctypes::mode_t) -> isize {
    assert_eq!(dfd, AT_FDCWD);
    api::sys_open(fname, flags, mode) as isize
}

fn sys_close(fd: i32) -> isize {
    api::sys_close(fd) as isize
}

fn sys_read(fd: i32, buf: *mut c_void, count: usize) -> isize {
    api::sys_read(fd, buf, count)
}

fn sys_write(fd: i32, buf: *const c_void, count: usize) -> isize {
    api::sys_write(fd, buf, count)
}

fn sys_writev(fd: i32, iov: *const api::ctypes::iovec, iocnt: i32) -> isize {
    unsafe { api::sys_writev(fd, iov, iocnt) }
}

fn sys_set_tid_address(tid_ptd: *const i32) -> isize {
    let curr = current();
    curr.task_ext().set_clear_child_tid(tid_ptd as _);
    curr.id().as_u64() as isize
}

fn sys_ioctl(_fd: i32, _op: usize, _argp: *mut c_void) -> i32 {
    ax_println!("Ignore SYS_IOCTL");
    0
}

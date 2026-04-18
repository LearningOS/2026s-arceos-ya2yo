#![no_std]

use allocator::{BaseAllocator, ByteAllocator, PageAllocator};

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const SIZE: usize> {
    start:usize,
    end:usize,
    b_pos:usize,
    p_pos:usize,
    alloc_count:usize,
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start:0,
            end:0,
            b_pos:0,
            p_pos:0,
            alloc_count:0
        }
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        self.start=start;
        self.end=start+size;
        self.b_pos=self.start;
        self.p_pos=(start+size)&!(SIZE-1);
    }

    fn add_memory(&mut self, start: usize, size: usize) -> allocator::AllocResult {
        if self.start==0 {
            self.init(start, size);
            Ok(())
        }else{
            Err(allocator::AllocError::NoMemory)
        }
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(
        &mut self,
        layout: core::alloc::Layout,
    ) -> allocator::AllocResult<core::ptr::NonNull<u8>> {
        let align=layout.align();
        let size=layout.size();
        // 分配的区间
        let alloc_start=(self.b_pos+align-1)&!(align-1);
        let alloc_end=alloc_start+size;
        // 冲突检测
        if alloc_end <= self.p_pos {
            self.b_pos=alloc_end;
            self.alloc_count+=1;
            Ok(core::ptr::NonNull::new(alloc_start as *mut u8).unwrap())
        }else{
            Err(allocator::AllocError::NoMemory)
        }
    }

    fn dealloc(&mut self, _pos: core::ptr::NonNull<u8>, _layout: core::alloc::Layout) {
        if self.alloc_count > 0{
            self.alloc_count-=1;
        }
        if self.alloc_count==0{
            self.b_pos=self.start;
        }

    }

    fn total_bytes(&self) -> usize {
        self.end-self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos-self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos-self.b_pos
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(
        &mut self,
        num_pages: usize,
        _align_pow2: usize,
    ) -> allocator::AllocResult<usize> {
        let total_size = num_pages * Self::PAGE_SIZE;
        // 对齐
        let new_p_pos = self.p_pos.checked_sub(total_size).ok_or(allocator::AllocError::NoMemory)?;
        // 检查是否撞上了字节分配区域
        if new_p_pos >= self.b_pos {
            self.p_pos = new_p_pos;
            Ok(new_p_pos)
        } else {
            Err(allocator::AllocError::NoMemory)
        }
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {
        todo!()
    }

    fn total_pages(&self) -> usize {
        (self.end-self.start) / SIZE
    }

    fn used_pages(&self) -> usize {
        let initial_pos=self.end&!(SIZE-1);
        (initial_pos-self.p_pos)/SIZE
    }

    fn available_pages(&self) -> usize {
        self.available_bytes()/SIZE
    }
}
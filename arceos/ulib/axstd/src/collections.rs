#[cfg(feature = "alloc")]
pub use alloc::collections::*;
pub use hashbrown::HashMap;
use axhal::misc::random;

fn kernel_get_random(buf:&mut [u8])->Result<(),getrandom::Error>{
    for i in 0..buf.len(){
        buf[i]=random() as u8;
    }
    Ok(())
}
getrandom::register_custom_getrandom!(kernel_get_random);
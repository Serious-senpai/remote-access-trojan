#[derive(Debug)]
pub struct KernelHandoff {
    pub original_driver_entry: *mut u8,
    pub original_instructions: *const u8,
    pub original_instructions_len: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct KernelHandoff {
    pub original_driver_entry: *mut u8,
    pub original_instructions: *const u8,
    pub original_instructions_len: usize,
    pub rtl_random_addr: *mut u8,
    pub rtl_random_ex_addr: *mut u8,
}

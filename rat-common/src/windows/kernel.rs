#[derive(Debug, Clone, Copy)]
pub struct InstructionRecoveryInfo {
    pub address: *mut u8,
    pub instructions: *const u8,
    pub instructions_len: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct KernelHandoff {
    pub driver_entry: InstructionRecoveryInfo,
    pub object_callback_trampoline: *const u8,
}

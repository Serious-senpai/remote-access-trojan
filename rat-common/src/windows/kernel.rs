#[derive(Debug, Clone, Copy)]
pub struct InstructionRecoveryInfo {
    pub address: *mut u8,
    pub instructions: *const u8,
    pub instructions_len: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct KernelHandoff {
    pub driver_entry: InstructionRecoveryInfo,
    pub mm_verify_callback_function_check_flags: InstructionRecoveryInfo,
    pub ke_bug_check_ex: InstructionRecoveryInfo,
}

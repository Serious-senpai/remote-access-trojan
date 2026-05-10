use core::arch::global_asm;

use crate::utils::get_function_code;

/// Overwrite instructions at `target` with `patched`.
///
/// The number of bytes overwritten is `patched.len()`. After overwriting, an `addr` is written
/// in little-endian order to `target` at index `addr_insert_offset`.
///
/// Optionally:
/// - If `original` is not None, return the overwritten bytes originally.
/// - If `size` is not None, return the number of bytes overwritten (which is `patched.len()`).
///
/// Returns whether the operation succeeded.
fn _call_insertion(
    target: &mut [u8],
    addr: u64,
    original: Option<&mut [u8]>,
    size: Option<&mut usize>,
    patched: &[u8],
    addr_insert_offset: usize,
) -> bool {
    let len = patched.len();
    if let Some(size) = size {
        *size = len;
    }

    if target.len() < len {
        return false;
    }

    if let Some(original) = original {
        if original.len() < len {
            return false;
        }

        original[..len].copy_from_slice(&target[..len]);
    }

    target[..len].copy_from_slice(patched);
    target[addr_insert_offset..addr_insert_offset + 8].copy_from_slice(&addr.to_le_bytes());
    true
}

global_asm!(
    ".global CallTrampoline",
    "CallTrampoline:",
    "movabs rax, 0",
    "call rax",
    ".global CallTrampolineEnd",
    "CallTrampolineEnd:",
);

unsafe extern "win64" {
    fn CallTrampoline();
    fn CallTrampolineEnd();
}

pub fn insert_call_trampoline(
    target: &mut [u8],
    addr: u64,
    original: Option<&mut [u8]>,
    size: Option<&mut usize>,
) -> bool {
    let patched = get_function_code(CallTrampoline as *const u8, CallTrampolineEnd as *const u8);

    _call_insertion(target, addr, original, size, patched, 2)
}

global_asm!(
    ".global JmpTrampoline",
    "JmpTrampoline:",
    "movabs rax, 0",
    "jmp rax",
    ".global JmpTrampolineEnd",
    "JmpTrampolineEnd:",
);

unsafe extern "win64" {
    fn JmpTrampoline();
    fn JmpTrampolineEnd();
}

pub fn insert_jmp_trampoline(
    target: &mut [u8],
    addr: u64,
    original: Option<&mut [u8]>,
    size: Option<&mut usize>,
) -> bool {
    let patched = get_function_code(JmpTrampoline as *const u8, JmpTrampolineEnd as *const u8);

    _call_insertion(target, addr, original, size, patched, 2)
}

global_asm!(
    ".global ReturnZero",
    "ReturnZero:",
    "xor eax, eax",
    "ret",
    ".global ReturnZeroEnd",
    "ReturnZeroEnd:",
);

unsafe extern "win64" {
    fn ReturnZero();
    fn ReturnZeroEnd();
}

pub fn return_zero_patch() -> &'static [u8] {
    get_function_code(ReturnZero as *const u8, ReturnZeroEnd as *const u8)
}

global_asm!(
    ".global ReturnOne",
    "ReturnOne:",
    "mov eax, 1",
    "ret",
    ".global ReturnOneEnd",
    "ReturnOneEnd:",
);

unsafe extern "win64" {
    fn ReturnOne();
    fn ReturnOneEnd();
}

pub fn return_one_patch() -> &'static [u8] {
    get_function_code(ReturnOne as *const u8, ReturnOneEnd as *const u8)
}

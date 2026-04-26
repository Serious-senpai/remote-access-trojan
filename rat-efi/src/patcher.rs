use alloc::vec::Vec;
use core::arch::global_asm;

use crate::utils;

pub struct PatternFinder {
    _original: &'static [u8],
}

impl PatternFinder {
    pub fn new(original: &'static [u8]) -> Self {
        Self {
            _original: original,
        }
    }

    // pub fn len(&self) -> usize {
    //     self._original.len()
    // }

    pub fn find_offset(&self, buffer: &[u8]) -> Option<usize> {
        utils::find_pattern(buffer, self._original)
    }

    // pub fn find_mut<'a>(&self, buffer: &'a mut [u8]) -> Option<&'a mut [u8]> {
    //     self.find_offset(buffer).map(|i| &mut buffer[i..])
    // }

    // pub fn find_ref<'a>(&self, buffer: &'a [u8]) -> Option<&'a [u8]> {
    //     self.find_offset(buffer).map(|i| &buffer[i..])
    // }
}

pub struct VariablePatternFinder<const SIZE: usize> {
    _patterns: Vec<PatternFinder>,
}

impl<const SIZE: usize> VariablePatternFinder<SIZE> {
    pub fn new(original: &'static [[u8; SIZE]]) -> Self {
        Self {
            _patterns: original
                .iter()
                .map(|pattern| PatternFinder::new(pattern))
                .collect(),
        }
    }

    pub fn find_offset(&self, buffer: &[u8]) -> Option<usize> {
        for finder in &self._patterns {
            if let Some(r) = finder.find_offset(buffer) {
                return Some(r);
            }
        }

        None
    }

    pub fn find_mut<'a>(&self, buffer: &'a mut [u8]) -> Option<&'a mut [u8]> {
        self.find_offset(buffer).map(|i| &mut buffer[i..])
    }

    // pub fn find_ref<'a>(&self, buffer: &'a [u8]) -> Option<&'a [u8]> {
    //     self.find_offset(buffer).map(|i| &buffer[i..])
    // }
}

fn call_insertion(
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

    log::info!("Replacing {:02X?}", &target[..len]);
    target[..len].copy_from_slice(patched);
    target[addr_insert_offset..addr_insert_offset + 8].copy_from_slice(&addr.to_le_bytes());
    log::info!("With {:02X?}", &target[..len]);
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
    let patched =
        utils::get_function_code(CallTrampoline as *const u8, CallTrampolineEnd as *const u8);

    call_insertion(target, addr, original, size, patched, 2)
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
    let patched =
        utils::get_function_code(JmpTrampoline as *const u8, JmpTrampolineEnd as *const u8);

    call_insertion(target, addr, original, size, patched, 2)
}

global_asm!(
    ".global ReturnZero",
    "ReturnZero:",
    "xor rax, rax",
    "ret",
    ".global ReturnZeroEnd",
    "ReturnZeroEnd:",
);

unsafe extern "win64" {
    fn ReturnZero();
    fn ReturnZeroEnd();
}

pub fn return_zero_patch() -> &'static [u8] {
    utils::get_function_code(ReturnZero as *const u8, ReturnZeroEnd as *const u8)
}

// global_asm!(
//     ".global ReturnOne",
//     "ReturnOne:",
//     "mov rax, 1",
//     "ret",
//     ".global ReturnOneEnd",
//     "ReturnOneEnd:",
// );

// unsafe extern "win64" {
//     fn ReturnOne();
//     fn ReturnOneEnd();
// }

// pub fn return_one_patch() -> &'static [u8] {
//     utils::get_function_code(ReturnOne as *const u8, ReturnOneEnd as *const u8)
// }

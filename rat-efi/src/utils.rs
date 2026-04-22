use core::arch::asm;
use core::time::Duration;
use core::{mem, slice};

use log::{debug, info, trace};
use uefi::boot;
use windows_sys::Win32::System::Diagnostics::Debug::IMAGE_NT_HEADERS64;
use windows_sys::Win32::System::SystemServices::IMAGE_DOS_HEADER;

use crate::hooks::types::{_BLDR_DATA_TABLE_ENTRY, _KLDR_DATA_TABLE_ENTRY, _LIST_ENTRY};

pub fn countdown(seconds: u64) {
    for i in 0..seconds {
        info!("Counting down {} seconds...", seconds - i);
        boot::stall(Duration::from_secs(1));
    }
}

/// Extract rel32 of the instruction `E8 rel32`.
/// The provided buffer must start at `0xE8`.
pub fn extract_call_rel32(instruction: &[u8]) -> i32 {
    let mut rel32 = [0; 4];
    rel32.copy_from_slice(&instruction[1..5]);
    i32::from_le_bytes(rel32)
}

pub fn find_pattern(buffer: &[u8], pattern: &[u8]) -> Option<usize> {
    (0..buffer.len().saturating_sub(pattern.len())).find(|&index| buffer[index..index + pattern.len()] == *pattern)
}

#[macro_export]
macro_rules! fill_nops {
    (8) => {
        unsafe {
            asm!(
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 8
            );
        }
    };
    (16) => {
        unsafe {
            asm!(
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 8
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 16
            );
        }
    };
    (32) => {
        unsafe {
            asm!(
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 8
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 16
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", //
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 32
            );
        }
    };
    (64) => {
        unsafe {
            asm!(
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 8
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 16
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", //
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 32
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", //
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", //
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", //
                "nop", "nop", "nop", "nop", "nop", "nop", "nop", "nop", // 64
            );
        }
    };
}

macro_rules! find_pe_image {
    ($code:ident, $qualifier:ident, $deref:ident, $from_raw_parts:ident) => {
        loop {
            let ptr = $code as *$qualifier IMAGE_DOS_HEADER;
            match unsafe { ptr.$deref() } {
                Some(header) => {
                    if header.e_magic == 0x5A4D {
                        if let Ok(e_lfanew) = header.e_lfanew.try_into() {
                            let ptr = ptr.wrapping_byte_offset(e_lfanew) as *$qualifier IMAGE_NT_HEADERS64;
                            if let Some(header) = unsafe { ptr.$deref() } {
                                if header.Signature == 0x4550 {
                                    break Some(header);
                                }
                            }
                        }
                    }
                }
                None => break None,
            }

            $code = $code.wrapping_byte_sub(1);
        }.map(|h| unsafe { slice::$from_raw_parts($code, h.OptionalHeader.SizeOfImage as usize) })
    };
}

// pub unsafe fn find_pe_image<'a>(mut code: *const u8) -> Option<&'a [u8]> {
//     find_pe_image!(code, const, as_ref, from_raw_parts)
// }

pub unsafe fn find_pe_image_mut<'a>(mut code: *mut u8) -> Option<&'a mut [u8]> {
    find_pe_image!(code, mut, as_mut, from_raw_parts_mut)
}

pub fn write_cr0(cr0: u64) {
    unsafe {
        asm!(
            "mov cr0, {0}",
            in(reg) cr0,
        );
    }
}

pub fn read_cr0() -> u64 {
    let mut cr0: u64;
    unsafe {
        asm!(
            "mov {0}, cr0",
            out(reg) cr0,
        );
    }

    cr0
}

pub struct DisableWriteProtection {
    _original_cr0: u64,
}

impl DisableWriteProtection {
    pub fn new() -> Self {
        let cr0 = read_cr0();
        write_cr0(cr0 & !0x10000);
        Self { _original_cr0: cr0 }
    }
}

impl Drop for DisableWriteProtection {
    fn drop(&mut self) {
        write_cr0(self._original_cr0);
    }
}

pub fn get_function_code(
    start: unsafe extern "efiapi" fn(),
    end: unsafe extern "efiapi" fn(),
) -> &'static [u8] {
    let start_ptr = start as *const u8;
    let end_ptr = end as *const u8;

    let start_addr = start_ptr as usize;
    let end_addr = end_ptr as usize;

    unsafe { slice::from_raw_parts(start_ptr, end_addr.saturating_sub(start_addr)) }
}

/// Reference: https://github.com/Mattiwatti/EfiGuard/blob/801ad43372021d3806ef1be22dddfd0fb860693b/EfiGuardDxe/PatchWinload.c#L57-L79
pub unsafe fn get_boot_loaded_module<'a>(
    load_order_list_head: *const _LIST_ENTRY,
    module_name: *const u16,
) -> Option<&'a _BLDR_DATA_TABLE_ENTRY> {
    let mut entry = load_order_list_head;
    debug!("Searching among loaded modules");

    let module_name = if module_name.is_null() {
        debug!("Module name is NULL");
        return None;
    } else {
        let mut size = 0;
        while unsafe { *module_name.add(size) } != 0 {
            size += 1;
        }

        unsafe { slice::from_raw_parts(module_name, size) }
    };

    debug!("module_name = {module_name:02X?}");
    loop {
        match unsafe { entry.as_ref() } {
            Some(e) => {
                entry = e.Flink as *const _LIST_ENTRY;
                if entry.is_null() || entry == load_order_list_head {
                    break None;
                }

                trace!("Navigated to next module list entry {entry:p}");
                let entry = entry
                    .wrapping_byte_sub(mem::offset_of!(_KLDR_DATA_TABLE_ENTRY, InLoadOrderLinks))
                    .wrapping_byte_sub(mem::offset_of!(_BLDR_DATA_TABLE_ENTRY, kldr_entry))
                    as *const _BLDR_DATA_TABLE_ENTRY;

                trace!("_BLDR_DATA_TABLE_ENTRY at {entry:p}");
                match unsafe { entry.as_ref() } {
                    Some(e) => {
                        let name = unsafe {
                            let name = (*entry).kldr_entry.BaseDllName;
                            if name.Buffer.is_null() {
                                &[]
                            } else {
                                slice::from_raw_parts(
                                    name.Buffer,
                                    usize::from(name.Length) / size_of::<u16>(),
                                )
                            }
                        };

                        trace!("name buffer = {name:02X?}");
                        // trace!("Current module name = {:?}", String::from_utf16_lossy(name)); // `alloc` is not always available

                        if name == module_name {
                            debug!("Found module with matching name");
                            break Some(e);
                        }
                    }
                    None => {
                        break None;
                    }
                }
            }
            None => {
                break None;
            }
        }
    }
}

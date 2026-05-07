pub mod mapper;
pub mod pe;
pub mod types;

use core::arch::asm;
use core::time::Duration;
use core::{mem, slice};

use log::info;
use types::{_BLDR_DATA_TABLE_ENTRY, _KLDR_DATA_TABLE_ENTRY, _LIST_ENTRY};
use uefi::boot;

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
    (0..buffer.len().saturating_sub(pattern.len()))
        .find(|&index| buffer[index..index + pattern.len()] == *pattern)
}

unsafe fn find_pe_image_impl(mut code: *const u8) -> Option<(*const u8, usize)> {
    loop {
        if code.is_null() {
            break None;
        }

        let nt_headers = unsafe { pe::get_nt_headers(code) };
        if nt_headers.is_null() {
            code = code.wrapping_byte_sub(1);
        } else {
            break Some(nt_headers);
        }
    }
    .map(|h| (code, unsafe { *h }.OptionalHeader.SizeOfImage as usize))
}

// pub unsafe fn find_pe_image<'a>(code: *const u8) -> Option<&'a [u8]> {
//     unsafe { find_pe_image_impl(code) }
//         .map(|(base, size)| unsafe { slice::from_raw_parts(base, size) })
// }

pub unsafe fn find_pe_image_mut<'a>(code: *mut u8) -> Option<&'a mut [u8]> {
    unsafe { find_pe_image_impl(code) }
        .map(|(base, size)| unsafe { slice::from_raw_parts_mut(base as *mut u8, size) })
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

pub fn get_function_code(start: *const u8, end: *const u8) -> &'static [u8] {
    let start_addr = start as usize;
    let end_addr = end as usize;

    unsafe { slice::from_raw_parts(start, end_addr.saturating_sub(start_addr)) }
}

const MODULE_NAME_MAX_LEN: usize = 32;

/// Reference: https://github.com/Mattiwatti/EfiGuard/blob/801ad43372021d3806ef1be22dddfd0fb860693b/EfiGuardDxe/PatchWinload.c#L57-L79
pub unsafe fn get_boot_loaded_module<'a>(
    load_order_list_head: *const _LIST_ENTRY,
    module_name: *const u16,
) -> Option<&'a _BLDR_DATA_TABLE_ENTRY> {
    let mut entry = load_order_list_head;

    let module_name = if module_name.is_null() {
        return None;
    } else {
        let mut size = 0;
        while unsafe { *module_name.add(size) } != 0 {
            size += 1;
        }

        unsafe { slice::from_raw_parts(module_name, size) }
    };

    loop {
        match unsafe { entry.as_ref() } {
            Some(e) => {
                entry = e.Flink;
                if entry.is_null() || entry == load_order_list_head {
                    break None;
                }

                let entry = entry
                    .wrapping_byte_sub(mem::offset_of!(_KLDR_DATA_TABLE_ENTRY, InLoadOrderLinks))
                    .wrapping_byte_sub(mem::offset_of!(_BLDR_DATA_TABLE_ENTRY, kldr_entry))
                    .cast::<_BLDR_DATA_TABLE_ENTRY>();

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

                        let mut wname = [0; MODULE_NAME_MAX_LEN];
                        let copy_len = name.len().min(wname.len());
                        wname[..copy_len].copy_from_slice(&name[..copy_len]);

                        let mut cname = [0; MODULE_NAME_MAX_LEN];
                        for (i, c) in char::decode_utf16(wname)
                            .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                            .enumerate()
                        {
                            cname[i] = c as u8;
                        }

                        if let Some(c) = cname.last_mut() {
                            *c = 0;
                        }

                        if name == module_name {
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

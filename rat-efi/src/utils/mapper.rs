/// Reference: https://github.com/memN0ps/redlotus-rs/blob/2d8c264f5f5e9e6fd859a266db8c355599108a21/bootkit/src/mapper/mod.rs
use core::ffi::CStr;
use core::{mem, slice};

use log::debug;
use windows_sys::Win32::System::Diagnostics::Debug::IMAGE_DIRECTORY_ENTRY_BASERELOC;
use windows_sys::Win32::System::SystemServices::{
    IMAGE_BASE_RELOCATION, IMAGE_REL_BASED_DIR64, IMAGE_REL_BASED_HIGHLOW,
};

use crate::utils::types::_LIST_ENTRY;
use crate::utils::{self, pe};

fn cstr_eq_ignore_ascii_case(a: &CStr, b: &[u8]) -> bool {
    let bytes = a.to_bytes();
    if bytes.len() != b.len() {
        return false;
    }

    bytes
        .iter()
        .zip(b.iter())
        .all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
}

unsafe fn resolve_export_address(module: &[u8], name: &CStr) -> Option<u64> {
    let mut resolved = None;

    unsafe {
        pe::iterate_export_address_table(module, |export_name, function| {
            if resolved.is_none() && export_name == name {
                resolved = Some(function.as_ptr() as u64);
            }
        });
    }

    resolved
}

unsafe fn process_relocation(new_image: &mut [u8], delta: i128) -> bool {
    match unsafe { pe::get_nt_headers(new_image.as_ptr()).as_ref() } {
        Some(nt_headers) => {
            if delta != 0 {
                let base_reloc_dir = nt_headers.OptionalHeader.DataDirectory
                    [IMAGE_DIRECTORY_ENTRY_BASERELOC as usize];

                // Reference:
                // - https://stackoverflow.com/a/22513813
                // - https://offensivecraft.wordpress.com/2022/04/19/pe-relocation-table/
                let relocations = &new_image[base_reloc_dir.VirtualAddress as usize..];
                debug!(
                    "DataDirectory[IMAGE_DIRECTORY_ENTRY_BASERELOC].VirtualAddress = 0x{:X}",
                    base_reloc_dir.VirtualAddress
                );
                let mut relocations = &relocations[..base_reloc_dir.Size as usize];

                let image_size = nt_headers.OptionalHeader.SizeOfImage as usize;
                while !relocations.is_empty()
                    && let Some(block) = unsafe {
                        relocations
                            .as_ptr()
                            .cast::<IMAGE_BASE_RELOCATION>()
                            .as_ref()
                    }
                {
                    let block_size = block.SizeOfBlock as usize;
                    if block_size < mem::size_of::<IMAGE_BASE_RELOCATION>() {
                        break;
                    }

                    let base_rva = block.VirtualAddress as usize;
                    let entries = unsafe {
                        slice::from_raw_parts(
                            relocations
                                .as_ptr()
                                .byte_add(mem::size_of::<IMAGE_BASE_RELOCATION>())
                                .cast::<u16>(),
                            (block_size - mem::size_of::<IMAGE_BASE_RELOCATION>())
                                / mem::size_of::<u16>(),
                        )
                    };
                    debug!(
                        "Relocation block: VirtualAddress=0x{base_rva:X}, SizeOfBlock=0x{block_size:X} ({} entries)",
                        entries.len(),
                    );

                    fn bound_check_and_process<T>(
                        image: *mut T,
                        image_size: usize,
                        rva: usize,
                        process: impl FnOnce(&mut T),
                    ) {
                        if rva + mem::size_of::<T>() <= image_size {
                            let target = unsafe { image.byte_add(rva) };
                            process(unsafe { &mut *target });
                        }
                    }

                    for (index, &entry) in entries.iter().enumerate() {
                        if entry == 0 {
                            break;
                        }

                        let offset = usize::from(entry) & 0xFFF;
                        let rva = base_rva.saturating_add(offset);
                        let reloc_type = entry >> 12;
                        debug!(
                            "Relocation entry #{}/{}: 0x{entry:X} (type=0x{reloc_type:X}, offset=0x{offset:X} -> rva=0x{rva:X})",
                            index + 1,
                            entries.len()
                        );

                        match u32::from(reloc_type) {
                            IMAGE_REL_BASED_HIGHLOW => {
                                bound_check_and_process(
                                    new_image.as_ptr() as *mut u32,
                                    image_size,
                                    rva,
                                    |word| {
                                        *word = word.wrapping_add_signed(delta as i32);
                                    },
                                );
                            }
                            IMAGE_REL_BASED_DIR64 => {
                                bound_check_and_process::<u64>(
                                    new_image.as_ptr() as *mut u64,
                                    image_size,
                                    rva,
                                    |qword| {
                                        *qword = qword.wrapping_add_signed(delta as i64);
                                    },
                                );
                            }
                            _ => {}
                        }
                    }

                    if block_size < relocations.len() {
                        relocations = &relocations[block_size..];
                    } else {
                        break;
                    }
                }
            }

            true
        }
        None => false,
    }
}

pub struct ManualMapping {
    pub size: usize,
    pub entrypoint: u32,
}

pub unsafe fn manual_map(
    image: &[u8],
    load_order_list_head: &_LIST_ENTRY,
    new_image: &mut [u8],
) -> Option<ManualMapping> {
    debug!(
        "Mapping image at {:p} (size {}) to buffer at {:p} (size {})",
        image.as_ptr(),
        image.len(),
        new_image.as_ptr(),
        new_image.len(),
    );
    debug!(
        "First 32 bytes of image: {:02X?}",
        &image[..image.len().min(32)]
    );

    unsafe {
        let nt_headers_ptr = pe::get_nt_headers(image.as_ptr());
        match nt_headers_ptr.as_ref() {
            Some(nt_headers) => {
                // Copy headers
                let headers_size = nt_headers.OptionalHeader.SizeOfHeaders as usize;
                if new_image.len() < headers_size {
                    return None;
                }
                new_image[..headers_size].copy_from_slice(&image[..headers_size]);

                let mut success = true;

                // Copy sections
                pe::iterate_sections_disk(image, |header, section| {
                    let rva = header.VirtualAddress as usize;
                    if rva < new_image.len() {
                        let target = &mut new_image[rva..];
                        if target.len() < section.len() {
                            success = false;
                            return;
                        }

                        target[..section.len()].copy_from_slice(section);
                        let total_size = target.len().min(header.Misc.VirtualSize as usize);
                        if total_size > section.len() {
                            target[section.len()..total_size].fill(0);
                        }
                    } else {
                        success = false;
                    }
                });

                if !success {
                    return None;
                }

                // Process relocations
                let delta =
                    new_image.as_ptr() as i128 - nt_headers.OptionalHeader.ImageBase as i128;
                debug!(
                    "delta = {:p} - 0x{:x} = 0x{delta:X} ({delta})",
                    new_image.as_ptr(),
                    nt_headers.OptionalHeader.ImageBase as i128,
                );
                process_relocation(new_image, delta);

                // Resolve imports from ntoskrnl using its export table.
                pe::iterate_import_address_table_mut(new_image, |module, name, thunk| {
                    if let Ok(module) = module.to_str() {
                        let mut buffer = [0; 64];
                        for (i, c) in module.encode_utf16().enumerate() {
                            if i + 1 == buffer.len() {
                                break;
                            }

                            buffer[i] = c;
                        }

                        if let Some(module) =
                            utils::get_boot_loaded_module(load_order_list_head, buffer.as_ptr())
                        {
                            let module = slice::from_raw_parts(
                                module.kldr_entry.DllBase.cast(),
                                module.kldr_entry.SizeOfImage as usize,
                            );
                            if let Some(address) = resolve_export_address(module, name) {
                                *thunk = address;
                                debug!("Resolved {module:?}!{name:?} to 0x{address:X}");
                            }
                        }
                    }
                });

                Some(ManualMapping {
                    size: nt_headers.OptionalHeader.SizeOfImage as usize,
                    entrypoint: nt_headers.OptionalHeader.AddressOfEntryPoint,
                })
            }
            None => None,
        }
    }
}

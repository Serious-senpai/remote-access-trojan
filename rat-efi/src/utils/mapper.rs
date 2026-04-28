/// Reference: https://github.com/memN0ps/redlotus-rs/blob/2d8c264f5f5e9e6fd859a266db8c355599108a21/bootkit/src/mapper/mod.rs
use core::ffi::CStr;
use core::{mem, slice};

use windows_sys::Win32::System::Diagnostics::Debug::IMAGE_DIRECTORY_ENTRY_BASERELOC;
use windows_sys::Win32::System::SystemServices::{
    IMAGE_BASE_RELOCATION, IMAGE_REL_BASED_DIR64, IMAGE_REL_BASED_HIGHLOW,
};

use crate::utils::pe;

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

unsafe fn resolve_export_address(ntoskrnl: &mut [u8], name: &CStr) -> Option<u64> {
    let mut resolved = None;

    unsafe {
        pe::iterate_export_address_table(ntoskrnl, |export_name, function| {
            if resolved.is_none() && export_name == name {
                resolved = Some(function.as_ptr() as u64);
            }
        });
    }

    resolved
}

unsafe fn process_relocation(new_image: &mut [u8], delta: isize) -> bool {
    match unsafe { pe::get_nt_headers(new_image.as_ptr()).as_ref() } {
        Some(nt_headers) => {
            if delta != 0 {
                let base_reloc_dir = nt_headers.OptionalHeader.DataDirectory
                    [IMAGE_DIRECTORY_ENTRY_BASERELOC as usize];

                // Reference:
                // - https://stackoverflow.com/a/22513813
                // - https://offensivecraft.wordpress.com/2022/04/19/pe-relocation-table/
                let relocations = &new_image[base_reloc_dir.VirtualAddress as usize..];
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
                    for &entry in entries {
                        if entry == 0 {
                            break;
                        }

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

                        let rva = base_rva.saturating_add(usize::from(entry) & 0x0FFF);
                        match u32::from(entry >> 12) {
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

                    relocations = &relocations[block_size..];
                }
            }

            true
        }
        None => false,
    }
}

pub unsafe fn manual_map(image: &[u8], ntoskrnl: &[u8], new_image: &mut [u8]) -> Option<usize> {
    unsafe {
        let nt_headers_ptr = pe::get_nt_headers(image.as_ptr());
        match nt_headers_ptr.as_ref() {
            Some(nt_headers) => {
                // Copy headers
                let headers_size = nt_headers.OptionalHeader.SizeOfHeaders as usize;
                new_image[..headers_size].copy_from_slice(&image[..headers_size]);

                // Copy sections
                pe::iterate_sections(image, |header, section| {
                    let target = &mut new_image[header.VirtualAddress as usize..];
                    target[..section.len()].copy_from_slice(section);
                });

                // Process relocations
                let delta =
                    new_image.as_ptr() as i128 - nt_headers.OptionalHeader.ImageBase as i128;
                process_relocation(new_image, delta as isize);

                let ntoskrnl_mut =
                    slice::from_raw_parts_mut(ntoskrnl.as_ptr() as *mut u8, ntoskrnl.len());

                // Resolve imports from ntoskrnl using its export table.
                pe::iterate_import_address_table_mut(new_image, |module, name, thunk| {
                    if !cstr_eq_ignore_ascii_case(module, b"ntoskrnl.exe")
                        && !cstr_eq_ignore_ascii_case(module, b"ntoskrnl")
                    {
                        return;
                    }

                    if let Some(address) = resolve_export_address(ntoskrnl_mut, name) {
                        *thunk = address;
                    }
                });

                Some(nt_headers.OptionalHeader.SizeOfImage as usize)
            }
            None => None,
        }
    }
}

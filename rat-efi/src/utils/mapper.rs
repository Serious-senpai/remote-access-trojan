/// Reference: https://github.com/memN0ps/redlotus-rs/blob/2d8c264f5f5e9e6fd859a266db8c355599108a21/bootkit/src/mapper/mod.rs
use core::mem;

use windows_sys::Win32::System::Diagnostics::Debug::{
    IMAGE_DIRECTORY_ENTRY_BASERELOC, IMAGE_DIRECTORY_ENTRY_IMPORT, IMAGE_SECTION_HEADER,
};
use windows_sys::Win32::System::SystemServices::{
    IMAGE_BASE_RELOCATION, IMAGE_IMPORT_BY_NAME, IMAGE_IMPORT_DESCRIPTOR, IMAGE_REL_BASED_DIR64,
    IMAGE_REL_BASED_HIGHLOW,
};
use windows_sys::Win32::System::WindowsProgramming::IMAGE_THUNK_DATA64;

use crate::utils::pe;

unsafe fn process_relocation(module_base: *mut u8, delta: isize) -> bool {
    match unsafe { pe::get_nt_headers(module_base).as_ref() } {
        Some(nt_headers) => {
            if delta != 0 {
                let mut base_relocation = unsafe {
                    module_base
                        .byte_add(
                            nt_headers.OptionalHeader.DataDirectory
                                [IMAGE_DIRECTORY_ENTRY_BASERELOC as usize]
                                .VirtualAddress as usize,
                        )
                        .cast::<IMAGE_BASE_RELOCATION>()
                };

                if base_relocation.is_null() {
                    return false;
                }

                let base_relocation_end = unsafe {
                    base_relocation.byte_add(
                        nt_headers.OptionalHeader.DataDirectory
                            [IMAGE_DIRECTORY_ENTRY_BASERELOC as usize]
                            .Size as usize,
                    )
                } as usize;

                while unsafe {
                    (base_relocation as usize) < base_relocation_end
                        && (*base_relocation).SizeOfBlock != 0
                } {
                    // TODO

                    base_relocation = unsafe {
                        base_relocation.byte_add((*base_relocation).SizeOfBlock as usize)
                    };
                }
            }

            true
        }
        None => false,
    }
}

pub unsafe fn manual_map(image: &[u8], ntoskrnl: &[u8], new_module_base: *mut u8) -> Option<usize> {
    unsafe {
        let nt_headers_ptr = pe::get_nt_headers(image.as_ptr());
        match nt_headers_ptr.as_ref() {
            Some(nt_headers) => {
                // Copy headers
                new_module_base.copy_from_nonoverlapping(
                    image.as_ptr(),
                    nt_headers.OptionalHeader.SizeOfHeaders as usize,
                );

                // Copy sections
                pe::iterate_sections(image, |header, section| {
                    let dest = new_module_base.byte_add(header.VirtualAddress as usize);
                    let src = image.as_ptr().byte_add(header.PointerToRawData as usize);
                    dest.copy_from_nonoverlapping(src, header.SizeOfRawData as usize); // TODO: Check
                });

                let delta = new_module_base as i128 - nt_headers.OptionalHeader.ImageBase as i128;

                unsafe {
                    // pe::iterate_import_address_table_mut();
                }

                Some(nt_headers.OptionalHeader.SizeOfImage as usize)
            }
            None => None,
        }
    }
}

use core::ffi::{CStr, c_char};
use core::{mem, ptr, slice};

use windows_sys::Win32::System::Diagnostics::Debug::{
    IMAGE_DIRECTORY_ENTRY_EXPORT, IMAGE_NT_HEADERS64, IMAGE_SECTION_HEADER,
};
use windows_sys::Win32::System::SystemServices::{IMAGE_DOS_HEADER, IMAGE_EXPORT_DIRECTORY};

use crate::utils::{DOS_SIGNATURE, NT_SIGNATURE};

pub unsafe fn get_nt_headers(image_base: *const u8) -> *const IMAGE_NT_HEADERS64 {
    let dos_header = image_base.cast::<IMAGE_DOS_HEADER>();
    if let Some(dos) = unsafe { dos_header.as_ref() } {
        if dos.e_magic == DOS_SIGNATURE {
            let nt_headers_offset = dos.e_lfanew as usize;
            let nt_headers_ptr =
                unsafe { image_base.byte_add(nt_headers_offset) }.cast::<IMAGE_NT_HEADERS64>();

            if let Some(nt_headers) = unsafe { nt_headers_ptr.as_ref() } {
                if nt_headers.Signature == NT_SIGNATURE {
                    return nt_headers_ptr;
                }
            }
        }
    }

    ptr::null_mut()
}

pub unsafe fn get_nt_headers_mut(image_base: *mut u8) -> *mut IMAGE_NT_HEADERS64 {
    unsafe { get_nt_headers(image_base) as *mut IMAGE_NT_HEADERS64 }
}

pub unsafe fn iterate_section_mut(
    image: &mut [u8],
    callback: impl Fn(IMAGE_SECTION_HEADER, &mut [u8]),
) {
    let nt_headers = unsafe { get_nt_headers(image.as_ptr()) };
    let nt_headers_offset = unsafe { nt_headers.cast::<u8>().offset_from(image.as_ptr()) } as usize;

    if let Some(nt_headers) = unsafe { nt_headers.as_ref() } {
        let section_headers_offset = nt_headers_offset
            + mem::size_of_val(&nt_headers.Signature)
            + mem::size_of_val(&nt_headers.FileHeader)
            + usize::from(nt_headers.FileHeader.SizeOfOptionalHeader);

        let num_sections = usize::from(nt_headers.FileHeader.NumberOfSections);
        let section_size = mem::size_of::<IMAGE_SECTION_HEADER>();

        for i in 0..num_sections {
            let section_offset = section_headers_offset + i * section_size;
            if section_offset + section_size > image.len() {
                break;
            }

            if let Some(section_header) = unsafe {
                image
                    .as_ptr()
                    .byte_add(section_offset)
                    .cast::<IMAGE_SECTION_HEADER>()
                    .as_ref()
            } {
                let virtual_address = section_header.VirtualAddress as usize;
                let virtual_size = unsafe { section_header.Misc.VirtualSize } as usize;

                if virtual_address + virtual_size <= image.len() {
                    callback(*section_header, unsafe {
                        slice::from_raw_parts_mut(
                            image.as_mut_ptr().byte_add(virtual_address),
                            virtual_size,
                        )
                    });
                }
            }
        }
    }
}

pub unsafe fn iterate_export_address_table_mut(
    image: &mut [u8],
    callback: impl Fn(&CStr, &mut [u8]),
) {
    if let Some(nt_headers) = unsafe { get_nt_headers(image.as_ptr()).as_ref() } {
        let export_dir =
            &nt_headers.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT as usize];

        let exports = image[export_dir.VirtualAddress as usize..]
            .as_ptr()
            .cast::<IMAGE_EXPORT_DIRECTORY>();
        if let Some(exports) = unsafe { exports.as_ref() } {
            let len = exports.NumberOfNames as usize;
            let names = unsafe {
                slice::from_raw_parts(
                    image[exports.AddressOfNames as usize..]
                        .as_ptr()
                        .cast::<u32>(),
                    len,
                )
            };
            let ordinals = unsafe {
                slice::from_raw_parts(
                    image[exports.AddressOfNameOrdinals as usize..]
                        .as_ptr()
                        .cast::<u16>(),
                    len,
                )
            };
            let functions = unsafe {
                slice::from_raw_parts(
                    image[exports.AddressOfFunctions as usize..]
                        .as_ptr()
                        .cast::<u32>(),
                    exports.NumberOfFunctions as usize,
                )
            };

            for i in 0..len {
                let name = image[names[i] as usize..].as_ptr().cast::<c_char>();
                let name = unsafe { CStr::from_ptr(name) };

                let rva = functions[ordinals[i as usize] as usize];
                if let Some(function) = image.get_mut(rva as usize..) {
                    callback(name, function);
                }
            }
        }
    }
}

use core::ffi::{CStr, c_char};
use core::{mem, ptr, slice};

use windows_sys::Win32::System::Diagnostics::Debug::{
    IMAGE_DIRECTORY_ENTRY_EXPORT, IMAGE_DIRECTORY_ENTRY_IMPORT, IMAGE_NT_HEADERS64,
    IMAGE_SECTION_HEADER,
};
use windows_sys::Win32::System::SystemServices::{
    IMAGE_DOS_HEADER, IMAGE_EXPORT_DIRECTORY, IMAGE_IMPORT_BY_NAME, IMAGE_IMPORT_DESCRIPTOR,
};
use windows_sys::Win32::System::WindowsProgramming::IMAGE_THUNK_DATA64;

pub const DOS_SIGNATURE: u16 = 0x5A4D; // "MZ"
pub const NT_SIGNATURE: u32 = 0x4550; // "PE\0\0"

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

// pub unsafe fn get_nt_headers_mut(image_base: *mut u8) -> *mut IMAGE_NT_HEADERS64 {
//     unsafe { get_nt_headers(image_base) as *mut IMAGE_NT_HEADERS64 }
// }

unsafe fn iterate_sections_impl(
    image: &[u8],
    mut callback: impl FnMut(IMAGE_SECTION_HEADER, *const u8, usize),
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
                let rva = section_header.VirtualAddress as usize;
                let size = section_header.SizeOfRawData as usize;

                if rva + size <= image.len() {
                    callback(
                        *section_header,
                        unsafe { image.as_ptr().byte_add(rva) },
                        size,
                    );
                }
            }
        }
    }
}

pub unsafe fn iterate_sections(
    image: &[u8],
    mut callback: impl FnMut(IMAGE_SECTION_HEADER, &[u8]),
) {
    unsafe {
        iterate_sections_impl(image, |header, ptr, len| {
            callback(header, slice::from_raw_parts(ptr, len))
        });
    }
}

// pub unsafe fn iterate_sections_mut(
//     image: &mut [u8],
//     mut callback: impl FnMut(IMAGE_SECTION_HEADER, &mut [u8]),
// ) {
//     unsafe {
//         iterate_sections_impl(image, |header, ptr, len| {
//             callback(header, slice::from_raw_parts_mut(ptr as *mut u8, len))
//         });
//     }
// }

unsafe fn iterate_export_address_table_impl(image: &[u8], mut callback: impl FnMut(&CStr, u32)) {
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
                callback(name, rva);
            }
        }
    }
}

pub unsafe fn iterate_export_address_table(image: &[u8], mut callback: impl FnMut(&CStr, &[u8])) {
    unsafe {
        iterate_export_address_table_impl(image, |name, rva| {
            if let Some(function) = image.get(rva as usize..) {
                callback(name, function);
            }
        });
    }
}

pub unsafe fn iterate_export_address_table_mut(
    image: &mut [u8],
    mut callback: impl FnMut(&CStr, &mut [u8]),
) {
    unsafe {
        iterate_export_address_table_impl(
            slice::from_raw_parts(image.as_ptr(), image.len()),
            |name, rva| {
                if let Some(function) = image.get_mut(rva as usize..) {
                    callback(name, function);
                }
            },
        );
    }
}

pub unsafe fn iterate_import_address_table_mut(
    image: &mut [u8],
    mut callback: impl FnMut(&CStr, &CStr, &mut u64),
) {
    if let Some(nt_headers) = unsafe { get_nt_headers(image.as_ptr()).as_ref() } {
        let import_dir =
            &nt_headers.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT as usize];

        if import_dir.VirtualAddress == 0 || import_dir.Size == 0 {
            return;
        }

        if import_dir.VirtualAddress as usize >= image.len() {
            return;
        }

        let mut imports_ptr = image[import_dir.VirtualAddress as usize..]
            .as_ptr()
            .cast::<IMAGE_IMPORT_DESCRIPTOR>();
        if imports_ptr.is_null() {
            return;
        }

        while unsafe { *imports_ptr }.Name != 0 {
            let imports = unsafe { *imports_ptr };
            if imports.Name as usize >= image.len() {
                imports_ptr = unsafe { imports_ptr.add(1) };
                continue;
            }

            let module_name_ptr = unsafe {
                image
                    .as_ptr()
                    .byte_add(imports.Name as usize)
                    .cast::<c_char>()
            };
            if module_name_ptr.is_null() {
                imports_ptr = unsafe { imports_ptr.add(1) };
                continue;
            }

            let module_name = unsafe { CStr::from_ptr(module_name_ptr) };
            let mut original_thunk = unsafe {
                let rva = if imports.Anonymous.OriginalFirstThunk == 0 {
                    imports.FirstThunk
                } else {
                    imports.Anonymous.OriginalFirstThunk
                };

                image[rva as usize..].as_ptr().cast::<IMAGE_THUNK_DATA64>()
            };

            if original_thunk.is_null() {
                break;
            }

            let mut thunk = unsafe { image.as_mut_ptr().byte_add(imports.FirstThunk as usize) }
                .cast::<IMAGE_THUNK_DATA64>();

            unsafe {
                while (*original_thunk).u1.Function != 0 {
                    let addr = (*original_thunk).u1.AddressOfData;
                    let is_ordinal = (addr & 0x8000_0000_0000_0000u64) != 0;

                    if !is_ordinal {
                        let addr = addr as usize;
                        if addr < image.len() {
                            let thunk_data =
                                image.as_ptr().byte_add(addr).cast::<IMAGE_IMPORT_BY_NAME>();

                            if let Some(thunk_data) = thunk_data.as_ref() {
                                let name = CStr::from_ptr(thunk_data.Name.as_ptr());
                                callback(module_name, name, &mut (*thunk).u1.Function)
                            }
                        }
                    }

                    thunk = thunk.add(1);
                    original_thunk = original_thunk.add(1);
                }
            }

            imports_ptr = unsafe { imports_ptr.add(1) };
        }
    }
}

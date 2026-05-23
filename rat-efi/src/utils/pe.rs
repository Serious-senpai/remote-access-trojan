use core::ffi::{CStr, c_char};
use core::{mem, ptr, slice};

use log::{debug, warn};
use windows_sys::Win32::System::Diagnostics::Debug::{
    IMAGE_DIRECTORY_ENTRY_EXPORT, IMAGE_DIRECTORY_ENTRY_IMPORT, IMAGE_NT_HEADERS64,
    IMAGE_SECTION_HEADER,
};
use windows_sys::Win32::System::SystemServices::{
    IMAGE_DOS_HEADER, IMAGE_EXPORT_DIRECTORY, IMAGE_IMPORT_BY_NAME, IMAGE_IMPORT_DESCRIPTOR,
    IMAGE_ORDINAL_FLAG64,
};
use windows_sys::Win32::System::WindowsProgramming::IMAGE_THUNK_DATA64;

pub const DOS_SIGNATURE: u16 = 0x5A4D; // "MZ"
pub const NT_SIGNATURE: u32 = 0x4550; // "PE\0\0"

pub unsafe fn get_nt_headers(image_base: *const u8) -> *const IMAGE_NT_HEADERS64 {
    let dos_header = image_base.cast::<IMAGE_DOS_HEADER>();
    if let Some(dos) = unsafe { dos_header.as_ref() }
        && dos.e_magic == DOS_SIGNATURE
    {
        let nt_headers_offset = dos.e_lfanew as usize;
        let nt_headers_ptr =
            unsafe { image_base.byte_add(nt_headers_offset) }.cast::<IMAGE_NT_HEADERS64>();

        if let Some(nt_headers) = unsafe { nt_headers_ptr.as_ref() }
            && nt_headers.Signature == NT_SIGNATURE
        {
            return nt_headers_ptr;
        }
    }

    ptr::null_mut()
}

// pub unsafe fn get_nt_headers_mut(image_base: *mut u8) -> *mut IMAGE_NT_HEADERS64 {
//     unsafe { get_nt_headers(image_base) as *mut IMAGE_NT_HEADERS64 }
// }

unsafe fn iterate_sections_disk_impl(
    image: &[u8],
    mut callback: impl FnMut(IMAGE_SECTION_HEADER, *const u8, usize),
) {
    debug!("Iterating sections of PE image (size=0x{:X})", image.len());
    let nt_headers = unsafe { get_nt_headers(image.as_ptr()) };
    let nt_headers_offset = unsafe { nt_headers.cast::<u8>().offset_from(image.as_ptr()) } as usize;

    if let Some(nt_headers) = unsafe { nt_headers.as_ref() } {
        let section_headers_offset = nt_headers_offset
            + mem::size_of_val(&nt_headers.Signature)
            + mem::size_of_val(&nt_headers.FileHeader)
            + usize::from(nt_headers.FileHeader.SizeOfOptionalHeader);

        let num_sections = usize::from(nt_headers.FileHeader.NumberOfSections);
        let section_size = mem::size_of::<IMAGE_SECTION_HEADER>();
        debug!("Number of sections: {num_sections}, section_size=0x{section_size:X}");

        for i in 0..num_sections {
            let section_offset = section_headers_offset + i * section_size;
            if section_offset + section_size > image.len() {
                debug!(
                    "Section header #{}/{num_sections} is out of bounds (offset=0x{section_offset:X})",
                    i + 1
                );
                break;
            }

            match unsafe {
                image
                    .as_ptr()
                    .byte_add(section_offset)
                    .cast::<IMAGE_SECTION_HEADER>()
                    .as_ref()
            } {
                Some(header) => {
                    let offset = header.PointerToRawData as usize;
                    let size = header.SizeOfRawData as usize;
                    let name = CStr::from_bytes_until_nul(&header.Name);
                    debug!(
                        "Section {name:?}: PointerToRawData=0x{offset:X}, SizeOfRawData=0x{:X}, VirtualSize=0x{:X}",
                        header.SizeOfRawData,
                        unsafe { header.Misc.VirtualSize },
                    );
                    if offset + size <= image.len() {
                        callback(*header, unsafe { image.as_ptr().byte_add(offset) }, size);
                    } else {
                        warn!("Section {name:?} is out of bounds");
                    }
                }
                None => {
                    warn!("Failed to read section header #{}/{}", i + 1, num_sections);
                }
            }
        }
    }
}

pub unsafe fn iterate_sections_disk(
    image: &[u8],
    mut callback: impl FnMut(IMAGE_SECTION_HEADER, &[u8]),
) {
    unsafe {
        iterate_sections_disk_impl(image, |header, ptr, len| {
            callback(header, slice::from_raw_parts(ptr, len))
        });
    }
}

// pub unsafe fn iterate_sections_disk_mut(
//     image: &mut [u8],
//     mut callback: impl FnMut(IMAGE_SECTION_HEADER, &mut [u8]),
// ) {
//     unsafe {
//         iterate_sections_disk_impl(image, |header, ptr, len| {
//             callback(header, slice::from_raw_parts_mut(ptr as *mut u8, len))
//         });
//     }
// }

unsafe fn iterate_sections_mem_impl(
    image: &[u8],
    mut callback: impl FnMut(IMAGE_SECTION_HEADER, *const u8, usize),
) {
    debug!("Iterating sections of PE image (size=0x{:X})", image.len());
    let nt_headers = unsafe { get_nt_headers(image.as_ptr()) };
    let nt_headers_offset = unsafe { nt_headers.cast::<u8>().offset_from(image.as_ptr()) } as usize;

    if let Some(nt_headers) = unsafe { nt_headers.as_ref() } {
        let section_headers_offset = nt_headers_offset
            + mem::size_of_val(&nt_headers.Signature)
            + mem::size_of_val(&nt_headers.FileHeader)
            + usize::from(nt_headers.FileHeader.SizeOfOptionalHeader);

        let num_sections = usize::from(nt_headers.FileHeader.NumberOfSections);
        let section_size = mem::size_of::<IMAGE_SECTION_HEADER>();
        debug!("Number of sections: {num_sections}, section_size=0x{section_size:X}");

        for i in 0..num_sections {
            let section_offset = section_headers_offset + i * section_size;
            if section_offset + section_size > image.len() {
                debug!(
                    "Section header #{}/{num_sections} is out of bounds (offset=0x{section_offset:X})",
                    i + 1
                );
                break;
            }

            match unsafe {
                image
                    .as_ptr()
                    .byte_add(section_offset)
                    .cast::<IMAGE_SECTION_HEADER>()
                    .as_ref()
            } {
                Some(header) => {
                    let offset = header.VirtualAddress as usize;
                    let size = unsafe { header.Misc.VirtualSize } as usize;
                    let name = CStr::from_bytes_until_nul(&header.Name);
                    debug!(
                        "Section {name:?}: VirtualAddress=0x{offset:X}, SizeOfRawData=0x{:X}, VirtualSize=0x{:X}",
                        header.SizeOfRawData,
                        unsafe { header.Misc.VirtualSize },
                    );
                    if offset + size <= image.len() {
                        callback(*header, unsafe { image.as_ptr().byte_add(offset) }, size);
                    } else {
                        warn!("Section {name:?} is out of bounds");
                    }
                }
                None => {
                    warn!("Failed to read section header #{}/{}", i + 1, num_sections);
                }
            }
        }
    }
}

pub unsafe fn iterate_sections_mem_mut(
    image: &mut [u8],
    mut callback: impl FnMut(IMAGE_SECTION_HEADER, &mut [u8]),
) {
    unsafe {
        iterate_sections_mem_impl(image, |header, ptr, len| {
            callback(header, slice::from_raw_parts_mut(ptr as *mut u8, len))
        });
    }
}

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

                let rva = functions[ordinals[i] as usize];
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

        let mut import_dir = image[import_dir.VirtualAddress as usize..]
            .as_ptr()
            .cast::<IMAGE_IMPORT_DESCRIPTOR>();
        if import_dir.is_null() {
            return;
        }

        while unsafe { *import_dir }.Name != 0 {
            let imports = &unsafe { *import_dir };
            let rva = imports.Name as usize;
            if rva < image.len()
                && let Ok(module_name) = CStr::from_bytes_until_nul(&image[rva..])
            {
                let try_original_first_thunk =
                    unsafe { imports.Anonymous.OriginalFirstThunk } as usize;
                let mut original_first_thunk = if try_original_first_thunk == 0 {
                    image[imports.FirstThunk as usize..].as_ptr()
                } else {
                    image[try_original_first_thunk..].as_ptr()
                }
                .cast::<IMAGE_THUNK_DATA64>();

                let mut first_thunk = image[imports.FirstThunk as usize..]
                    .as_ptr()
                    .cast::<IMAGE_THUNK_DATA64>();

                unsafe {
                    while (*original_first_thunk).u1.AddressOfData != 0 {
                        if (*original_first_thunk).u1.Ordinal & IMAGE_ORDINAL_FLAG64 == 0 {
                            let function = &*image
                                [(*original_first_thunk).u1.AddressOfData as usize..]
                                .as_ptr()
                                .cast::<IMAGE_IMPORT_BY_NAME>();
                            let function_name = CStr::from_ptr(function.Name.as_ptr());

                            let first_thunk_mut = first_thunk as *mut IMAGE_THUNK_DATA64;
                            callback(
                                module_name,
                                function_name,
                                &mut (*first_thunk_mut).u1.Function,
                            );
                        }

                        original_first_thunk = original_first_thunk.add(1);
                        first_thunk = first_thunk.add(1);
                    }
                }
            }

            import_dir = unsafe { import_dir.add(1) };
        }
    }
}

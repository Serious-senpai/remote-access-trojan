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

pub unsafe fn manual_map(image: &[u8], ntoskrnl: &[u8], new_module_base: *mut u8) -> Option<usize> {
    unsafe {
        let nt_headers_ptr = pe::get_nt_headers(image.as_ptr());
        match nt_headers_ptr.as_ref() {
            Some(nt_headers) => {
                new_module_base.copy_from_nonoverlapping(
                    image.as_ptr(),
                    nt_headers.OptionalHeader.SizeOfHeaders as usize,
                );

                let nt_headers_offset =
                    nt_headers_ptr.cast::<u8>().offset_from(image.as_ptr()) as usize;
                let section_headers_offset = nt_headers_offset
                    + mem::size_of_val(&nt_headers.Signature)
                    + mem::size_of_val(&nt_headers.FileHeader)
                    + nt_headers.FileHeader.SizeOfOptionalHeader as usize;

                let num_sections = nt_headers.FileHeader.NumberOfSections as usize;
                let section_size = mem::size_of::<IMAGE_SECTION_HEADER>();

                for i in 0..num_sections {
                    let section_offset = section_headers_offset + i * section_size;
                    if section_offset + section_size > image.len() {
                        break;
                    }

                    let section_header = &*image
                        .as_ptr()
                        .byte_add(section_offset)
                        .cast::<IMAGE_SECTION_HEADER>();

                    let dest = new_module_base.byte_add(section_header.VirtualAddress as usize);
                    let src = image
                        .as_ptr()
                        .byte_add(section_header.PointerToRawData as usize);
                    dest.copy_from_nonoverlapping(src, section_header.SizeOfRawData as usize); // TODO: Check
                }

                let delta = new_module_base as i128 - nt_headers.OptionalHeader.ImageBase as i128;
                if delta != 0 {
                    let mut base_relocation = image
                        .as_ptr()
                        .byte_add(
                            nt_headers.OptionalHeader.DataDirectory
                                [IMAGE_DIRECTORY_ENTRY_BASERELOC as usize]
                                .VirtualAddress as usize,
                        )
                        .cast::<IMAGE_BASE_RELOCATION>();

                    if base_relocation.is_null() {
                        return None;
                    }

                    let base_relocation_end = base_relocation.byte_add(
                        (*nt_headers).OptionalHeader.DataDirectory
                            [IMAGE_DIRECTORY_ENTRY_BASERELOC as usize]
                            .Size as usize,
                    ) as usize;

                    while (*base_relocation).VirtualAddress != 0
                        && ((*base_relocation).VirtualAddress as usize) < base_relocation_end
                    {
                        let address =
                            new_module_base.byte_add((*base_relocation).VirtualAddress as usize);

                        let item = base_relocation
                            .byte_add(mem::size_of::<IMAGE_BASE_RELOCATION>())
                            .cast::<u16>();
                        let count = ((*base_relocation).SizeOfBlock as usize
                            - size_of::<IMAGE_BASE_RELOCATION>())
                            / size_of::<u16>();

                        for i in 0..count {
                            let type_field = u32::from(*item.add(i) >> 12);
                            let offset = *item.add(i) & 0xFFF;

                            if type_field == IMAGE_REL_BASED_DIR64
                                || type_field == IMAGE_REL_BASED_HIGHLOW
                            {
                                *address.byte_add(offset.into()).cast::<isize>() += delta as isize;
                            }
                        }

                        base_relocation =
                            base_relocation.byte_add((*base_relocation).SizeOfBlock as usize);
                    }
                }

                let mut import_directory = image
                    .as_ptr()
                    .byte_add(
                        nt_headers.OptionalHeader.DataDirectory
                            [IMAGE_DIRECTORY_ENTRY_IMPORT as usize]
                            .VirtualAddress as usize,
                    )
                    .cast::<IMAGE_IMPORT_DESCRIPTOR>();

                if import_directory.is_null() {
                    return None;
                }

                while (*import_directory).Name != 0 {
                    let try_original_thunk = new_module_base
                        .byte_add((*import_directory).Anonymous.OriginalFirstThunk as usize);
                    let mut original_thunk = if try_original_thunk.is_null() {
                        new_module_base.byte_add((*import_directory).FirstThunk as usize)
                    } else {
                        try_original_thunk
                    }
                    .cast::<IMAGE_THUNK_DATA64>();

                    if original_thunk.is_null() {
                        return None;
                    }

                    let mut thunk = new_module_base
                        .byte_add((*import_directory).FirstThunk as usize)
                        .cast::<IMAGE_THUNK_DATA64>();

                    if thunk.is_null() {
                        return None;
                    }

                    while (*original_thunk).u1.Function != 0 {
                        let thunk_data = new_module_base
                            .byte_add((*original_thunk).u1.AddressOfData as usize)
                            .cast::<IMAGE_IMPORT_BY_NAME>();

                        thunk = thunk.add(1);
                        original_thunk = original_thunk.add(1);
                    }

                    // Increment and get a pointer to the next _IMAGE_IMPORT_DESCRIPTOR
                    import_directory = (import_directory as usize
                        + size_of::<IMAGE_IMPORT_DESCRIPTOR>() as usize)
                        as _;
                }

                Some(nt_headers.OptionalHeader.SizeOfImage as usize)
            }
            None => None,
        }
    }
}

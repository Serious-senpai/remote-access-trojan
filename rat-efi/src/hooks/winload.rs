use core::slice;

use log::{error, info, warn};

use crate::utils::find_pattern;

const SEARCH_BACKWARDS: isize = -1024; // 1 KB
const SEARCH_RANGE: usize = 2 * 1024 * 1024; // 2 MB

const ARCHPX64_TRANSFER_TO64_BIT_APPLICATION_ASM: &[[u8; 21]] = &[[
    0x00, 0x48, 0x89, 0x05, 0x6C, 0xEB, 0x1B, 0x00, // 8 bytes before
    0xE8, 0x07, 0xBC, 0x13, 0x00, // call Archpx64TransferTo64BitApplicationAsm
    0xE8, 0xD2, 0x87, 0x0C, 0x00, 0x84, 0xC0, 0x74, // 8 bytes after
]];

const NTOSKRNL_ENTRYPOINT: &[[u8; 32]] = &[[
    0x0F, 0x22, 0xD9, 0x48, 0x2B, 0xED, 0x48, 0x8B, // 8 bytes before
    0x25, 0x85, 0x2E, 0x08, 0x00, 0x48, 0x2B, 0xF6, // another 8 bytes
    0x48, 0x8B, 0x0D, 0x83, 0x2E, 0x08, 0x00, // mov rcx, cs:ArchpChildAppParameters
    0x48, 0x8B, 0x05, 0x9C, 0x2E, 0x08, 0x00, // mov rax, cs:ArchpChildAppEntryRoutine
    0xFF, 0xD0, // call rax ; ArchpChildAppEntryRoutine
]];

unsafe extern "efiapi" fn ntoskrnl_entrypoint_hooked() {}

pub fn patch_winload(entrypoint: *mut u8) {
    info!("Patching winload.efi...");
    let buffer = unsafe {
        slice::from_raw_parts_mut(entrypoint.wrapping_offset(SEARCH_BACKWARDS), SEARCH_RANGE)
    };

    let mut patched = false;
    for pattern in ARCHPX64_TRANSFER_TO64_BIT_APPLICATION_ASM {
        if let Some(offset) = find_pattern(buffer, pattern) {
            let modify = &mut buffer[offset + 8..];
            let offset = match usize::try_from(i32::from_le_bytes({
                let mut value = [0; 4];
                value.copy_from_slice(&modify[1..5]);
                value
            })) {
                Ok(addr) => addr + 5,
                Err(e) => {
                    warn!("Error while finding Archpx64TransferTo64BitApplicationAsm: {e}");
                    continue;
                }
            };

            let modify = &mut modify[offset..];
            for pattern in NTOSKRNL_ENTRYPOINT {
                if let Some(offset) = find_pattern(modify, pattern) {
                    // WIP
                }
            }

            info!("Found Archpx64TransferTo64BitApplicationAsm at offset {offset}");
            patched = true;
            break;
        }
    }

    if !patched {
        error!("Cannot find Archpx64TransferTo64BitApplicationAsm");
    }
}

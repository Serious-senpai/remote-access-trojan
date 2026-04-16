use core::ffi::c_int;
use core::mem;
use core::sync::atomic::{AtomicI64, Ordering};

use log::{error, info};

use crate::hooks::winload::patch_winload;
use crate::utils::find_pattern;

type BlpArchTransferTo64BitApplicationFn = unsafe extern "efiapi" fn(
    entrypoint: *mut u8,
    params: *mut u8,
    top_of_stack: *mut u8,
    page_table_base: *mut u8,
    flags: c_int,
    descriptor_table_context: *mut u8,
) -> i64;

static ORIGINAL_BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION: AtomicI64 = AtomicI64::new(0);

// const ARCHPX64_TRANSFER_TO64_BIT_APPLICATION_ASM: &[[u8; 21]] = &[
//     [
//         0x00, 0x48, 0x89, 0x05, 0x84, 0xD9, 0x07, 0x00, // 8 bytes before
//         0xE8, 0x8F, 0xE2, 0x03, 0x00, // call Archpx64TransferTo64BitApplicationAsm
//         0xE8, 0x42, 0xB9, 0xFB, 0xFF, 0x84, 0xC0, 0x74, // 8 bytes after
//     ],
//     [
//         0x00, 0x48, 0x89, 0x05, 0xC0, 0xD9, 0x07, 0x00, // 8 bytes before
//         0xE8, 0xFB, 0xE0, 0x03, 0x00, // call Archpx64TransferTo64BitApplicationAsm
//         0xE8, 0x42, 0xC6, 0xFB, 0xFF, 0x84, 0xC0, 0x74, // 8 bytes after
//     ],
// ];

const BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION: &[[u8; 21]] = &[[
    0x03, 0x4D, 0x67, 0x48, 0x89, 0x44, 0x24, 0x28, // 8 bytes before
    0xE8, 0xFE, 0x29, 0x08, 0x00, // call BlpArchTransferTo64BitApplication
    0x44, 0x8B, 0xF0, 0xE8, 0xA6, 0xBF, 0x00, 0x00, // 8 bytes after
]];

const BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION_PATCHED: &[u8] = &[
    0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // movabs rax, imm64
    0xFF, 0xD0, // call rax
];

const BM_FW_VERIFY_SELF_INTEGRITY: &[[u8; 17]] = &[[
    0xCC, // 1 byte before (int)
    0x89, 0x4C, 0x24, 0x08, 0x55, 0x53, 0x56, 0x57, // first 8 bytes
    0x41, 0x55, 0x41, 0x56, 0x48, 0x8B, 0xEC, 0x48, // next 8 bytes
]];

const BM_FW_VERIFY_SELF_INTEGRITY_PATCHED: &[u8] = &[
    0x48, 0x31, 0xC0, // xor rax, rax
    0xC3, // ret
];

unsafe extern "efiapi" fn blp_arch_transfer_to64_bit_application_hooked(
    entrypoint: *mut u8,
    params: *mut u8,
    top_of_stack: *mut u8,
    page_table_base: *mut u8,
    flags: c_int,
    descriptor_table_context: *mut u8,
) -> i64 {
    patch_winload(entrypoint);

    let original = ORIGINAL_BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION.load(Ordering::Acquire);
    unsafe {
        let original_fn = mem::transmute::<i64, BlpArchTransferTo64BitApplicationFn>(original);
        original_fn(
            entrypoint,
            params,
            top_of_stack,
            page_table_base,
            flags,
            descriptor_table_context,
        )
    }
}

pub fn patch_bootmgfw(buffer: &mut [u8]) {
    info!("Patching bootmgfw_old.efi...");

    let mut patched = false;
    for pattern in BM_FW_VERIFY_SELF_INTEGRITY {
        if let Some(offset) = find_pattern(buffer, pattern) {
            let modify = &mut buffer[offset + 1..];
            modify[..BM_FW_VERIFY_SELF_INTEGRITY_PATCHED.len()]
                .copy_from_slice(BM_FW_VERIFY_SELF_INTEGRITY_PATCHED);

            info!("Patched BmFwVerifySelfIntegrity at offset {offset}");
            patched = true;
            break;
        }
    }

    if !patched {
        error!("Cannot find BmFwVerifySelfIntegrity");
        return;
    }

    patched = false;
    for pattern in BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION {
        if let Some(offset) = find_pattern(buffer, pattern) {
            let modify = &mut buffer[offset + 8..];

            let original_call_addr = modify.as_ptr() as i64
                + 5
                + i64::from(i32::from_le_bytes({
                    let mut value = [0; 4];
                    value.copy_from_slice(&modify[1..5]);
                    value
                }));
            ORIGINAL_BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION
                .store(original_call_addr, Ordering::Release);

            // While patching this `call` instruction, we also override some instructions after it
            // because the original `call` only has 5 bytes but the patch to `call rax` is way
            // longer than that. However, we do not really need to patch back the instructions at
            // return address.
            // Why? Ideally, BlpArchTransferTo64BitApplication transfers control to winload.efi
            // (which eventually transfers to ntoskrnl.exe). Therefore, this function (and also its
            // hooked variant) should never return, and we do not need to fix return address
            // corruption (maybe?).
            let target_func_addr =
                blp_arch_transfer_to64_bit_application_hooked as *const u8 as i64;
            modify[..BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION_PATCHED.len()]
                .copy_from_slice(BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION_PATCHED);
            modify[2..10].copy_from_slice(&target_func_addr.to_le_bytes());

            info!("Patched BlpArchTransferTo64BitApplication at offset {offset}");
            patched = true;
            break;
        }
    }

    if !patched {
        error!("Cannot find BlpArchTransferTo64BitApplication");
    }
}

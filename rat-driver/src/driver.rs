use core::ffi::c_void;
use core::ptr;

use log::info;
use wdk::{self, nt_success};
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{KeDelayExecutionThread, PsCreateSystemThread};
use wdk_sys::{DRIVER_OBJECT, LARGE_INTEGER, NTSTATUS, PDRIVER_OBJECT, THREAD_ALL_ACCESS};
use widestring::Utf16Str;

use crate::log;

unsafe extern "C" fn thread_routine(context: *mut c_void) {
    let driver = context.cast::<DRIVER_OBJECT>();
    let mut sleep = LARGE_INTEGER {
        QuadPart: -50000000,
    };
    loop {
        log!("Thread running from driver {driver:p}");
        let status = unsafe { KeDelayExecutionThread(KernelMode as i8, 0, &mut sleep) };
        if !nt_success(status) {
            log!("KeDelayExecutionThread failed: 0x{status:X}");
            break;
        }
    }
}

pub fn driver_entry_prehook(
    driver: PDRIVER_OBJECT,
    registry_path: Option<&Utf16Str>,
) -> anyhow::Result<()> {
    info!("DriverEntry: {driver:p}, {registry_path:?}");
    log!("DriverEntry: {driver:p}, {registry_path:?}");
    let mut thread = ptr::null_mut();

    let status = unsafe {
        PsCreateSystemThread(
            &mut thread,
            THREAD_ALL_ACCESS,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            Some(thread_routine),
            driver.cast(),
        )
    };
    if !nt_success(status) {
        info!("PsCreateSystemThread failed: 0x{status:X}");
    }

    Ok(())
}

pub fn driver_entry_posthook(
    _: PDRIVER_OBJECT,
    _: Option<&Utf16Str>,
    status: NTSTATUS,
) -> anyhow::Result<()> {
    info!("Original DriverEntry returned with status: 0x{status:X}");

    Ok(())
}

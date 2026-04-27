#![no_std]

extern crate alloc;
extern crate wdk_panic;

mod log;
mod string;

use core::ffi::c_void;
use core::ptr;

use wdk::{self, nt_success};
use wdk_alloc::WdkAllocator;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{KeDelayExecutionThread, PsCreateSystemThread};
use wdk_sys::{
    DRIVER_OBJECT, LARGE_INTEGER, NTSTATUS, PCUNICODE_STRING, PDRIVER_OBJECT, STATUS_SUCCESS,
    THREAD_ALL_ACCESS,
};

#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;

unsafe extern "C" fn thread_routine(context: *mut c_void) {
    let driver = context.cast::<DRIVER_OBJECT>();
    let mut sleep = LARGE_INTEGER { QuadPart: 50000000 };
    loop {
        log!("Thread running from driver {driver:p}");
        let status = unsafe { KeDelayExecutionThread(KernelMode as i8, 0, &mut sleep) };
        if !nt_success(status) {
            log!("KeDelayExecutionThread failed: 0x{status:X}");
            break;
        }
    }
}

#[unsafe(export_name = "DriverEntry")]
pub unsafe extern "system" fn driver_entry(
    driver: PDRIVER_OBJECT,
    registry_path: PCUNICODE_STRING,
) -> NTSTATUS {
    let registry_path = match unsafe { registry_path.as_ref() } {
        Some(s) => unsafe { string::to_utf16str(s) },
        None => None,
    };

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
        log!("PsCreateSystemThread failed: 0x{status:X}");
    }

    STATUS_SUCCESS
}

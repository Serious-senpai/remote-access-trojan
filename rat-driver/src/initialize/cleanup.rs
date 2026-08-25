use core::ffi::c_void;
use core::mem;

use wdk::nt_success;
use wdk_sys::ntddk::{
    IoDeleteDevice, IoDeleteSymbolicLink, ObUnRegisterCallbacks, PsSetCreateProcessNotifyRoutineEx,
    RtlInitUnicodeString,
};
use wdk_sys::{HANDLE, PDEVICE_OBJECT, PEPROCESS, PPS_CREATE_NOTIFY_INFO, UNICODE_STRING};

use crate::global::DOS_NAME;
use crate::{info, warn};

pub fn cleanup_device(device: PDEVICE_OBJECT) {
    if !device.is_null() {
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
}

pub fn cleanup_process_notify_routine(routine: *const u8) {
    if !routine.is_null() {
        info!("Unregistering process notify routine: {routine:p}");
        unsafe {
            let _ = PsSetCreateProcessNotifyRoutineEx(
                Some(mem::transmute::<
                    *const u8,
                    unsafe extern "C" fn(PEPROCESS, HANDLE, PPS_CREATE_NOTIFY_INFO),
                >(routine)),
                1,
            );
        }
    }
}

pub fn cleanup_object_callbacks(handle: *mut c_void) {
    if !handle.is_null() {
        info!("Unregistering object callbacks");
        unsafe {
            ObUnRegisterCallbacks(handle);
        }
    }
}

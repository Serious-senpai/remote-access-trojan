use alloc::boxed::Box;
use core::ffi::c_void;

use aho_corasick::AhoCorasick;
use wdk::nt_success;
use wdk_sys::ntddk::{
    IoDeleteDevice, IoDeleteSymbolicLink, ObUnRegisterCallbacks, RtlInitUnicodeString,
};
use wdk_sys::{PDEVICE_OBJECT, UNICODE_STRING};

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

pub fn cleanup_object_callbacks(handle: *mut c_void) {
    if !handle.is_null() {
        info!("Unregistering object callbacks");
        unsafe {
            ObUnRegisterCallbacks(handle);
        }
    }
}

pub fn cleanup_aho_corasick(ac: *mut AhoCorasick) {
    if !ac.is_null() {
        info!("Dropping Aho-Corasick automaton");
        unsafe {
            let _ = Box::from_raw(ac);
        }
    }
}

use core::ffi::c_void;

use wdk_sys::ntddk::{RtlInitUnicodeString, ZwSetValueKey};
use wdk_sys::{HANDLE, NTSTATUS, REG_DWORD, ULONG, UNICODE_STRING};
use widestring::U16CStr;

pub fn registry_write_dword(key: HANDLE, name: &U16CStr, mut value: ULONG) -> NTSTATUS {
    let mut u_name = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut u_name, name.as_ptr());
        ZwSetValueKey(
            key,
            &mut u_name,
            0,
            REG_DWORD,
            &mut value as *mut ULONG as *mut c_void,
            size_of::<ULONG>() as u32,
        )
    }
}

pub fn registry_write_string(
    key: HANDLE,
    name: &U16CStr,
    value: &U16CStr,
    reg_type: ULONG,
) -> NTSTATUS {
    let mut u_name = UNICODE_STRING::default();
    let mut u_value = UNICODE_STRING::default();

    unsafe {
        RtlInitUnicodeString(&mut u_name, name.as_ptr());
        RtlInitUnicodeString(&mut u_value, value.as_ptr());

        // Registry strings expect the DataSize to include the NULL terminator (2 bytes for UTF-16).
        // u_value.Length is the size in bytes of the string characters.
        let data_size = u32::from(u_value.Length) + size_of::<u16>() as u32;

        ZwSetValueKey(
            key,
            &mut u_name,
            0,
            reg_type,
            u_value.Buffer as *mut c_void,
            data_size,
        )
    }
}

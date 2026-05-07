use core::ffi::{c_int, c_void};
use core::sync::atomic::Ordering;
use core::{ptr, slice};

use wdk::nt_success;
use wdk_sys::_REG_NOTIFY_CLASS::{
    RegNtPreDeleteKey, RegNtPreDeleteValueKey, RegNtPreRenameKey, RegNtPreReplaceKey,
    RegNtPreSaveKey, RegNtPreSetInformationKey, RegNtPreSetValueKey, RegNtPreUnLoadKey,
};
use wdk_sys::ntddk::{
    CmCallbackGetKeyObjectIDEx, CmRegisterCallbackEx, PsGetCurrentProcessId, RtlInitUnicodeString,
};
use wdk_sys::{
    LARGE_INTEGER, NTSTATUS, PDRIVER_OBJECT, REG_DELETE_KEY_INFORMATION,
    REG_DELETE_VALUE_KEY_INFORMATION, REG_RENAME_KEY_INFORMATION, REG_REPLACE_KEY_INFORMATION,
    REG_SAVE_KEY_INFORMATION, REG_SET_INFORMATION_KEY_INFORMATION, REG_SET_VALUE_KEY_INFORMATION,
    REG_UNLOAD_KEY_INFORMATION, STATUS_ACCESS_DENIED, STATUS_SUCCESS, UNICODE_STRING,
};

use crate::global::{ALTITUDE, CM_REGISTER_CALLBACK_COOKIE, SERVICE_REGISTRY_AHO_CORASICK};
use crate::info;

pub fn cm_register_callback(driver: PDRIVER_OBJECT) -> anyhow::Result<LARGE_INTEGER> {
    let mut altitude = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut altitude, ALTITUDE.as_ptr());
    }

    let mut cookie = LARGE_INTEGER::default();
    let status = unsafe {
        CmRegisterCallbackEx(
            Some(registry_callback),
            &altitude,
            driver.cast(),
            ptr::null_mut(),
            &mut cookie,
            ptr::null_mut(),
        )
    };

    anyhow::ensure!(
        nt_success(status),
        "CmRegisterCallbackEx error: 0x{status:X}"
    );
    Ok(cookie)
}

/// References:
/// - https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/filtering-registry-calls
/// - https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/registering-for-notifications
/// - https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/handling-notifications
/// - https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/supporting-layered-registry-filtering-drivers
/// - https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/obtaining-additional-registry-information
/// - https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/invalid-key-object-pointers-in-registry-notifications
unsafe extern "C" fn registry_callback(
    _: *mut c_void,
    notify_cls: *mut c_void,
    info: *mut c_void,
) -> NTSTATUS {
    macro_rules! parse_notify_info {
        ($t:ty) => {
            info.cast::<$t>().as_ref().map(|i| i.Object)
        };
    }

    let pid = unsafe { PsGetCurrentProcessId() } as u64;
    if pid <= 4 {
        return STATUS_SUCCESS;
    }

    let notify_cls = notify_cls as c_int;
    let object = unsafe {
        match notify_cls {
            RegNtPreDeleteKey => parse_notify_info!(REG_DELETE_KEY_INFORMATION),
            RegNtPreSetValueKey => parse_notify_info!(REG_SET_VALUE_KEY_INFORMATION),
            RegNtPreDeleteValueKey => parse_notify_info!(REG_DELETE_VALUE_KEY_INFORMATION),
            RegNtPreSetInformationKey => parse_notify_info!(REG_SET_INFORMATION_KEY_INFORMATION),
            RegNtPreRenameKey => parse_notify_info!(REG_RENAME_KEY_INFORMATION),
            RegNtPreUnLoadKey => parse_notify_info!(REG_UNLOAD_KEY_INFORMATION),
            RegNtPreSaveKey => parse_notify_info!(REG_SAVE_KEY_INFORMATION),
            RegNtPreReplaceKey => parse_notify_info!(REG_REPLACE_KEY_INFORMATION),
            _ => return STATUS_SUCCESS,
        }
    };

    if let Some(object) = object
        && !object.is_null()
    {
        let cookie = CM_REGISTER_CALLBACK_COOKIE.load(Ordering::Acquire);
        if cookie != 0 {
            let mut cookie = LARGE_INTEGER { QuadPart: cookie };
            let mut name = ptr::null();
            let status = unsafe {
                CmCallbackGetKeyObjectIDEx(&mut cookie, object, ptr::null_mut(), &mut name, 0)
            };
            if nt_success(status)
                && let Some(name) = unsafe { name.as_ref() }
                && let Some(ac) = unsafe {
                    SERVICE_REGISTRY_AHO_CORASICK
                        .load(Ordering::Acquire)
                        .as_ref()
                }
                && // aho-corasick supports &[u8] only, so we match 2 &[u8] slices against each other
                ac
                    .find(unsafe { slice::from_raw_parts(name.Buffer.cast(), name.Length.into()) })
                    .is_some()
            {
                info!("Blocking registry operation from process {pid} (class {notify_cls})");
                return STATUS_ACCESS_DENIED;
            }
        }
    }

    STATUS_SUCCESS
}

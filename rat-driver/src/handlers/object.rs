use alloc::string::String;
use core::ffi::c_void;
use core::{ptr, slice};

use rat_common::utils::DropGuard;
use wdk::nt_success;
use wdk_sys::_MODE::UserMode;
use wdk_sys::_OB_PREOP_CALLBACK_STATUS::OB_PREOP_SUCCESS;
use wdk_sys::ntddk::{
    ExFreePool, ExGetPreviousMode, IoGetCurrentProcess, ObRegisterCallbacks, RtlInitUnicodeString,
    SeLocateProcessImageName,
};
use wdk_sys::{
    OB_CALLBACK_REGISTRATION, OB_FLT_REGISTRATION_VERSION, OB_OPERATION_HANDLE_CREATE,
    OB_OPERATION_REGISTRATION, OB_PRE_OPERATION_INFORMATION, OB_PREOP_CALLBACK_STATUS,
    PsProcessType, UNICODE_STRING,
};
use widestring::u16cstr;

use crate::trace;

pub fn ob_register_callbacks() -> anyhow::Result<()> {
    let mut altitude = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut altitude, u16cstr!("360000").as_ptr());
    }

    let mut object_operations = [OB_OPERATION_REGISTRATION {
        ObjectType: unsafe { PsProcessType },
        Operations: OB_OPERATION_HANDLE_CREATE,
        PreOperation: Some(process_preop_callback),
        PostOperation: None,
    }];
    let mut object_callbacks = OB_CALLBACK_REGISTRATION {
        Version: OB_FLT_REGISTRATION_VERSION as u16,
        OperationRegistrationCount: object_operations.len() as u16,
        Altitude: altitude,
        RegistrationContext: ptr::null_mut(),
        OperationRegistration: object_operations.as_mut_ptr(),
    };

    let mut handle = ptr::null_mut();
    let status = unsafe { ObRegisterCallbacks(&mut object_callbacks, &mut handle) };
    anyhow::ensure!(
        nt_success(status),
        "ObRegisterCallbacks failed: 0x{status:X}",
    );

    Ok(())
}

unsafe extern "C" fn process_preop_callback(
    _: *mut c_void,
    info: *mut OB_PRE_OPERATION_INFORMATION,
) -> OB_PREOP_CALLBACK_STATUS {
    if let Some(info) = unsafe { info.as_mut() }
        && unsafe {
            info.__bindgen_anon_1.__bindgen_anon_1.KernelHandle() == 0
                && i32::from(ExGetPreviousMode()) == UserMode
        }
    {
        let mut source = ptr::null_mut();
        let status = unsafe { SeLocateProcessImageName(IoGetCurrentProcess(), &mut source) };
        let guard = DropGuard::new(source, |s| unsafe {
            ExFreePool(s.cast());
        });

        if let Some(source) = unsafe { source.as_ref() }
            && nt_success(status)
            && !info.Object.is_null()
        {
            let mut target = ptr::null_mut();
            let status = unsafe { SeLocateProcessImageName(info.Object.cast(), &mut target) };
            let guard = DropGuard::new(target, |s| unsafe {
                ExFreePool(s.cast());
            });

            if let Some(target) = unsafe { target.as_ref() }
                && nt_success(status)
            {
                let source = String::from_utf16_lossy(unsafe {
                    slice::from_raw_parts(source.Buffer, source.Length.into())
                });
                let target = String::from_utf16_lossy(unsafe {
                    slice::from_raw_parts(target.Buffer, target.Length.into())
                });
                trace!("Process object: {source:?} -> {target:?}");
            }

            drop(guard);
        }

        drop(guard);
    }

    OB_PREOP_SUCCESS
}

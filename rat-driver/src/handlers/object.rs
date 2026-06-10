use alloc::vec;
use core::ffi::c_void;
use core::sync::atomic::Ordering;
use core::{mem, ptr, slice};

use rat_common::utils::DropGuard;
use rat_common::windows::kernel::KernelHandoff;
use rat_common::windows::{PROCESS_PROTECTED_ACCESS, THREAD_PROTECTED_ACCESS};
use wdk::nt_success;
use wdk_sys::_OB_PREOP_CALLBACK_STATUS::OB_PREOP_SUCCESS;
use wdk_sys::ntddk::{
    ExFreePool, IoGetCurrentProcess, IoThreadToProcess, ObRegisterCallbacks, PsGetCurrentProcessId,
    RtlInitUnicodeString, SeLocateProcessImageName,
};
use wdk_sys::{
    OB_CALLBACK_REGISTRATION, OB_FLT_REGISTRATION_VERSION, OB_OPERATION_HANDLE_CREATE,
    OB_OPERATION_REGISTRATION, OB_PRE_OPERATION_INFORMATION, OB_PREOP_CALLBACK_STATUS, PEPROCESS,
    PsProcessType, PsThreadType, UNICODE_STRING,
};

use crate::global::{ALTITUDE, OBJ_PATH_AHO_CORASICK, SELF_DEFENSE_ACTIVATED};
use crate::info;

type _ObPreOperationCallbackFn =
    unsafe extern "C" fn(*mut c_void, *mut OB_PRE_OPERATION_INFORMATION) -> i32;

pub fn ob_register_callbacks(extra: &KernelHandoff) -> anyhow::Result<*mut c_void> {
    let mut altitude = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut altitude, ALTITUDE.as_ptr());
    }

    let mut object_operations = vec![];
    if !extra.process_preop_trampoline.is_null() {
        object_operations.push(OB_OPERATION_REGISTRATION {
            ObjectType: unsafe { PsProcessType },
            Operations: OB_OPERATION_HANDLE_CREATE,
            PreOperation: Some(unsafe {
                mem::transmute::<*const u8, _ObPreOperationCallbackFn>(
                    extra.process_preop_trampoline,
                )
            }),
            PostOperation: None,
        });
    }
    if !extra.thread_preop_trampoline.is_null() {
        object_operations.push(OB_OPERATION_REGISTRATION {
            ObjectType: unsafe { PsThreadType },
            Operations: OB_OPERATION_HANDLE_CREATE,
            PreOperation: Some(unsafe {
                mem::transmute::<*const u8, _ObPreOperationCallbackFn>(
                    extra.thread_preop_trampoline,
                )
            }),
            PostOperation: None,
        });
    }

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

#[unsafe(export_name = "ProcessPreopCallback")]
unsafe extern "C" fn process_preop_callback(
    _: *mut c_void,
    info: *mut OB_PRE_OPERATION_INFORMATION,
) -> OB_PREOP_CALLBACK_STATUS {
    if !SELF_DEFENSE_ACTIVATED.load(Ordering::Acquire) {
        return OB_PREOP_SUCCESS;
    }

    if let Some(info) = unsafe { info.as_mut() } {
        let process = info.Object.cast();
        if unsafe { IoGetCurrentProcess() == process } {
            return OB_PREOP_SUCCESS;
        }

        if _is_protected_process(process)
            && let Some(parameters) = unsafe { info.Parameters.as_mut() }
        {
            let access = unsafe { &mut parameters.CreateHandleInformation.DesiredAccess };
            if *access & PROCESS_PROTECTED_ACCESS != 0 {
                *access &= !PROCESS_PROTECTED_ACCESS;

                let pid = unsafe { PsGetCurrentProcessId() } as u64;
                info!("Modified process operation from process {pid}");
            }
        }
    }

    OB_PREOP_SUCCESS
}

#[unsafe(export_name = "ThreadPreopCallback")]
unsafe extern "C" fn thread_preop_callback(
    _: *mut c_void,
    info: *mut OB_PRE_OPERATION_INFORMATION,
) -> OB_PREOP_CALLBACK_STATUS {
    if !SELF_DEFENSE_ACTIVATED.load(Ordering::Acquire) {
        return OB_PREOP_SUCCESS;
    }

    if let Some(info) = unsafe { info.as_mut() } {
        let process = unsafe { IoThreadToProcess(info.Object.cast()) };
        if unsafe { IoGetCurrentProcess() == process } {
            return OB_PREOP_SUCCESS;
        }

        if _is_protected_process(process)
            && let Some(parameters) = unsafe { info.Parameters.as_mut() }
        {
            let access = unsafe { &mut parameters.CreateHandleInformation.DesiredAccess };
            if *access & THREAD_PROTECTED_ACCESS != 0 {
                *access &= !THREAD_PROTECTED_ACCESS;

                let pid = unsafe { PsGetCurrentProcessId() } as u64;
                info!("Modified thread operation from process {pid}");
            }
        }
    }

    OB_PREOP_SUCCESS
}

fn _is_protected_process(process: PEPROCESS) -> bool {
    if process.is_null() {
        return false;
    }

    let ac = match unsafe { OBJ_PATH_AHO_CORASICK.load(Ordering::Acquire).as_ref() } {
        Some(ac) => ac,
        None => return false,
    };

    let mut ctarget = ptr::null_mut();
    let status = unsafe { SeLocateProcessImageName(process, &mut ctarget) };

    if nt_success(status)
        && let Some(target) = unsafe { ctarget.as_ref() }
    {
        let guard = DropGuard::new(ctarget, |s| unsafe {
            ExFreePool(s.cast());
        });

        let target =
            unsafe { slice::from_raw_parts(target.Buffer.cast(), usize::from(target.Length)) };

        if ac.find(target).is_some() {
            return true;
        }

        drop(guard);
    }

    false
}

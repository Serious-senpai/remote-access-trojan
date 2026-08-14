use core::sync::atomic::{AtomicPtr, Ordering};
use core::{ptr, slice};

use aho_corasick::AhoCorasick;
use rat_common::utils::DropGuard;
use wdk::nt_success;
use wdk_sys::PEPROCESS;
use wdk_sys::ntddk::{ExFreePool, SeLocateProcessImageName};

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

use core::ffi::c_void;
use core::{ptr, slice};

use wdk_sys::_LOCK_OPERATION::IoReadAccess;
use wdk_sys::_MEMORY_CACHING_TYPE::MmCached;
use wdk_sys::_MM_PAGE_PRIORITY::HighPagePriority;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::MDL;
use wdk_sys::ntddk::{
    IoAllocateMdl, IoFreeMdl, MmMapLockedPagesSpecifyCache, MmProbeAndLockPages, MmUnlockPages,
    MmUnmapLockedPages,
};

pub struct MdlGuard {
    _mdl: *mut MDL,
    _mapped_address: *mut c_void,
    _len: u32,
}

impl MdlGuard {
    pub unsafe fn new(virtual_address: *mut c_void, len: u32) -> anyhow::Result<Self> {
        let mut this = Self {
            _mdl: ptr::null_mut(),
            _mapped_address: ptr::null_mut(),
            _len: len,
        };
        this._mdl = unsafe { IoAllocateMdl(virtual_address, len, 0, 0, ptr::null_mut()) };
        anyhow::ensure!(!this._mdl.is_null(), "Failed to allocate MDL");

        let kernel_mode = KernelMode as i8;
        let high_page_priority = HighPagePriority as u32;

        // Lock pages in memory.
        unsafe {
            MmProbeAndLockPages(this._mdl, kernel_mode, IoReadAccess);
        }

        // Map the locked pages to a new, writable virtual address.
        this._mapped_address = unsafe {
            MmMapLockedPagesSpecifyCache(
                this._mdl,
                kernel_mode,
                MmCached,
                ptr::null_mut(),
                0,
                high_page_priority,
            )
        };

        anyhow::ensure!(
            !this._mapped_address.is_null(),
            "Failed to map locked pages",
        );

        Ok(this)
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self._mapped_address.cast(), self._len as usize) }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self._mapped_address.cast(), self._len as usize) }
    }
}

impl Drop for MdlGuard {
    fn drop(&mut self) {
        if !self._mapped_address.is_null() {
            unsafe {
                MmUnmapLockedPages(self._mapped_address, self._mdl);
            }
        }

        if !self._mdl.is_null() {
            unsafe {
                MmUnlockPages(self._mdl);
                IoFreeMdl(self._mdl)
            }
        }
    }
}

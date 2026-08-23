use core::ffi::c_void;
use core::ptr;

use wdk::nt_success;
use wdk_sys::ntddk::{RtlInitUnicodeString, ZwClose, ZwCreateFile, ZwReadFile, ZwWriteFile};
use wdk_sys::{
    FILE_ATTRIBUTE_NORMAL, FILE_OPEN, FILE_SUPERSEDE, FILE_SYNCHRONOUS_IO_NONALERT, GENERIC_READ,
    GENERIC_WRITE, HANDLE, IO_STATUS_BLOCK, OBJ_CASE_INSENSITIVE, OBJ_KERNEL_HANDLE,
    OBJECT_ATTRIBUTES, STATUS_END_OF_FILE, SYNCHRONIZE, ULONG_PTR, UNICODE_STRING,
};

use crate::wrappers::bindings::InitializeObjectAttributes;

pub struct File {
    _handle: HANDLE,
}

impl File {
    fn create_internal(
        path: *const u16,
        desired_access: u32,
        create_disposition: u32,
    ) -> anyhow::Result<Self> {
        let mut u_path = UNICODE_STRING::default();
        let mut object_attributes = OBJECT_ATTRIBUTES::default();
        unsafe {
            RtlInitUnicodeString(&mut u_path, path);
            InitializeObjectAttributes(
                &mut object_attributes,
                &mut u_path,
                OBJ_CASE_INSENSITIVE | OBJ_KERNEL_HANDLE,
                ptr::null_mut(),
                ptr::null_mut(),
            );
        }

        let mut handle = HANDLE::default();
        let mut status_block = IO_STATUS_BLOCK::default();
        let status = unsafe {
            ZwCreateFile(
                &mut handle,
                desired_access | SYNCHRONIZE,
                &mut object_attributes,
                &mut status_block,
                ptr::null_mut(),
                FILE_ATTRIBUTE_NORMAL,
                0,
                create_disposition,
                FILE_SYNCHRONOUS_IO_NONALERT,
                ptr::null_mut(),
                0,
            )
        };

        anyhow::ensure!(nt_success(status), "ZwCreateFile error: 0x{status:X}");
        Ok(Self { _handle: handle })
    }

    pub fn create(path: *const u16) -> anyhow::Result<Self> {
        Self::create_internal(path, GENERIC_WRITE, FILE_SUPERSEDE)
    }

    pub fn open(path: *const u16) -> anyhow::Result<Self> {
        Self::create_internal(path, GENERIC_READ, FILE_OPEN)
    }

    pub fn write(&self, buffer: &[u8]) -> anyhow::Result<ULONG_PTR> {
        let mut status_block = IO_STATUS_BLOCK::default();
        let length = buffer.len().try_into().unwrap_or(u32::MAX);

        let status = unsafe {
            ZwWriteFile(
                self._handle,
                ptr::null_mut(),
                None,
                ptr::null_mut(),
                &mut status_block,
                buffer.as_ptr() as *mut c_void,
                length,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };

        anyhow::ensure!(nt_success(status), "ZwWriteFile error: 0x{status:X}");
        Ok(status_block.Information)
    }

    pub fn read(&self, buffer: &mut [u8]) -> anyhow::Result<ULONG_PTR> {
        let mut status_block = IO_STATUS_BLOCK::default();
        let length = buffer.len().try_into().unwrap_or(u32::MAX);

        let status = unsafe {
            ZwReadFile(
                self._handle,
                ptr::null_mut(),
                None,
                ptr::null_mut(),
                &mut status_block,
                buffer.as_mut_ptr().cast(),
                length,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };

        if status == STATUS_END_OF_FILE {
            return Ok(0);
        }

        anyhow::ensure!(nt_success(status), "ZwReadFile error: 0x{status:X}");
        Ok(status_block.Information)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe {
            let _ = ZwClose(self._handle);
        }
    }
}

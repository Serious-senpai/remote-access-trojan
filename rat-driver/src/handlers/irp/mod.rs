mod cleanup;
mod create;
mod wmi;

use core::ptr;

use cleanup::CleanupHandler;
use wdk::nt_success;
use wdk_sys::ntddk::{
    IoCreateDevice, IoCreateSymbolicLink, IofCompleteRequest, RtlInitUnicodeString,
};
use wdk_sys::{
    DEVICE_OBJECT, DO_BUFFERED_IO, DO_DEVICE_INITIALIZING, FILE_DEVICE_SECURE_OPEN,
    FILE_DEVICE_UNKNOWN, IO_NO_INCREMENT, IO_STACK_LOCATION, IRP, NTSTATUS, PDEVICE_OBJECT,
    PDRIVER_OBJECT, PIRP, STATUS_INVALID_DEVICE_REQUEST, STATUS_INVALID_PARAMETER, STATUS_SUCCESS,
    STATUS_UNSUCCESSFUL, UNICODE_STRING,
};

use crate::global::{DEVICE_NAME, DOS_NAME};
use crate::handlers::irp::create::CreateHandler;
use crate::handlers::irp::wmi::WMIHandler;
use crate::wrappers::bindings::IoGetCurrentIrpStackLocation;
use crate::{debug, error};

/// Trait for handling IRP requests.
///
/// Each time an IRP request is received, an instance of a type implementing this trait
/// will be created to handle the request.
pub trait IrpHandler<'a> {
    /// The IRP major function code this handler is responsible for.
    const CODE: u32;

    /// Create a new instance of the handler.
    ///
    /// Implementations should use this method to populate necessary fields of their own
    /// (note that [`IrpHandler::handle()`] does not take any arguments).
    fn new(
        device: &'a DEVICE_OBJECT,
        irp: &'a mut IRP,
        irpsp: &'a mut IO_STACK_LOCATION,
    ) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Handle the IRP request.
    ///
    /// Implementations should set the appropriate fields in the [`IRP::IoStatus`] structure, except
    /// `Status`, as that field will be set automatically by the framework afterwards.
    fn handle(&mut self) -> anyhow::Result<(), NTSTATUS>;
}

macro_rules! _irp_handle {
    ($device:expr, $irp:expr, $irpsp:expr, $($Handler:tt,)*) => {
        match $irpsp.MajorFunction.into() {
            $(
                $Handler::CODE => {
                    let mut handler = $Handler::new(
                        $device,
                        $irp,
                        $irpsp,
                    ).map_err(|_| STATUS_UNSUCCESSFUL)?;
                    handler.handle()
                },
            )*
            _ => Err(STATUS_INVALID_DEVICE_REQUEST),
        }
    };
}

fn irp_handler(
    device: &DEVICE_OBJECT,
    irp: &mut IRP,
    irpsp: &mut IO_STACK_LOCATION,
) -> anyhow::Result<(), NTSTATUS> {
    _irp_handle!(
        device,
        irp,
        irpsp,
        CleanupHandler,
        CreateHandler,
        WMIHandler,
    )
}

unsafe extern "C" fn c_irp_handler(device: PDEVICE_OBJECT, irp: PIRP) -> NTSTATUS {
    let device = match unsafe { device.as_ref() } {
        Some(d) => d,
        None => {
            error!("irp_handler: PDEVICE_OBJECT is null");
            return STATUS_INVALID_PARAMETER;
        }
    };

    let irp = match unsafe { irp.as_mut() } {
        Some(i) => i,
        None => {
            error!("irp_handler: PIRP is null");
            return STATUS_INVALID_PARAMETER;
        }
    };

    let irpsp = match unsafe { IoGetCurrentIrpStackLocation(irp).as_mut() } {
        Some(s) => s,
        None => {
            error!("irp_handler: Failed to call IoGetCurrentIrpStackLocation");
            return STATUS_INVALID_PARAMETER;
        }
    };

    debug!("Received IRP {}", irpsp.MajorFunction);
    let status = match irp_handler(device, irp, irpsp) {
        Ok(()) => STATUS_SUCCESS,
        Err(status) => {
            error!("Error when handling IRP: 0x{status:X}");
            status
        }
    };

    irp.IoStatus.__bindgen_anon_1.Status = status;
    unsafe {
        IofCompleteRequest(irp, IO_NO_INCREMENT as i8);
    }

    status
}

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
            for handler in driver.MajorFunction.iter_mut() {
                *handler = Some(c_irp_handler);
            }
        }
        None => {
            anyhow::bail!("A null pointer is provided to create_device");
        }
    }

    let status = unsafe { IoCreateSymbolicLink(&mut dos_name, &mut device_name) };
    anyhow::ensure!(
        nt_success(status),
        "IoCreateSymbolicLink error: 0x{status:X}"
    );

    Ok(device)
}

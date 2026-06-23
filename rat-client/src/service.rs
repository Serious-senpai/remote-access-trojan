use std::ffi::OsString;
use std::ptr;
use std::time::Duration;

use log::{error, info, warn};
use rat_common::utils::DropGuard;
use rat_common::windows::{DRIVER_USER_OBJECT, IOCTL_START_DEFENSE, RAT_CLIENT_SERVICE_NAME};
use tokio::task;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::ServiceControlHandlerResult;
use windows_service::{define_windows_service, service_control_handler, service_dispatcher};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, GetLastError, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING};
use windows_sys::Win32::System::IO::DeviceIoControl;

define_windows_service!(ffi_service_main, service_main);

fn service_main(_: Vec<OsString>) {
    match service_control_handler::register(RAT_CLIENT_SERVICE_NAME, |event| {
        info!("Received service control event: {event:?}");
        if event == ServiceControl::Interrogate {
            ServiceControlHandlerResult::NoError
        } else {
            ServiceControlHandlerResult::NotImplemented
        }
    }) {
        Ok(event_handler) => {
            if let Err(e) = event_handler.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            }) {
                error!("Failed to set service status: {e}");
            }
        }
        Err(e) => {
            error!("Failed to register service control handler: {e}");
        }
    }
}

pub struct WindowsServiceDispatcher;

impl WindowsServiceDispatcher {
    pub fn start() {
        task::spawn_blocking(|| {
            if let Err(e) = service_dispatcher::start(RAT_CLIENT_SERVICE_NAME, ffi_service_main) {
                error!("Windows Service error: {e}");
            }
        });

        let driver = unsafe {
            CreateFileW(
                DRIVER_USER_OBJECT.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };

        if driver == INVALID_HANDLE_VALUE {
            warn!(
                "Failed to open handle to {DRIVER_USER_OBJECT:?}: 0x{:X}",
                unsafe { GetLastError() },
            );
        } else {
            let guard = DropGuard::new(driver, |h| unsafe {
                CloseHandle(h);
            });

            if unsafe {
                DeviceIoControl(
                    driver,
                    IOCTL_START_DEFENSE,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    ptr::null_mut(),
                ) == 0
            } {
                warn!("Failed to send IOCTL to driver: 0x{:X}", unsafe {
                    GetLastError()
                });
            }

            drop(guard);
        }
    }
}

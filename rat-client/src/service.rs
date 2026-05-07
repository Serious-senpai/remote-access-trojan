use std::ffi::OsString;
use std::sync::Once;
use std::time::Duration;

use log::{error, info};
use rat_common::global::RAT_CLIENT_SERVICE_NAME;
use tokio::task::{self, JoinHandle};
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::ServiceControlHandlerResult;
use windows_service::{define_windows_service, service_control_handler, service_dispatcher};

static SERVICE_STOPPED: Once = Once::new();

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
                return;
            }

            SERVICE_STOPPED.wait();
        }
        Err(e) => {
            error!("Failed to register service control handler: {e}");
        }
    }
}

pub struct WindowsServiceDispatcher {
    thread: JoinHandle<()>,
}

impl WindowsServiceDispatcher {
    pub fn start() -> Self {
        Self {
            thread: task::spawn_blocking(|| {
                if let Err(e) = service_dispatcher::start(RAT_CLIENT_SERVICE_NAME, ffi_service_main)
                {
                    error!("Windows Service error: {e}");
                }
            }),
        }
    }

    pub async fn stop(self) {
        SERVICE_STOPPED.call_once(|| {});
        if let Err(e) = self.thread.await {
            error!("Failed to stop Windows Service: {e}");
        }
    }
}

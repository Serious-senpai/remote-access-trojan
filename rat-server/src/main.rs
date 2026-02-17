use std::time::Duration;

use clap::Parser;
use log::{info, warn};
use rat_common::framework::Module;
use rat_common::logger::initialize_logger;
use rat_server::cli::Arguments;
use rat_server::config::Config;
use rat_server::modules::server::Server;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    initialize_logger(arguments.log_level, &arguments.log_path)?;

    info!("Starting server: {arguments:?}");
    if arguments.request_timeout == 0 {
        warn!("Request timeout is set to 0ms.");
    }

    let config = Config {
        heartbeat_interval: Duration::from_millis(arguments.heartbeat_interval),
        request_timeout: Duration::from_millis(arguments.request_timeout),
    };

    let server = Server::bind(
        ("127.0.0.1", arguments.admin_port),
        ("0.0.0.0", arguments.port),
        config,
    )
    .await?;

    let server_c = server.clone();
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        info!("Received Ctrl-C signal.");
        server_c.stop();
    });

    server.run().await?;

    Ok(())
}

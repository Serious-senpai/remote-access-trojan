use std::sync::Arc;

use clap::Parser;
use log::info;
use rat_client::cli::Arguments;
use rat_client::client::Client;
use rat_common::framework::Module;
use rat_common::logger::initialize_logger;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    initialize_logger(arguments.log_level, &arguments.log_path)?;

    info!("Starting client: {arguments:?}");
    let client = Arc::new(Client::connect(arguments.host).await);

    let client_c = client.clone();
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        info!("Received Ctrl-C signal.");
        client_c.stop();
    });

    client.run().await?;

    Ok(())
}

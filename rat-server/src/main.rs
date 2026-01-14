use clap::Parser;
use log::info;
use rat_common::module::Module;
use rat_server::cli::Arguments;
use rat_server::logger::initialize_logger;
use rat_server::modules::server::Server;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse();
    initialize_logger(arguments.log_level, &arguments.log_path)?;

    info!("Starting server: {arguments:?}");
    let server = Server::bind(
        ("0.0.0.0", arguments.port),
        ("127.0.0.1", arguments.admin_port),
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

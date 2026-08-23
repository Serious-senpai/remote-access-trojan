use clap::{Parser, crate_description, crate_version};

#[derive(Debug, Parser)]
#[command(
    long_about = crate_description!(),
    propagate_version = true,
    version = crate_version!(),
)]
pub struct Arguments {
    /// Arguments for the user-mode Windows service component
    #[arg(
        long,
        default_value = "--host 192.168.56.1:12110 --log-path \"%SystemRoot%\\System32\\violet.log\" --scm"
    )]
    pub argument: String,
}

mod commands;
mod modules;

use std::env;
use std::fs::OpenOptions;
use std::path::PathBuf;

use log::LevelFilter;
use rat_common::module::Module;
use simplelog::{ConfigBuilder, WriteLogger};

use crate::modules::server::Server;

fn _setup_logger() -> anyhow::Result<()> {
    let workdir = if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        dir.to_path_buf()
    } else {
        PathBuf::new()
    };
    let log_path = workdir.join("server.log");
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .write(true)
        .open(log_path)?;
    WriteLogger::init(
        LevelFilter::Debug,
        ConfigBuilder::new()
            .set_location_level(LevelFilter::Debug)
            .set_time_offset_to_local()
            .unwrap_or_else(|e| e)
            .build(),
        file,
    )?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    _setup_logger()?;

    let server = Server::bind("0.0.0.0:8000").await?;
    let _ = server.run().await;
    Ok(())
}

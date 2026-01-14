use std::fs::OpenOptions;
use std::path::Path;

use log::LevelFilter;
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};

pub fn initialize_logger<P: AsRef<Path>>(level: LevelFilter, log_path: P) -> anyhow::Result<()> {
    let config = ConfigBuilder::new()
        .set_location_level(level)
        .set_time_offset_to_local()
        .unwrap_or_else(|e| e)
        .build();

    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_path)?;
    CombinedLogger::init(vec![
        WriteLogger::new(level, config.clone(), file),
        TermLogger::new(level, config, TerminalMode::Stderr, ColorChoice::Auto),
    ])?;

    Ok(())
}

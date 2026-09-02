use alloc::string::String;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub host: String,
    pub log_level: String,
    pub log_path: String,
    pub disable_wd: bool,
    pub disable_sd: bool,
}

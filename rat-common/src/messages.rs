use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SystemInfo {
    pub boot_time: u64,
    pub cpu_arch: String,
    pub distribution_id: String,
    pub host_name: Option<String>,
    pub kernel_long_version: String,
    pub kernel_version: Option<String>,
    pub long_os_version: Option<String>,
    pub name: Option<String>,
    pub open_files_limit: Option<usize>,
    pub os_version: Option<String>,
    pub physical_core_count: Option<usize>,
    pub uptime: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Pong { value: u32 },
    SystemInfoUpdate { info: SystemInfo },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Ping { value: u32 },
}

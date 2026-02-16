use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SystemInfo {
    pub name: String,
    pub kernel_version: String,
    pub os_version: String,
    pub host_name: String,
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

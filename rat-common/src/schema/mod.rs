pub mod input;
pub mod output;

use std::sync::Arc;

use poem_openapi::{Enum, Object, Union};
use serde::{Deserialize, Serialize};

use crate::schema::input::SessionInput;
use crate::schema::output::SessionOutput;
use crate::snowflake::SnowflakeId;

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
pub struct SessionMetadata {
    pub id: SnowflakeId,
    pub inner: SessionMetadataInner,
}

#[derive(Clone, Debug, Deserialize, Serialize, Union)]
#[oai(discriminator_name = "type", rename_all = "kebab-case")]
pub enum SessionMetadataInner {
    Terminal(SessionMetadataInnerTerminal),
}

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
pub struct SessionMetadataInnerTerminal {
    pub pid: u32,
}

#[derive(Clone, Debug, Deserialize, Enum, Serialize)]
#[oai(rename_all = "kebab-case")]
pub enum SessionCreateRequest {
    Terminal,
}

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
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
pub struct ClientMessage {
    pub id: SnowflakeId,
    pub data: ClientMessageData,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClientMessageData {
    Error {
        message: String,
    },
    Pong,
    SystemInfoQueryResponse {
        info: SystemInfo,
    },
    SessionQueryResponse {
        sessions: Vec<Arc<SessionMetadata>>,
    },
    SessionCreateResponse {
        session: Arc<SessionMetadata>,
    },
    SessionInputResponse,
    SessionOutput {
        session_id: SnowflakeId,
        output: SessionOutput,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerMessage {
    pub id: SnowflakeId,
    pub data: ServerMessageData,
}

impl ServerMessage {
    pub fn new(data: ServerMessageData) -> Self {
        Self {
            id: SnowflakeId::new(),
            data,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ServerMessageData {
    Ping,
    SystemInfoQuery,
    SessionQuery,
    SessionCreate {
        request: SessionCreateRequest,
    },
    SessionInput {
        session_id: SnowflakeId,
        input: SessionInput,
    },
}

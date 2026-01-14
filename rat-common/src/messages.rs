use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Pong { value: u32 },
}

#[repr(C)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Ping { value: u32 },
}

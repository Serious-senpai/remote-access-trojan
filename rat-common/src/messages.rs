use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Pong { value: u32 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Ping { value: u32 },
}

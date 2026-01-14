use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Pong {
        value: u32,
    },
    CommandResult {
        id: u32,
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Ping { value: u32 },
    ExecuteCommand { id: u32, command: String },
}

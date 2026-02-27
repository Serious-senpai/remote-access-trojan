use poem_openapi::{Object, Union};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Union)]
pub enum SessionOutput {
    TerminalStdout(SessionOutputTerminalStdout),
    TerminalStderr(SessionOutputTerminalStderr),
    TerminalClosed(SessionOutputTerminalClosed),
}

impl SessionOutput {
    pub fn terminal_stdout(data: String) -> Self {
        Self::TerminalStdout(SessionOutputTerminalStdout { data })
    }

    pub fn terminal_stdout_bytes(data: &[u8]) -> Self {
        Self::terminal_stdout(String::from_utf8_lossy(data).to_string())
    }

    pub fn terminal_stderr(data: String) -> Self {
        Self::TerminalStderr(SessionOutputTerminalStderr { data })
    }

    pub fn terminal_stderr_bytes(data: &[u8]) -> Self {
        Self::terminal_stderr(String::from_utf8_lossy(data).to_string())
    }

    pub fn closed() -> Self {
        Self::TerminalClosed(SessionOutputTerminalClosed {})
    }
}

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
pub struct SessionOutputTerminalStdout {
    pub data: String,
}

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
pub struct SessionOutputTerminalStderr {
    pub data: String,
}

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
pub struct SessionOutputTerminalClosed;

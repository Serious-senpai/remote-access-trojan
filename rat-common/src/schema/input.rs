use poem_openapi::{Object, Union};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Union)]
#[oai(discriminator_name = "type", rename_all = "kebab-case")]
pub enum SessionInput {
    TerminalStdin(SessionInputTerminalStdin),
    Close(SessionInputClose),
}

impl SessionInput {
    pub fn close() -> Self {
        Self::Close(SessionInputClose {})
    }
}

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
pub struct SessionInputTerminalStdin {
    pub data: String,
}

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
pub struct SessionInputClose;

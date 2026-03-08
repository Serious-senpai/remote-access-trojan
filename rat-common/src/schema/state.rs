use poem_openapi::{Object, Union};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Union)]
#[oai(discriminator_name = "type", rename_all = "kebab-case")]
pub enum SessionState {
    Terminal(TerminalSessionState),
}

#[derive(Clone, Debug, Deserialize, Object, Serialize)]
pub struct TerminalSessionState {
    pub data: String,
}

use std::sync::Weak;

use crate::modules::server::Server;

pub struct AdminAPIState {
    pub server: Weak<Server>,
}

impl AdminAPIState {
    pub fn new(server: Weak<Server>) -> Self {
        Self { server }
    }
}

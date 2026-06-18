use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::warn;
use rat_common::framework::{Module, ModuleImpl, ModuleState};
use rat_common::schema::{ClientMessageData, ServerMessage, ServerMessageData};
use tokio::time::sleep;

use crate::config::Config;
use crate::modules::cc::client::ClientConnector;

pub struct ClientInfo {
    _peer: SocketAddr,
    _connector: Weak<ClientConnector>,
    _config: Config,
    _initial_pulse: AtomicBool,
    _name: String,
    _state: Arc<ModuleState>,
}

impl ClientInfo {
    pub fn new(peer: SocketAddr, connector: Weak<ClientConnector>, config: Config) -> Self {
        Self {
            _peer: peer,
            _connector: connector,
            _config: config,
            _initial_pulse: AtomicBool::new(true),
            _name: format!("ClientInfo [{peer}]"),
            _state: ModuleState::new(),
        }
    }
}

#[async_trait]
impl ModuleImpl for ClientInfo {
    type EventType = ();

    fn name(&self) -> &str {
        &self._name
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        if self._initial_pulse.swap(false, Ordering::AcqRel) {
            return;
        }

        sleep(self._config.heartbeat_interval).await;
    }

    async fn handle(self: Arc<Self>, _event: Self::EventType) -> anyhow::Result<()> {
        if let Some(connector) = self._connector.upgrade() {
            match connector
                .request(ServerMessage::new(ServerMessageData::SystemInfoQuery))
                .await
            {
                Ok(response) => match response.data {
                    ClientMessageData::SystemInfoQueryResponse { info } => {
                        connector.update_info(info).await;
                        self.stop();
                    }
                    response => {
                        warn!(
                            "Unexpected response to system info query from {}: {response:?}",
                            self._peer,
                        );
                    }
                },
                Err(e) => {
                    warn!("Failed to query system info from {}: {e}", self._peer);
                }
            }
        }

        Ok(())
    }
}

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::error;
use rat_common::framework::{Module, ModuleImpl, ModuleState};
use rat_common::messages::{ClientMessage, ServerMessage};
use tokio::time::sleep;

use crate::config::Config;
use crate::modules::cc::client::ClientConnector;

pub struct ClientPing {
    _peer: SocketAddr,
    _seq: AtomicU32,
    _connector: Weak<ClientConnector>,
    _config: Config,
    _name: String,
    _state: Arc<ModuleState>,
}

impl ClientPing {
    pub fn new(peer: SocketAddr, connector: Weak<ClientConnector>, config: Config) -> Self {
        Self {
            _peer: peer,
            _seq: AtomicU32::new(0),
            _connector: connector,
            _config: config,
            _name: format!("ClientPing [{peer}]"),
            _state: ModuleState::new(),
        }
    }
}

#[async_trait]
impl ModuleImpl for ClientPing {
    type EventType = ();

    fn name(&self) -> &str {
        &self._name
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        sleep(self._config.heartbeat_interval).await;
    }

    async fn handle(self: Arc<Self>, _event: Self::EventType) -> anyhow::Result<()> {
        if let Some(connector) = self._connector.upgrade() {
            let send = self._seq.fetch_add(1, Ordering::AcqRel);
            if let Err(e) = connector
                .request(
                    &ServerMessage::Ping { value: send },
                    move |m| matches!(m, ClientMessage::Pong { value } if *value == send.wrapping_add(1)),
                )
                .await
            {
                error!(
                    "Ping timed out to {}: {e}. Disconnecting from client.",
                    self._peer,
                );
                connector.stop();
            }
        }

        Ok(())
    }
}

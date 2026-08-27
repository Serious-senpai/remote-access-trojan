use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::schema::{ClientMessage, ClientMessageData};
use tokio::time::sleep;

use crate::client::Client;

const _PING_INTERVAL: Duration = Duration::from_secs(60);

pub struct Ping {
    _client: Weak<Client>,
    _name: String,
    _state: Arc<ModuleState>,
}

impl Ping {
    pub fn new(client: Weak<Client>) -> Self {
        Self {
            _client: client,
            _name: "Ping()".to_string(),
            _state: ModuleState::new(),
        }
    }
}

#[async_trait]
impl ModuleImpl for Ping {
    type EventType = ();

    fn name(&self) -> &str {
        &self._name
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        sleep(_PING_INTERVAL).await;
    }

    async fn handle(self: Arc<Self>, _event: Self::EventType) -> anyhow::Result<()> {
        if let Some(client) = self._client.upgrade()
            && client.is_connected()
        {
            let _ = client
                .send(&ClientMessage::new(ClientMessageData::Nop))
                .await;
        }

        Ok(())
    }
}

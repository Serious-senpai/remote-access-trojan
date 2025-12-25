use std::sync::Arc;

use async_trait::async_trait;
use rat_common::messages::ServerMessage;
use rat_common::module::{Module, ModuleState};
use rat_common::utils::acquire_free_mutex;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::Mutex;

pub struct Sender {
    _tcp: Mutex<OwnedWriteHalf>,
    _state: Arc<ModuleState>,
}

impl Sender {
    pub fn new(tcp: OwnedWriteHalf) -> Self {
        Self {
            _tcp: Mutex::new(tcp),
            _state: Arc::new(ModuleState::new()),
        }
    }

    pub async fn send(&self, message: &ServerMessage) -> anyhow::Result<()> {
        let bytes = postcard::to_stdvec_cobs(message)?;

        let mut tcp = acquire_free_mutex(&self._tcp);
        if let Err(e) = tcp.write_all(&bytes).await {
            self.stop();
            return Err(e.into());
        }

        Ok(())
    }
}

#[async_trait]
impl Module for Sender {
    type EventType = ();

    fn name(&self) -> &str {
        "Sender"
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        self.wait_until_stopped().await;
    }

    async fn handle(self: Arc<Self>, _: Self::EventType) -> anyhow::Result<()> {
        Ok(())
    }
}

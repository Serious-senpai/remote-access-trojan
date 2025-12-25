use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use rat_common::messages::ClientMessage;
use rat_common::module::Module;
use rat_common::utils::acquire_free_mutex;
use tokio::sync::{Mutex, SetOnce, mpsc};

pub struct Console {
    _receiver: Mutex<mpsc::Receiver<(SocketAddr, ClientMessage)>>,
    _stopped: Arc<SetOnce<()>>,
}

impl Console {
    pub fn new(internal: mpsc::Receiver<(SocketAddr, ClientMessage)>) -> Self {
        Self {
            _receiver: Mutex::new(internal),
            _stopped: Arc::new(SetOnce::new()),
        }
    }
}

#[async_trait]
impl Module for Console {
    type EventType = Option<(SocketAddr, ClientMessage)>;

    fn name(&self) -> &str {
        "Console"
    }

    fn stopped(&self) -> std::sync::Arc<tokio::sync::SetOnce<()>> {
        self._stopped.clone()
    }

    async fn listen(self: std::sync::Arc<Self>) -> Self::EventType {
        let mut receiver = acquire_free_mutex(&self._receiver);
        receiver.recv().await
    }

    async fn handle(self: std::sync::Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        if let Some((addr, message)) = event {
            println!("Received from {addr}: {message:?}");
        }
        Ok(())
    }
}

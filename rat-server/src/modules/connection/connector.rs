use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rat_common::messages::{ClientMessage, ServerMessage};
use rat_common::module::{Module, ModuleState};
use rat_common::utils::acquire_free_mutex;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

use crate::modules::connection::receiver::Receiver;
use crate::modules::connection::sender::Sender;

const BASE_PING_INTERVAL_MS: u64 = 10000;
const PING_INTERVAL_ADDITIVE_JITTER_MS: u64 = 30000;
const PING_TIMEOUT: Duration = Duration::from_millis(3000);

pub struct Connector {
    _peer: SocketAddr,
    _sender: Arc<Sender>,
    _receiver: Arc<Receiver>,
    _tasks: Mutex<Vec<JoinHandle<()>>>,
    _state: Arc<ModuleState>,
}

impl Connector {
    pub fn new(
        stream: TcpStream,
        peer: SocketAddr,
        incoming: mpsc::Sender<(SocketAddr, ClientMessage)>,
    ) -> Self {
        let (read, write) = stream.into_split();
        let sender = Arc::new(Sender::new(write));
        let receiver = Arc::new(Receiver::new(peer, read, incoming));

        Self {
            _peer: peer,
            _sender: sender,
            _receiver: receiver,
            _tasks: Mutex::new(vec![]),
            _state: Arc::new(ModuleState::new()),
        }
    }

    pub async fn send(&self, message: &ServerMessage) -> anyhow::Result<()> {
        self._sender.send(message).await
    }

    pub async fn wait_for<F: Fn(&ClientMessage) -> bool + Send + Sync + 'static>(
        &self,
        predicate: F,
    ) -> Option<ClientMessage> {
        self._receiver.wait_for(predicate).await
    }
}

#[async_trait]
impl Module for Connector {
    type EventType = ();

    fn name(&self) -> &str {
        "Connector"
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        sleep(Duration::from_millis(
            BASE_PING_INTERVAL_MS + rand::random_range(0..PING_INTERVAL_ADDITIVE_JITTER_MS),
        ))
        .await;
    }

    async fn handle(self: Arc<Self>, _: Self::EventType) -> anyhow::Result<()> {
        let ping = rand::random();

        let self_cloned = self.clone();
        let wait_for_pong = tokio::spawn(async move {
            timeout(
                PING_TIMEOUT,
                self_cloned.wait_for(move |m| match m {
                    &ClientMessage::Pong { value } => value == ping + 1,
                }),
            )
            .await
        });
        if let Err(e) = self.send(&ServerMessage::Ping { value: ping }).await {
            self.stop();
            return Err(e);
        }

        if wait_for_pong.await.is_err() {
            self.stop();
        }

        Ok(())
    }

    async fn before_hook(self: Arc<Self>) -> anyhow::Result<()> {
        let mut tasks = acquire_free_mutex(&self._tasks);

        let sender = self._sender.clone();
        tasks.push(tokio::spawn(async move {
            let _ = sender.run().await;
        }));

        let receiver = self._receiver.clone();
        tasks.push(tokio::spawn(async move {
            let _ = receiver.run().await;
        }));

        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> anyhow::Result<()> {
        self._sender.stop();
        self._receiver.stop();

        let mut tasks = acquire_free_mutex(&self._tasks);
        while let Some(task) = tasks.pop() {
            let _ = task.await;
        }

        Ok(())
    }
}

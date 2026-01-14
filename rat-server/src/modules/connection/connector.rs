use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use log::{debug, warn};
use rat_common::messages::{ClientMessage, ServerMessage};
use rat_common::module::{Module, ModuleState};
use rat_common::utils::acquire_free_mutex;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

use crate::message::InternalMessage;
use crate::modules::connection::receiver::Receiver;

const BASE_PING_INTERVAL_MS: u64 = 1000;
const PING_INTERVAL_ADDITIVE_JITTER_MS: u64 = 3000;
const PING_TIMEOUT: Duration = Duration::from_millis(3000);

pub struct Connector {
    _peer: SocketAddr,
    _tcp_send: Mutex<OwnedWriteHalf>,
    _receiver: Arc<Receiver>,
    _receiver_task: Mutex<Option<JoinHandle<()>>>,
    _state: Arc<ModuleState>,
}

impl Connector {
    pub fn new(
        stream: TcpStream,
        peer: SocketAddr,
        notifier: mpsc::Sender<InternalMessage>,
    ) -> Self {
        let (read, write) = stream.into_split();
        let receiver = Arc::new(Receiver::new(peer, read, notifier));

        Self {
            _peer: peer,
            _tcp_send: Mutex::new(write),
            _receiver: receiver,
            _receiver_task: Mutex::new(None),
            _state: Arc::new(ModuleState::new()),
        }
    }

    pub async fn send(&self, message: &ServerMessage) -> anyhow::Result<()> {
        let bytes = postcard::to_stdvec_cobs(message)?;

        let mut tcp = self._tcp_send.lock().await;
        if let Err(e) = tcp.write_all(&bytes).await {
            self.stop();
            return Err(e.into());
        }

        Ok(())
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

        let start = Instant::now();
        if let Err(e) = self.send(&ServerMessage::Ping { value: ping }).await {
            self.stop();
            return Err(e);
        }

        let wait_for_pong = wait_for_pong.await;
        let end = Instant::now();
        debug!("RTT to {}: {:?}", self._peer, end - start);

        if let Ok(Ok(_)) = wait_for_pong {
            // pass
        } else {
            warn!("Ping to {} timed out", self._peer);
            self.stop();
        }

        Ok(())
    }

    async fn before_hook(self: Arc<Self>) -> anyhow::Result<()> {
        let receiver = self._receiver.clone();
        let mut task = acquire_free_mutex(&self._receiver_task);
        *task = Some(tokio::spawn(async move {
            let _ = receiver.run().await;
        }));

        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> anyhow::Result<()> {
        self._receiver.stop();

        let mut task = acquire_free_mutex(&self._receiver_task);
        if let Some(task) = task.take() {
            let _ = task.await;
        }

        Ok(())
    }
}

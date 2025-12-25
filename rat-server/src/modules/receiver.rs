use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::warn;
use rat_common::messages::ClientMessage;
use rat_common::module::Module;
use rat_common::utils::acquire_free_mutex;
use tokio::io::AsyncReadExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::{Mutex, SetOnce, mpsc};

pub struct Receiver {
    _peer: SocketAddr,
    _tcp: Mutex<OwnedReadHalf>,
    _current_buffer: Mutex<Vec<u8>>,
    _sender: mpsc::Sender<(SocketAddr, ClientMessage)>,
    _stopped: Arc<SetOnce<()>>,
}

impl Receiver {
    pub fn new(
        peer: SocketAddr,
        tcp: OwnedReadHalf,
        internal: mpsc::Sender<(SocketAddr, ClientMessage)>,
    ) -> Self {
        Self {
            _peer: peer,
            _tcp: Mutex::new(tcp),
            _current_buffer: Mutex::new(vec![]),
            _sender: internal,
            _stopped: Arc::new(SetOnce::new()),
        }
    }
}

#[async_trait]
impl Module for Receiver {
    type EventType = io::Result<Vec<u8>>;

    fn name(&self) -> &str {
        "Receiver"
    }

    fn stopped(&self) -> Arc<SetOnce<()>> {
        self._stopped.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        let mut tcp = acquire_free_mutex(&self._tcp);
        let mut temp = vec![0u8; 512];

        let n = tcp.read(&mut temp).await?;
        Ok(temp[..n].to_vec())
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        let new_data = match event {
            Ok(d) => d,
            Err(e) => {
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
                    Ok(message) => self._sender.send((self._peer, message)).await?,
                    Err(e) => warn!("Failed to deserialize message from {e}: {}", self._peer),
                }
            }
        }

        Ok(())
    }
}

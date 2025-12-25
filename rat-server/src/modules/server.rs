use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::info;
use rat_common::module::Module;
use tokio::net::{TcpListener, TcpStream, tcp};
use tokio::sync::{RwLock, SetOnce, mpsc};

use crate::messages::Message;
use crate::modules::sender::Sender;

pub struct Server {
    _listener: TcpListener,
    _stopped: Arc<SetOnce<()>>,
    _senders: RwLock<HashMap<SocketAddr, (Arc<Sender>, mpsc::Sender<Message>)>>,
}

impl Server {
    pub async fn new(addr: &str) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self {
            _listener: listener,
            _stopped: Arc::new(SetOnce::new()),
            _senders: RwLock::new(HashMap::new()),
        })
    }
}

#[async_trait]
impl Module for Server {
    type EventType = io::Result<(TcpStream, SocketAddr)>;

    fn name(&self) -> &str {
        "Server"
    }

    fn stopped(&self) -> Arc<SetOnce<()>> {
        self._stopped.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        self._listener.accept().await
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        let (stream, addr) = event?;
        info!("New connection from {addr}");

        let (tcp_read, tcp_write) = stream.into_split();
        let (internal_send, internal_receive) = mpsc::channel(3);
        let sender = Arc::new(Sender::new(tcp_write, internal_receive));

        let mut senders = self._senders.write().await;
        senders.insert(addr, (sender.clone(), internal_send));

        Ok(())
    }

    async fn before_hook(self: Arc<Self>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}

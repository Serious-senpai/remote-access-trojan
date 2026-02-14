use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use rat_common::module::{ModuleImpl, ModuleState};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

pub struct CCServer {
    _listener: TcpListener,
    _state: Arc<ModuleState>,
}

impl CCServer {
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> anyhow::Result<Arc<Self>> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Arc::new(Self {
            _listener: listener,
            _state: ModuleState::new(),
        }))
    }
}

#[async_trait]
impl ModuleImpl for CCServer {
    type EventType = io::Result<(TcpStream, SocketAddr)>;

    fn name(&self) -> &str {
        "C&C Server"
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        self._listener.accept().await
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        let (_, addr) = event?;
        println!("Accepted connection from {addr}");
        Ok(())
    }
}

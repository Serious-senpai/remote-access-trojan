mod api;
mod schema;
mod state;

use std::net::SocketAddrV4;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use log::{error, info};
use poem::EndpointExt;
use poem::endpoint::StaticFilesEndpoint;
use poem::listener::{Listener, RustlsCertificate, RustlsConfig, RustlsListener, TcpListener};
use poem_openapi::OpenApiService;
use rat_common::framework::{ModuleImpl, ModuleState};
use tokio::fs;
use tokio::net::ToSocketAddrs;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::modules::admin::state::AdminAPIState;
use crate::modules::server::Server;

pub struct AdminServer {
    _address: SocketAddrV4,
    _server: Weak<Server>,
    _task: Mutex<Option<JoinHandle<()>>>,
    _config: Config,
    _state: Arc<ModuleState>,
}

impl AdminServer {
    pub fn bind(server: Weak<Server>, addr: SocketAddrV4, config: Config) -> Arc<Self> {
        Arc::new(Self {
            _address: addr,
            _server: server,
            _task: Mutex::new(None),
            _config: config,
            _state: ModuleState::new(),
        })
    }

    /// Reference: https://github.com/rustls/tokio-rustls/blob/main/examples/server.rs
    async fn _prepare_tls_stream<T>(
        &self,
        listener: TcpListener<T>,
    ) -> anyhow::Result<RustlsListener<TcpListener<T>, RustlsConfig>>
    where
        T: ToSocketAddrs + Send,
    {
        let cert = fs::read(self._config.tls_cert_path.as_ref()).await?;
        let key = fs::read(self._config.tls_key_path.as_ref()).await?;
        let root_ca = fs::read(self._config.tls_client_trust_anchor.as_ref()).await?;
        let config = RustlsConfig::new()
            .fallback(RustlsCertificate::new().cert(cert).key(key))
            .client_auth_required(root_ca);

        Ok(listener.rustls(config))
    }
}

#[async_trait]
impl ModuleImpl for AdminServer {
    type EventType = ();

    fn name(&self) -> &str {
        "Admin Server"
    }

    fn state(&self) -> Arc<ModuleState> {
        self._state.clone()
    }

    async fn listen(self: Arc<Self>) -> Self::EventType {
        self.wait_until_stopped().await
    }

    async fn handle(self: Arc<Self>, _event: Self::EventType) -> anyhow::Result<()> {
        Ok(())
    }

    async fn before_hook(self: Arc<Self>) -> anyhow::Result<()> {
        let state = Arc::new(AdminAPIState::new(self._server.clone()));
        let api_service =
            OpenApiService::new(api::AdminAPI::new(self._config.clone()), "Admin API", "0.1")
                .server("/api");

        let spec = api_service.spec_endpoint();
        let swagger = api_service.swagger_ui();

        let app = poem::Route::new()
            .nest(
                "/",
                StaticFilesEndpoint::new(&*self._config.frontend_static_files)
                    .index_file("index.html"),
            )
            .nest("/api", api_service)
            .nest("/docs/openapi.json", spec)
            .nest("/docs/swagger", swagger)
            .data(state);

        let listener = self
            ._prepare_tls_stream(TcpListener::bind(self._address))
            .await?;
        let server = poem::Server::new(listener);

        let self_cloned = self.clone();
        self._task.lock().await.replace(tokio::spawn(async move {
            if let Err(e) = server
                .run_with_graceful_shutdown(
                    app,
                    async move {
                        self_cloned.wait_until_stopped().await;
                        info!("Shutting down Admin API server...");
                    },
                    Some(Duration::from_secs(5)),
                )
                .await
            {
                error!("OpenAPI server error: {e}");
            }
        }));

        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> anyhow::Result<()> {
        if let Some(task) = self._task.lock().await.take() {
            task.await?;
        }

        Ok(())
    }
}

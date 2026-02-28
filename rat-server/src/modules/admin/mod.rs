mod api;
mod schema;
mod state;

use std::net::SocketAddrV4;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::error;
use poem::EndpointExt;
use poem::endpoint::StaticFilesEndpoint;
use poem::listener::TcpListener;
use poem_openapi::OpenApiService;
use rat_common::framework::{ModuleImpl, ModuleState};
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
        let server = poem::Server::new(TcpListener::bind(self._address));

        let self_cloned = self.clone();
        self._task.lock().await.replace(tokio::spawn(async move {
            if let Err(e) = server
                .run_with_graceful_shutdown(
                    app,
                    async move {
                        self_cloned.wait_until_stopped().await;
                    },
                    None,
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

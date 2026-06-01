use std::net::SocketAddrV4;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use log::{error, info};
use poem::endpoint::StaticFilesEndpoint;
use poem::listener::TcpListener;
use rat_common::framework::{ModuleImpl, ModuleState};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::modules::server::Server;

pub struct StaticFilesServer {
    _address: SocketAddrV4,
    _server: Weak<Server>,
    _task: Mutex<Option<JoinHandle<()>>>,
    _config: Config,
    _state: Arc<ModuleState>,
}

impl StaticFilesServer {
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
impl ModuleImpl for StaticFilesServer {
    type EventType = ();

    fn name(&self) -> &str {
        "Static Files Server"
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
        let app = poem::Route::new().nest(
            "/",
            StaticFilesEndpoint::new(self._config.static_files_dir.as_ref()).show_files_listing(),
        );
        let listener = TcpListener::bind(self._address);
        let server = poem::Server::new(listener);

        let self_cloned = self.clone();
        self._task.lock().await.replace(tokio::spawn(async move {
            if let Err(e) = server
                .run_with_graceful_shutdown(
                    app,
                    async move {
                        self_cloned.wait_until_stopped().await;
                        info!("Shutting down {}...", self_cloned.name());
                    },
                    Some(Duration::from_secs(5)),
                )
                .await
            {
                error!("Static files server error: {e}");
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

mod api;

use std::sync::Arc;

use async_trait::async_trait;
use log::error;
use poem::listener::TcpListener;
use poem::{Route, Server};
use poem_openapi::OpenApiService;
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::types::PortableSocketAddrs;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub struct AdminServer<A>
where
    A: PortableSocketAddrs,
{
    _address: A,
    _task: Mutex<Option<JoinHandle<()>>>,
    _state: Arc<ModuleState>,
}

impl<A> AdminServer<A>
where
    A: PortableSocketAddrs,
{
    pub async fn bind(addr: A) -> anyhow::Result<Arc<Self>> {
        Ok(Arc::new(Self {
            _address: addr,
            _task: Mutex::new(None),
            _state: ModuleState::new(),
        }))
    }
}

#[async_trait]
impl<A> ModuleImpl for AdminServer<A>
where
    A: PortableSocketAddrs,
{
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
        let api_service = OpenApiService::new(api::AdminAPI, "Admin API", "1.0");
        let docs = api_service.swagger_ui();

        let app = Route::new().nest("/", api_service).nest("/docs", docs);
        let server = Server::new(TcpListener::bind(self._address.clone()));

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

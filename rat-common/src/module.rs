use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use log::{debug, error, info};
use tokio::sync::SetOnce;

pub struct ModuleState {
    pub(crate) stopped: SetOnce<()>,
    pub(crate) running: AtomicBool,
}

impl ModuleState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            stopped: SetOnce::new(),
            running: AtomicBool::new(false),
        })
    }
}

#[async_trait]
pub trait ModuleImpl: Send + Sync {
    type EventType;

    fn name(&self) -> &str;
    fn state(&self) -> Arc<ModuleState>;

    fn submodules(&self) -> Vec<Arc<dyn Module>> {
        vec![]
    }

    async fn wait_until_stopped(&self) {
        let _ = self.state().stopped.wait().await;
    }

    /// Listen for incoming events. Must be cancel-safe.
    async fn listen(self: Arc<Self>) -> Self::EventType;

    /// Handle an incoming event. Unlike [listen()], this method is not required to be cancel-safe.
    /// However a long running handler may increase shutdown latency.
    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()>;

    async fn before_hook(self: Arc<Self>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &str;
    fn state(&self) -> Arc<ModuleState>;

    async fn run(self: Arc<Self>) -> anyhow::Result<()>;

    fn stop(&self);
}

#[async_trait]
impl<T> Module for T
where
    T: ModuleImpl,
{
    fn name(&self) -> &str {
        ModuleImpl::name(self)
    }

    fn state(&self) -> Arc<ModuleState> {
        ModuleImpl::state(self)
    }

    async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        let state = self.state();
        if state.running.swap(true, Ordering::AcqRel) {
            error!("Module {} is already running", self.name());
            return Ok(());
        }

        if state.stopped.get().is_none() {
            debug!("Running before_hook for module {}", self.name());
            self.clone().before_hook().await.map_err(|e| {
                error!("Error in before_hook for module {}: {e}", self.name());
                e
            })?;

            let children = self.submodules();
            let mut children_tasks = vec![];
            for child in &children {
                let child = child.clone();
                children_tasks.push(tokio::spawn(async move {
                    let _ = child.run().await;
                }));
            }

            info!("Running module {}", self.name());
            while state.stopped.get().is_none() {
                let state = state.clone();
                let event = tokio::select! {
                    biased;
                    _ = state.stopped.wait() => break,
                    event = self.clone().listen() => event,
                };

                debug!("Running handler for module {}", self.name());
                if let Err(e) = self.clone().handle(event).await {
                    error!("Error when handling event in module {}: {e}", self.name());
                }
            }

            for child in &children {
                child.stop();
            }

            for task in children_tasks {
                let _ = task.await;
            }

            debug!("Running after_hook for module {}", self.name());
            self.clone().after_hook().await.map_err(|e| {
                error!("Error in after_hook for module {}: {e}", self.name());
                e
            })?;

            info!("Module {} completed successfully", self.name());
        } else {
            error!("Module {} is already stopped", self.name());
        }

        state.running.store(false, Ordering::Release);
        Ok(())
    }

    fn stop(&self) {
        let _ = self.state().stopped.set(());
    }
}

#[macro_export]
macro_rules! composite_module_impl {
    ($struct:ident, $name:literal, $state_field:ident, $submodules_field:ident) => {
        #[async_trait::async_trait]
        impl ModuleImpl for $struct {
            type EventType = ();

            fn name(&self) -> &str {
                $name
            }

            fn state(&self) -> Arc<ModuleState> {
                self.$state_field.clone()
            }

            fn submodules(&self) -> Vec<Arc<dyn Module>> {
                self.$submodules_field.clone()
            }

            async fn listen(self: Arc<Self>) -> Self::EventType {
                self.wait_until_stopped().await;
            }

            async fn handle(self: Arc<Self>, _event: Self::EventType) -> anyhow::Result<()> {
                Ok(())
            }
        }
    };
}

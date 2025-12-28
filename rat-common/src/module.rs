use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use log::{debug, error, info};
use tokio::sync::SetOnce;

pub struct ModuleState {
    _stopped: SetOnce<()>,
    _running: AtomicBool,
}

impl ModuleState {
    pub fn new() -> Self {
        Self {
            _stopped: SetOnce::new(),
            _running: AtomicBool::new(false),
        }
    }
}

impl Default for ModuleState {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
pub trait Module: Send + Sync {
    type EventType;

    fn name(&self) -> &str;
    fn state(&self) -> Arc<ModuleState>;

    async fn listen(self: Arc<Self>) -> Self::EventType;
    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()>;

    async fn before_hook(self: Arc<Self>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn wait_until_stopped(&self) {
        self.state()._stopped.wait().await;
    }

    async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        let state = self.state();
        if state._running.swap(true, Ordering::AcqRel) {
            error!("Module {} is already running", self.name());
            return Ok(());
        }

        debug!("Running before_hook for module {}", self.name());
        self.clone().before_hook().await.map_err(|e| {
            error!("Error in before_hook for module {}: {e}", self.name());
            e
        })?;

        info!("Running module {}", self.name());
        while state._stopped.get().is_none() {
            let state = state.clone();
            let event = tokio::select! {
                biased;
                _ = state._stopped.wait() => break,
                event = self.clone().listen() => event,
            };

            debug!("Running handler for module {}", self.name());
            if let Err(e) = self.clone().handle(event).await {
                error!("Error when handling event in module {}: {e}", self.name());
            }
        }

        debug!("Running after_hook for module {}", self.name());
        self.clone().after_hook().await.map_err(|e| {
            error!("Error in after_hook for module {}: {e}", self.name());
            e
        })?;

        info!("Module {} completed successfully", self.name());
        state._running.store(false, Ordering::Release);
        Ok(())
    }

    fn stop(&self) {
        info!("Stopping module {}", self.name());
        let _ = self.state()._stopped.set(());
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error, info};
use tokio::sync::SetOnce;

#[async_trait]
pub trait Module: Send + Sync {
    type EventType;

    fn name(&self) -> &str;
    fn stopped(&self) -> Arc<SetOnce<()>>;

    async fn listen(self: Arc<Self>) -> Self::EventType;
    async fn handle(self: Arc<Self>, event: Self::EventType) -> anyhow::Result<()>;

    async fn before_hook(self: Arc<Self>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn after_hook(self: Arc<Self>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        debug!("Running before_hook for module {}", self.name());
        self.clone().before_hook().await.map_err(|e| {
            error!("Error in before_hook for module {}: {e}", self.name());
            e
        })?;

        info!("Running module {}", self.name());
        while self.stopped().get().is_none() {
            let stopped = self.stopped();
            let event = tokio::select! {
                biased;
                _ = stopped.wait() => break,
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
        Ok(())
    }

    fn stop(&self) {
        info!("Stopping module {}", self.name());
        if let Err(e) = self.stopped().set(()) {
            error!("Error stopping module {}: {e}", self.name());
        } else {
            info!("Stop signal sent to module {}", self.name());
        }
    }
}

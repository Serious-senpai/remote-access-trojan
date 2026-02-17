use std::sync::Arc;

use async_trait::async_trait;

use crate::framework::Module;
use crate::framework::module_state::ModuleState;

#[async_trait]
pub trait ModuleImpl: Send + Sync {
    type EventType;

    fn name(&self) -> &str;
    fn state(&self) -> Arc<ModuleState>;

    async fn add_submodule(&self, submodule: Arc<dyn Module>) {
        self.state().add_submodule(submodule).await;
    }

    async fn wait_until_stopped(&self) {
        self.state().wait_until_stopped().await;
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

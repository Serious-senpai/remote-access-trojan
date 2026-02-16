use std::sync::Arc;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use log::{debug, error, info};

use crate::module::module_impl::ModuleImpl;
use crate::module::module_state::ModuleState;

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

        if !state.stopped() {
            debug!("Running before_hook for module {}", self.name());
            self.clone().before_hook().await.map_err(|e| {
                error!("Error in before_hook for module {}: {e}", self.name());
                e
            })?;

            self.state().start_all_submodules().await;

            info!("Running module {}", self.name());
            while !state.stopped() {
                let event = tokio::select! {
                    biased;
                    _ = self.wait_until_stopped() => break,
                    event = self.clone().listen() => event,
                };

                debug!("Running handler for module {}", self.name());
                if let Err(e) = self.clone().handle(event).await {
                    error!("Error when handling event in module {}: {e}", self.name());
                }
            }

            self.state().stop_all_submodules().await;

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
        let _ = self.state().stop();
    }
}

#[macro_export]
macro_rules! empty_module_impl {
    ($struct:ident, $name:literal, $state_field:ident) => {
        #[async_trait::async_trait]
        impl ModuleImpl for $struct {
            type EventType = ();

            fn name(&self) -> &str {
                $name
            }

            fn state(&self) -> Arc<ModuleState> {
                self.$state_field.clone()
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

pub mod terminal;

use std::sync::Arc;

use async_trait::async_trait;
use rat_common::framework::{Module, ModuleImpl};
use rat_common::schema::SessionMetadata;
use rat_common::schema::input::SessionInput;
use rat_common::schema::state::SessionState;

#[async_trait]
pub trait SessionImpl: Send + Sync {
    fn metadata(&self) -> Arc<SessionMetadata>;
    async fn input(&self, data: SessionInput) -> anyhow::Result<()>;
    async fn query_current_state(&self) -> anyhow::Result<SessionState>;
}

#[async_trait]
pub trait Session: SessionImpl + Module {}

impl<T: SessionImpl + ModuleImpl> Session for T {}

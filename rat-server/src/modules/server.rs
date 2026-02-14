use std::sync::Arc;

use rat_common::composite_module_impl;
use rat_common::module::{Module, ModuleImpl, ModuleState};
use tokio::net::ToSocketAddrs;

use crate::modules::admin::AdminServer;
use crate::modules::cc::CCServer;

pub struct Server {
    _state: Arc<ModuleState>,
    _submodules: Vec<Arc<dyn Module>>,
}

impl Server {
    pub async fn bind<A1, A2>(admin_addr: A1, cc_addr: A2) -> anyhow::Result<Arc<Self>>
    where
        A1: ToSocketAddrs + Clone + Send + Sync + 'static,
        A2: ToSocketAddrs,
    {
        let admin = AdminServer::bind(admin_addr).await?;
        let cc = CCServer::bind(cc_addr).await?;
        Ok(Arc::new(Self {
            _state: ModuleState::new(),
            _submodules: vec![admin, cc],
        }))
    }
}

composite_module_impl!(Server, "Server", _state, _submodules);

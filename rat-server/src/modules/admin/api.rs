use std::sync::Arc;

use poem::web::Data;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;
use poem_openapi::{OpenApi, Tags};
use rat_common::schema::{SessionCreateRequest, SessionInput};

use crate::modules::admin::schema;
use crate::modules::admin::state::AdminAPIState;

#[derive(Tags)]
enum AdminAPITag {
    Clients,
}

pub struct AdminAPI;

#[OpenApi(prefix_path = "/api", tag = AdminAPITag::Clients)]
impl AdminAPI {
    /// List all clients connected to the C&C server.
    #[oai(path = "/clients", method = "get")]
    async fn get_clients(&self, state: Data<&Arc<AdminAPIState>>) -> schema::GetClientsResponse {
        schema::GetClientsResponse::Ok(match state.server.upgrade() {
            Some(server) => {
                let clients = server.get_clients().await;
                schema::AdminResult::success(
                    clients
                        .into_iter()
                        .map(|(address, info)| schema::ClientAPI {
                            address: address.to_string(),
                            info,
                        })
                        .collect(),
                )
            }
            None => schema::AdminResult::error(schema::AdminResultCode::DeadServer),
        })
    }

    /// Get information about a specific client by its address.
    #[oai(path = "/clients/:addr", method = "get")]
    async fn get_clients_addr(
        &self,
        addr: Path<String>,
        state: Data<&Arc<AdminAPIState>>,
    ) -> schema::GetClientsAddrResponse {
        schema::GetClientsAddrResponse::Ok(match state.server.upgrade() {
            Some(server) => match addr.parse() {
                Ok(address) => match server.get_clients_addr(&address).await {
                    Some(info) => schema::AdminResult::success(schema::ClientAPI {
                        address: addr.0,
                        info,
                    }),
                    None => schema::AdminResult::error(schema::AdminResultCode::ClientNotFound),
                },
                Err(e) => schema::AdminResult::error_with_message(
                    schema::AdminResultCode::InvalidInput,
                    format!("Invalid address: {e}"),
                ),
            },
            None => schema::AdminResult::error(schema::AdminResultCode::DeadServer),
        })
    }

    /// List all sessions of a specific client
    #[oai(path = "/clients/:addr/sessions", method = "get")]
    async fn get_clients_addr_sessions(
        &self,
        addr: Path<String>,
        state: Data<&Arc<AdminAPIState>>,
    ) -> schema::GetClientsAddrSessionsResponse {
        schema::GetClientsAddrSessionsResponse::Ok(match state.server.upgrade() {
            Some(server) => match addr.parse() {
                Ok(address) => match server.get_clients_addr_sessions(&address).await {
                    Ok(Some(sessions)) => schema::AdminResult::success(sessions),
                    Ok(None) => schema::AdminResult::error(schema::AdminResultCode::ClientNotFound),
                    Err(e) => schema::AdminResult::error_with_message(
                        schema::AdminResultCode::Other,
                        format!("Failed to query client sessions: {e}"),
                    ),
                },
                Err(e) => schema::AdminResult::error_with_message(
                    schema::AdminResultCode::InvalidInput,
                    format!("Invalid address: {e}"),
                ),
            },
            None => schema::AdminResult::error(schema::AdminResultCode::DeadServer),
        })
    }

    /// Create a new session for a specific client
    #[oai(path = "/clients/:addr/sessions", method = "post")]
    async fn post_clients_addr_sessions(
        &self,
        addr: Path<String>,
        body: Json<SessionCreateRequest>,
        state: Data<&Arc<AdminAPIState>>,
    ) -> schema::PostClientsAddrSessionsResponse {
        schema::PostClientsAddrSessionsResponse::Ok(match state.server.upgrade() {
            Some(server) => match addr.parse() {
                Ok(address) => match server.post_clients_addr_sessions(&address, body.0).await {
                    Ok(Some(session)) => schema::AdminResult::success(session),
                    Ok(None) => schema::AdminResult::error(schema::AdminResultCode::ClientNotFound),
                    Err(e) => schema::AdminResult::error_with_message(
                        schema::AdminResultCode::Other,
                        format!("Failed to create client session: {e}"),
                    ),
                },
                Err(e) => schema::AdminResult::error_with_message(
                    schema::AdminResultCode::InvalidInput,
                    format!("Invalid address: {e}"),
                ),
            },
            None => schema::AdminResult::error(schema::AdminResultCode::DeadServer),
        })
    }

    /// Delete an existing session of a specific client
    #[oai(path = "/clients/:addr/sessions/:session_id", method = "delete")]
    async fn delete_clients_addr_sessions_session_id(
        &self,
        addr: Path<String>,
        session_id: Path<String>,
        state: Data<&Arc<AdminAPIState>>,
    ) -> schema::DeleteClientsAddrSessionsResponse {
        schema::DeleteClientsAddrSessionsResponse::Ok(match session_id.0.try_into() {
            Ok(session_id) => match state.server.upgrade() {
                Some(server) => match addr.parse() {
                    Ok(address) => match server
                        .delete_clients_addr_sessions_session_id(&address, session_id)
                        .await
                    {
                        Ok(Some(())) => schema::AdminResult::success(session_id),
                        Ok(None) => {
                            schema::AdminResult::error(schema::AdminResultCode::ClientNotFound)
                        }
                        Err(e) => schema::AdminResult::error_with_message(
                            schema::AdminResultCode::Other,
                            format!("Failed to delete client session: {e}"),
                        ),
                    },
                    Err(e) => schema::AdminResult::error_with_message(
                        schema::AdminResultCode::InvalidInput,
                        format!("Invalid address: {e}"),
                    ),
                },
                None => schema::AdminResult::error(schema::AdminResultCode::DeadServer),
            },
            Err(e) => schema::AdminResult::error_with_message(
                schema::AdminResultCode::InvalidInput,
                format!("Invalid session ID: {e}"),
            ),
        })
    }

    /// Send input to an existing session of a specific client
    #[oai(path = "/clients/:addr/sessions/:session_id/data", method = "post")]
    async fn post_clients_addr_sessions_session_id_input(
        &self,
        addr: Path<String>,
        session_id: Path<String>,
        body: Json<SessionInput>,
        state: Data<&Arc<AdminAPIState>>,
    ) -> schema::PostClientsAddrSessionsSessionIdInputResponse {
        schema::PostClientsAddrSessionsSessionIdInputResponse::Ok(match session_id.0.try_into() {
            Ok(session_id) => match state.server.upgrade() {
                Some(server) => match addr.parse() {
                    Ok(address) => match server
                        .post_clients_addr_sessions_session_id_input(&address, session_id, body.0)
                        .await
                    {
                        Ok(Some(())) => schema::AdminResult::success(session_id),
                        Ok(None) => {
                            schema::AdminResult::error(schema::AdminResultCode::ClientNotFound)
                        }
                        Err(e) => schema::AdminResult::error_with_message(
                            schema::AdminResultCode::Other,
                            format!("Failed to delete client session: {e}"),
                        ),
                    },
                    Err(e) => schema::AdminResult::error_with_message(
                        schema::AdminResultCode::InvalidInput,
                        format!("Invalid address: {e}"),
                    ),
                },
                None => schema::AdminResult::error(schema::AdminResultCode::DeadServer),
            },
            Err(e) => schema::AdminResult::error_with_message(
                schema::AdminResultCode::InvalidInput,
                format!("Invalid session ID: {e}"),
            ),
        })
    }
}

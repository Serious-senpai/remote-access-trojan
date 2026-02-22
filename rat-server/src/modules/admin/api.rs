use std::sync::Arc;

use poem::web::Data;
use poem_openapi::param::Path;
use poem_openapi::{OpenApi, Tags};

use crate::modules::admin::schema;
use crate::modules::admin::state::AdminAPIState;

#[derive(Tags)]
enum AdminAPITag {
    Clients,
}

pub struct AdminAPI;

#[OpenApi(tag = AdminAPITag::Clients)]
impl AdminAPI {
    /// List all clients connected to the C&C server.
    #[oai(path = "/clients", method = "get")]
    async fn get_clients(&self, state: Data<&Arc<AdminAPIState>>) -> schema::GetClientResponse {
        schema::GetClientResponse::Ok(match state.server.upgrade() {
            Some(server) => {
                let clients = server.clients().await;
                schema::AdminResult::success(
                    clients
                        .into_iter()
                        .map(|(address, info)| schema::Client {
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
    ) -> schema::GetClientAddrResponse {
        schema::GetClientAddrResponse::Ok(match state.server.upgrade() {
            Some(server) => match addr.parse() {
                Ok(address) => match server.client(&address).await {
                    Some(info) => schema::AdminResult::success(schema::Client {
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
}

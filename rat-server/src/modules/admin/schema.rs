use std::sync::Arc;

use poem_openapi::payload::{EventStream, Json};
use poem_openapi::types::{ParseFromJSON, ToJSON, Type};
use poem_openapi::{ApiResponse, Enum, Object};
use rat_common::schema::output::SessionOutput;
use rat_common::schema::{SessionMetadata, SystemInfo};
use rat_common::snowflake::SnowflakeId;

#[derive(Enum)]
#[oai(rename_all = "kebab-case")]
pub enum AdminResultCode {
    Success,
    DeadServer,
    InvalidInput,
    ClientNotFound,
    Other,
}

#[derive(Object)]
pub struct AdminResult<T>
where
    T: ParseFromJSON + ToJSON + Type + Send + Sync,
{
    pub code: AdminResultCode,
    pub error: Option<String>,
    pub data: Option<T>,
}

impl<T> AdminResult<T>
where
    T: ParseFromJSON + ToJSON + Type + Send + Sync,
{
    pub fn success(data: T) -> Json<Self> {
        Json(Self {
            code: AdminResultCode::Success,
            error: None,
            data: Some(data),
        })
    }

    pub fn error(code: AdminResultCode) -> Json<Self> {
        let message = match code {
            AdminResultCode::Success => "Success",
            AdminResultCode::DeadServer => "The server is not running",
            AdminResultCode::InvalidInput => "The input provided is invalid",
            AdminResultCode::ClientNotFound => "The specified client was not found",
            AdminResultCode::Other => "An unknown error occurred",
        }
        .to_string();
        Self::error_with_message(code, message)
    }

    pub fn error_with_message(code: AdminResultCode, message: impl Into<String>) -> Json<Self> {
        Json(Self {
            code,
            error: Some(message.into()),
            data: None,
        })
    }

    pub fn arbitrary_error(error: impl ToString) -> Json<Self> {
        Self::error_with_message(AdminResultCode::Other, error.to_string())
    }
}

#[derive(Object)]
pub struct ClientAPI {
    pub address: String,
    pub info: Option<SystemInfo>,
}

#[derive(ApiResponse)]
pub enum GetClientsResponse {
    #[oai(status = 200)]
    Ok(Json<AdminResult<Vec<ClientAPI>>>),
}

#[derive(ApiResponse)]
pub enum GetClientsAddrResponse {
    #[oai(status = 200)]
    Ok(Json<AdminResult<ClientAPI>>),
}

#[derive(ApiResponse)]
pub enum GetClientsAddrSessionsResponse {
    #[oai(status = 200)]
    Ok(Json<AdminResult<Vec<Arc<SessionMetadata>>>>),
}

#[derive(ApiResponse)]
pub enum PostClientsAddrSessionsResponse {
    #[oai(status = 200)]
    Ok(Json<AdminResult<Arc<SessionMetadata>>>),
}

#[derive(ApiResponse)]
pub enum DeleteClientsAddrSessionsResponse {
    #[oai(status = 200)]
    Ok(Json<AdminResult<SnowflakeId>>),
}

#[derive(ApiResponse)]
pub enum PostClientsAddrSessionsSessionIdInputResponse {
    #[oai(status = 200)]
    Ok(Json<AdminResult<SnowflakeId>>),
}

#[derive(ApiResponse)]
pub enum GetClientsAddrSessionsSessionIdInputResponse<T>
where
    T: futures_util::Stream<Item = AdminResult<SessionOutput>> + Send + 'static,
{
    #[oai(status = 200)]
    Ok(EventStream<T>),
}

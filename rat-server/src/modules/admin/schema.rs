use poem_openapi::payload::Json;
use poem_openapi::types::{ParseFromJSON, ToJSON, Type};
use poem_openapi::{ApiResponse, Enum, Object};
use rat_common::schema::SystemInfo;

#[derive(Enum)]
#[oai(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdminResultCode {
    Success,
    DeadServer,
    InvalidInput,
    ClientNotFound,
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
}

#[derive(Object)]
pub struct Client {
    pub address: String,
    pub info: Option<SystemInfo>,
}

#[derive(ApiResponse)]
pub enum GetClientResponse {
    #[oai(status = 200)]
    Ok(Json<AdminResult<Vec<Client>>>),
}

#[derive(ApiResponse)]
pub enum GetClientAddrResponse {
    #[oai(status = 200)]
    Ok(Json<AdminResult<Client>>),
}

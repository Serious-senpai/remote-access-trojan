use poem_openapi::OpenApi;
use poem_openapi::payload::PlainText;

pub struct AdminAPI;

#[OpenApi]
impl AdminAPI {
    /// Hello World endpoint
    #[oai(path = "/", method = "get")]
    async fn index(&self) -> PlainText<&'static str> {
        PlainText("Hello World!")
    }
}

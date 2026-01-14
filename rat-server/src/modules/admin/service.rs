use std::pin::Pin;
use std::sync::{LazyLock, Weak};

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::Service;
use hyper::{Method, Request, Response};

use crate::modules::server::Server;

pub struct AdminService {
    _server: Weak<Server>,
}

impl AdminService {
    pub fn new(server: Weak<Server>) -> Self {
        Self { _server: server }
    }
}

static HTTP_404: LazyLock<Response<Full<Bytes>>> = LazyLock::new(|| {
    Response::builder()
        .status(404)
        .body(Full::new(Bytes::from_static(b"Not Found")))
        .unwrap()
});

static HTTP_405: LazyLock<Response<Full<Bytes>>> = LazyLock::new(|| {
    Response::builder()
        .status(405)
        .body(Full::new(Bytes::from_static(b"Method Not Allowed")))
        .unwrap()
});

static HTTP_503: LazyLock<Response<Full<Bytes>>> = LazyLock::new(|| {
    Response::builder()
        .status(503)
        .body(Full::new(Bytes::from_static(b"Service Unavailable")))
        .unwrap()
});

impl Service<Request<Incoming>> for AdminService {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::http::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let server: Weak<Server> = self._server.clone();
        Box::pin(async move {
            let response = match request.uri().path() {
                "/clients" => match request.method() {
                    &Method::GET => match server.upgrade() {
                        Some(server) => {
                            let clients = server.list_clients().await;
                            let body = serde_json::to_vec(&clients).unwrap_or_default();
                            Response::builder()
                                .status(200)
                                .body(Full::new(body.into()))?
                        }
                        None => HTTP_503.clone(),
                    },
                    _ => HTTP_405.clone(),
                },
                "/cmd" => match request.method() {
                    &Method::POST => {}
                    _ => HTTP_405.clone(),
                },
                _ => HTTP_404.clone(),
            };

            Ok(response)
        })
    }
}

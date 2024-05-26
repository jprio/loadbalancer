use async_trait::async_trait;
use std::{
    collections::linked_list::Iter,
    fmt::Display,
    iter::Cycle,
    sync::atomic::{AtomicUsize, Ordering},
};

use actix_web::{
    http::header::ContentType,
    web::{self, Data},
    App, HttpRequest, HttpResponse, HttpServer, ResponseError,
};
use reqwest::Client;
pub struct Loadbalancer {
    port: u16,
    data: Data<AppState>,
}
struct AppState {
    client: Client,
    policy: Box<SafeRoutingPolicy>,
}
pub type SafeRoutingPolicy = dyn RoutingPolicy + Sync + Send;

pub struct SingleServerPolicy {
    server: String,
}

impl SingleServerPolicy {
    pub fn new(server: String) -> Self {
        Self { server: server }
    }
}
pub struct RoundRobinServerPolicy {
    servers: Vec<String>,
    idx: AtomicUsize,
}
impl RoundRobinServerPolicy {
    pub fn new(servers: Vec<String>) -> Self {
        Self {
            servers: servers,
            idx: AtomicUsize::new(0),
        }
    }
}
#[async_trait]
impl RoutingPolicy for RoundRobinServerPolicy {
    async fn next(&self, request: &HttpRequest) -> String {
        let servers = &self.servers;
        let max_server_idx = servers.len() - 1;

        // Update index
        let idx = self
            .idx
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |idx| match idx {
                x if x >= max_server_idx => Some(0),
                c => Some(c + 1),
            })
            .unwrap_or_default();

        // Return next server to forward the request to
        servers.get(idx).unwrap().clone()
    }
}

#[async_trait]
impl RoutingPolicy for SingleServerPolicy {
    async fn next(&self, request: &HttpRequest) -> String {
        self.server.clone()
    }
}
impl Loadbalancer {
    pub fn new(port: u16, policy: Box<SafeRoutingPolicy>) -> Self {
        Loadbalancer {
            port: port,
            data: web::Data::new(AppState {
                client: Client::new(),
                policy,
            }),
        }
    }
    pub fn uri(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    pub async fn run(&self) {
        let data = self.data.clone();

        HttpServer::new(move || {
            App::new()
                // Healthcheck endpoint always returning 200 OK
                .route("/health", web::get().to(HttpResponse::Ok))
                .default_service(web::to(Self::handler))
                // We add the initial instance of our shared app state
                .app_data(data.clone())
        })
        .bind(("127.0.0.1", self.port))
        .unwrap()
        .run()
        .await
        .unwrap();
    }

    async fn handler(
        req: HttpRequest,
        data: web::Data<AppState>,
        bytes: web::Bytes,
    ) -> Result<HttpResponse, Error> {
        let server = data.policy.next(&req).await;

        let uri = format!("{}{}", server, req.uri());

        let request_builder = data
            .client
            .request(
                reqwest::Method::from_bytes(req.method().clone().as_str().as_bytes()).unwrap(),
                uri,
            )
            // .headers(req.headers())
            .body(bytes);

        let response = request_builder.send().await?;

        let mut response_builder = HttpResponse::build(
            actix_web::http::StatusCode::from_u16(response.status().as_u16()).unwrap(),
        );
        for h in response.headers().iter() {
            response_builder.append_header((h.0.as_str(), h.1.as_bytes()));
        }
        let body = response.bytes().await?;
        Ok(response_builder.body(body))
    }
}
#[async_trait]
pub trait RoutingPolicy {
    async fn next(&self, request: &HttpRequest) -> String;
}

#[derive(Debug)]
struct Error {
    inner: reqwest::Error,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Forwarding error: {}", self.inner)
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Error { inner: value }
    }
}

impl ResponseError for Error {
    fn status_code(&self) -> actix_web::http::StatusCode {
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
    }

    fn error_response(&self) -> HttpResponse<actix_web::body::BoxBody> {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::html())
            .body(self.to_string())
    }
}

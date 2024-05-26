use std::fmt::Display;

use actix_web::{
    http::header::ContentType,
    web::{self},
    App, HttpRequest, HttpResponse, HttpServer, ResponseError,
};
use reqwest::Client;
pub struct Loadbalancer {
    port: u16,
    servers: Vec<String>,
}
struct AppState {
    servers: Vec<String>,
}
impl Loadbalancer {
    pub fn new(port: u16, uris: Vec<String>) -> Loadbalancer {
        Loadbalancer {
            port: port,
            servers: uris,
        }
    }
    pub fn uri(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    pub async fn run(&self) {
        let data = web::Data::new(AppState {
            servers: self.servers.clone(),
        });

        HttpServer::new(move || {
            App::new()
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
        let server = data.servers[0].clone();
        let uri = format!("{}{}", server, req.uri());

        let client = Client::new();
        let request_builder = client
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

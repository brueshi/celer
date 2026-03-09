use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{error, info};

use celer_pyhost::AsgiDispatcher;
use celer_runtime::NativeModule;

use crate::body::collect_body;
use crate::error::ServerError;
use crate::request::extract_headers;
use crate::router::{RouteDisposition, Router};
use crate::response;

/// Server configuration.
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Maximum request body size in bytes (default 1MB).
    pub max_body_size: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8000,
            max_body_size: 1_048_576,
        }
    }
}

/// The Celer HTTP server.
///
/// Serves compiled FastAPI routes over HTTP using hyper.
pub struct CelerServer {
    config: ServerConfig,
    router: Arc<Router>,
    native: Arc<NativeModule>,
}

impl CelerServer {
    pub fn new(config: ServerConfig, router: Router, native: NativeModule) -> Self {
        Self {
            config,
            router: Arc::new(router),
            native: Arc::new(native),
        }
    }

    /// Start the server and listen for incoming connections.
    ///
    /// This runs indefinitely until the process is terminated.
    pub async fn run(&self) -> Result<(), ServerError> {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| ServerError::BindFailed(format!("{e}")))?;

        let listener = TcpListener::bind(addr).await?;
        info!("celer-server listening on {addr}");

        loop {
            let (stream, remote) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let router = self.router.clone();
            let native = self.native.clone();

            tokio::task::spawn(async move {
                let service = service_fn(|req| {
                    handle_request(req, router.clone(), native.clone())
                });
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                {
                    error!("connection error from {remote}: {err}");
                }
            });
        }
    }
}

/// Hybrid server that routes requests to native AOT handlers or ASGI fallback.
///
/// Native-eligible requests bypass Python entirely.
/// All other requests are forwarded to the full Python ASGI app.
pub struct HybridServer {
    config: ServerConfig,
    router: Arc<Router>,
    native: Arc<NativeModule>,
    asgi: Arc<AsgiDispatcher>,
}

impl HybridServer {
    pub fn new(
        config: ServerConfig,
        router: Router,
        native: NativeModule,
        asgi: AsgiDispatcher,
    ) -> Self {
        Self {
            config,
            router: Arc::new(router),
            native: Arc::new(native),
            asgi: Arc::new(asgi),
        }
    }

    /// Start the hybrid server.
    pub async fn run(&self) -> Result<(), ServerError> {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| ServerError::BindFailed(format!("{e}")))?;

        let listener = TcpListener::bind(addr).await?;
        info!("celer hybrid server listening on {addr}");

        let max_body = self.config.max_body_size;

        loop {
            let (stream, remote) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let router = self.router.clone();
            let native = self.native.clone();
            let asgi = self.asgi.clone();

            tokio::task::spawn(async move {
                let service = service_fn(|req| {
                    handle_hybrid_request(
                        req,
                        router.clone(),
                        native.clone(),
                        asgi.clone(),
                        max_body,
                    )
                });
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                {
                    error!("connection error from {remote}: {err}");
                }
            });
        }
    }
}

async fn handle_request(
    req: Request<Incoming>,
    router: Arc<Router>,
    native: Arc<NativeModule>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let method = req.method().as_str();
    let path = req.uri().path();

    match router.match_route(method, path) {
        Some((route, params)) => {
            let args = match crate::request::convert_params(
                &params,
                &route.path_params,
                &route.param_types,
            ) {
                Ok(args) => args,
                Err(e) => return Ok(response::error_response(&e)),
            };

            match native.call_typed(&route.handler_name, &args, route.is_json) {
                Ok(value) => Ok(response::value_to_response(&value)),
                Err(e) => Ok(response::error_response(&format!("{e}"))),
            }
        }
        None => Ok(response::not_found_response()),
    }
}

async fn handle_hybrid_request(
    req: Request<Incoming>,
    router: Arc<Router>,
    native: Arc<NativeModule>,
    asgi: Arc<AsgiDispatcher>,
    max_body_size: usize,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query_string = req.uri().query().unwrap_or("").as_bytes().to_vec();

    let (disposition, params) = router.match_hybrid(&method, &path);

    match disposition {
        RouteDisposition::Native(route) => {
            let args = match crate::request::convert_params(
                &params,
                &route.path_params,
                &route.param_types,
            ) {
                Ok(args) => args,
                Err(e) => return Ok(response::error_response(&e)),
            };

            match native.call_typed(&route.handler_name, &args, route.is_json) {
                Ok(value) => Ok(response::value_to_response(&value)),
                Err(e) => Ok(response::error_response(&format!("{e}"))),
            }
        }
        RouteDisposition::Asgi => {
            let headers = extract_headers(req.headers());
            let body = match collect_body(req.into_body(), max_body_size).await {
                Ok(b) => b,
                Err(e) => return Ok(response::error_response(&format!("{e}"))),
            };

            match asgi.dispatch(&method, &path, &query_string, &headers, body).await {
                Ok(resp) => Ok(response::asgi_response_to_hyper(
                    resp.status,
                    &resp.headers,
                    resp.body,
                )),
                Err(e) => Ok(response::error_response(&format!("ASGI error: {e}"))),
            }
        }
    }
}

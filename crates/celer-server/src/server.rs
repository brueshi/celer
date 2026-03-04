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

use celer_runtime::NativeModule;

use crate::error::ServerError;
use crate::router::Router;
use crate::response;

/// Server configuration.
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8000,
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

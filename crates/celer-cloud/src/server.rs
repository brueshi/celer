use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::config::CloudConfig;
use crate::error::CloudError;
use crate::handlers;
use crate::job::JobStore;

pub struct CloudServer {
    config: Arc<CloudConfig>,
    store: Arc<JobStore>,
}

impl CloudServer {
    pub fn new(config: CloudConfig) -> Self {
        let store = JobStore::new(config.job_ttl_secs);
        Self {
            config: Arc::new(config),
            store: Arc::new(store),
        }
    }

    pub async fn run(&self) -> Result<(), CloudError> {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| CloudError::BindFailed(format!("{e}")))?;

        let listener = TcpListener::bind(addr).await?;
        info!("celer-cloud listening on {addr}");

        // Spawn background cleanup task
        let store_cleanup = self.store.clone();
        let ttl = self.config.job_ttl_secs;
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(ttl / 4)
                .max(std::time::Duration::from_secs(60));
            loop {
                tokio::time::sleep(interval).await;
                let removed = store_cleanup.cleanup_expired();
                if removed > 0 {
                    info!("cleaned up {removed} expired jobs");
                }
            }
        });

        loop {
            let (stream, _remote) = listener.accept().await?;
            let io = TokioIo::new(stream);
            let config = self.config.clone();
            let store = self.store.clone();

            tokio::task::spawn(async move {
                let service = service_fn(move |req| {
                    let config = config.clone();
                    let store = store.clone();
                    async move { route(req, store, config).await }
                });
                if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                    error!("connection error: {err}");
                }
            });
        }
    }
}

async fn route(
    req: Request<Incoming>,
    store: Arc<JobStore>,
    config: Arc<CloudConfig>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let response = match (method, path.as_str()) {
        (Method::GET, "/health") => handlers::handle_health().await,
        (Method::POST, "/compile") => handlers::handle_compile(req, store, config).await,
        _ if path.starts_with("/status/") => {
            let job_id = &path["/status/".len()..];
            handlers::handle_status(job_id, store).await
        }
        _ if path.starts_with("/download/") => {
            let job_id = &path["/download/".len()..];
            handlers::handle_download(job_id, store).await
        }
        _ => handlers::json_response(
            hyper::StatusCode::NOT_FOUND,
            serde_json::json!({"error": "not found"}),
        ),
    };

    Ok(response)
}

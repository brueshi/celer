use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use serde::Deserialize;

use crate::compiler;
use crate::config::CloudConfig;
use crate::job::{Job, JobStatus, JobStore};

#[derive(Deserialize)]
struct CompileRequest {
    source: String,
    #[serde(default = "default_module_name")]
    module_name: String,
}

fn default_module_name() -> String {
    "module".to_string()
}

pub fn json_response(status: StatusCode, body: serde_json::Value) -> Response<Full<Bytes>> {
    let body_str = serde_json::to_string(&body).unwrap_or_default();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body_str)))
        .unwrap()
}

pub async fn handle_health() -> Response<Full<Bytes>> {
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "status": "ok",
            "service": "celer-cloud"
        }),
    )
}

pub async fn handle_compile(
    req: Request<Incoming>,
    store: Arc<JobStore>,
    config: Arc<CloudConfig>,
) -> Response<Full<Bytes>> {
    // Read body
    let body = match req.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "error": format!("failed to read body: {e}")
                }),
            );
        }
    };

    // Check size
    if body.len() > config.max_source_bytes {
        return json_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            serde_json::json!({
                "error": format!(
                    "source too large: {} bytes (max: {})",
                    body.len(),
                    config.max_source_bytes
                )
            }),
        );
    }

    // Parse request
    let compile_req: CompileRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "error": format!("invalid request: {e}")
                }),
            );
        }
    };

    // Validate module name (alphanumeric + underscore only)
    if compile_req.module_name.is_empty()
        || !compile_req
            .module_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
    {
        return json_response(
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": "invalid module name: must be non-empty alphanumeric with underscores"
            }),
        );
    }

    // Create job
    let job_id = uuid::Uuid::new_v4().to_string();
    let response_job_id = job_id.clone();

    let job = Job {
        id: job_id.clone(),
        module_name: compile_req.module_name.clone(),
        status: JobStatus::Pending,
        created_at: std::time::Instant::now(),
        compile_time_ms: None,
        artifact_path: None,
        error: None,
    };
    store.insert(job);

    // Spawn compilation task
    let store_clone = store.clone();
    let timeout_secs = config.compile_timeout_secs;
    let module_name = compile_req.module_name;
    let source = compile_req.source;

    tokio::spawn(async move {
        store_clone.update_status(&job_id, JobStatus::Compiling);

        let job_id_inner = job_id.clone();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::task::spawn_blocking(move || {
                let output_dir = std::env::temp_dir().join("celer-cloud").join(&job_id_inner);
                std::fs::create_dir_all(&output_dir).ok();
                compiler::compile_to_shared_lib(&module_name, &source, &output_dir)
            }),
        )
        .await;

        match result {
            Ok(Ok(Ok((artifact_path, compile_time_ms)))) => {
                store_clone.mark_complete(&job_id, artifact_path, compile_time_ms);
            }
            Ok(Ok(Err(e))) => {
                store_clone.mark_failed(&job_id, format!("{e}"));
            }
            Ok(Err(e)) => {
                store_clone.mark_failed(&job_id, format!("task panic: {e}"));
            }
            Err(_) => {
                store_clone.mark_failed(
                    &job_id,
                    format!("compilation timed out after {timeout_secs}s"),
                );
            }
        }
    });

    json_response(
        StatusCode::ACCEPTED,
        serde_json::json!({
            "job_id": response_job_id,
            "status": "pending"
        }),
    )
}

pub async fn handle_status(job_id: &str, store: Arc<JobStore>) -> Response<Full<Bytes>> {
    match store.get(job_id) {
        Some(job) => {
            let mut resp = serde_json::json!({
                "job_id": job.id,
                "status": job.status,
                "module_name": job.module_name,
            });
            if let Some(ms) = job.compile_time_ms {
                resp["compile_time_ms"] = serde_json::json!(ms);
            }
            if let Some(ref err) = job.error {
                resp["error"] = serde_json::json!(err);
            }
            json_response(StatusCode::OK, resp)
        }
        None => json_response(
            StatusCode::NOT_FOUND,
            serde_json::json!({
                "error": format!("job not found: {job_id}")
            }),
        ),
    }
}

pub async fn handle_download(job_id: &str, store: Arc<JobStore>) -> Response<Full<Bytes>> {
    match store.get(job_id) {
        Some(job) => {
            if !matches!(job.status, JobStatus::Complete) {
                return json_response(
                    StatusCode::CONFLICT,
                    serde_json::json!({
                        "error": "job not complete"
                    }),
                );
            }
            match &job.artifact_path {
                Some(path) => match std::fs::read(path) {
                    Ok(data) => Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/octet-stream")
                        .header(
                            "content-disposition",
                            format!(
                                "attachment; filename=\"{}.{}\"",
                                job.module_name,
                                celer_runtime::shared_lib_extension()
                            ),
                        )
                        .body(Full::new(Bytes::from(data)))
                        .unwrap(),
                    Err(e) => json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        serde_json::json!({
                            "error": format!("failed to read artifact: {e}")
                        }),
                    ),
                },
                None => json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    serde_json::json!({
                        "error": "artifact path missing"
                    }),
                ),
            }
        }
        None => json_response(
            StatusCode::NOT_FOUND,
            serde_json::json!({
                "error": format!("job not found: {job_id}")
            }),
        ),
    }
}

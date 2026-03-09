use std::sync::Arc;

use bytes::Bytes;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyModule};
use tracing::debug;

use crate::error::PyHostError;
use crate::host::PythonHost;
use crate::scope::build_http_scope;

/// Response collected from ASGI `send` events.
#[derive(Debug, Clone)]
pub struct AsgiResponse {
    pub status: u16,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub body: Bytes,
}

/// Dispatches HTTP requests through the Python ASGI app.
///
/// Each dispatch:
/// 1. Builds an ASGI scope dict
/// 2. Creates `receive` callable (yields request body)
/// 3. Creates `send` callable (collects response)
/// 4. Calls `await app(scope, receive, send)` on the persistent event loop
pub struct AsgiDispatcher {
    host: Arc<PythonHost>,
    server_host: String,
    server_port: u16,
}

impl AsgiDispatcher {
    pub fn new(host: Arc<PythonHost>, server_host: String, server_port: u16) -> Self {
        Self {
            host,
            server_host,
            server_port,
        }
    }

    /// Dispatch an HTTP request to the ASGI app and collect the response.
    pub async fn dispatch(
        &self,
        method: &str,
        path: &str,
        query_string: &[u8],
        headers: &[(Vec<u8>, Vec<u8>)],
        body: Bytes,
    ) -> Result<AsgiResponse, PyHostError> {
        let method = method.to_string();
        let path = path.to_string();
        let query_string = query_string.to_vec();
        let headers = headers.to_vec();
        let server_host = self.server_host.clone();
        let server_port = self.server_port;
        let host = self.host.clone();

        // Run Python work in a blocking task to avoid holding the GIL on async runtime
        tokio::task::spawn_blocking(move || {
            dispatch_blocking(
                &host,
                &method,
                &path,
                &query_string,
                &headers,
                body,
                &server_host,
                server_port,
            )
        })
        .await
        .map_err(|e| PyHostError::DispatchFailed(format!("task join error: {e}")))?
    }
}

/// Synchronous ASGI dispatch executed inside `spawn_blocking`.
fn dispatch_blocking(
    host: &PythonHost,
    method: &str,
    path: &str,
    query_string: &[u8],
    headers: &[(Vec<u8>, Vec<u8>)],
    body: Bytes,
    server_host: &str,
    server_port: u16,
) -> Result<AsgiResponse, PyHostError> {
    Python::with_gil(|py| {
        let scope = build_http_scope(
            py,
            method,
            path,
            query_string,
            headers,
            server_host,
            server_port,
        )?;

        debug!(method, path, "dispatching ASGI request");

        // Build the dispatch coroutine via a helper Python snippet
        let helper = PyModule::from_code(
            py,
            c_str!(ASGI_HELPER_CODE),
            c"celer_asgi_helper.py",
            c"celer_asgi_helper",
        )
        .map_err(|e| PyHostError::ProtocolError(format!("helper load failed: {e}")))?;

        let body_bytes = PyBytes::new(py, &body);

        let dispatch_fn = helper
            .getattr("dispatch_asgi")
            .map_err(PyHostError::PythonError)?;

        let coro = dispatch_fn
            .call1((host.app().bind(py), &scope, body_bytes))
            .map_err(|e| PyHostError::DispatchFailed(e.to_string()))?;

        // Submit coroutine to the persistent event loop
        let asyncio = py.import("asyncio").map_err(PyHostError::PythonError)?;
        let future = asyncio
            .call_method1(
                "run_coroutine_threadsafe",
                (coro, host.event_loop().bind(py)),
            )
            .map_err(PyHostError::PythonError)?;

        // Store the future as a PyObject so we can use it across GIL boundaries
        let future_obj = future.unbind();

        // Release GIL while waiting, then re-acquire to get result
        let result = py.allow_threads(|| {
            Python::with_gil(|py| {
                future_obj
                    .bind(py)
                    .call_method0("result")
                    .map_err(|e| PyHostError::DispatchFailed(e.to_string()))
                    .and_then(|r| extract_response(&r))
            })
        })?;

        Ok(result)
    })
}

/// Extract an AsgiResponse from the Python result dict.
fn extract_response(result: &Bound<'_, PyAny>) -> Result<AsgiResponse, PyHostError> {
    let status: u16 = result
        .get_item("status")
        .map_err(|e| PyHostError::ProtocolError(format!("missing status: {e}")))?
        .extract()
        .map_err(|e| PyHostError::ProtocolError(format!("invalid status: {e}")))?;

    let headers_raw = result
        .get_item("headers")
        .map_err(|e| PyHostError::ProtocolError(format!("missing headers: {e}")))?;
    let headers_list: &Bound<'_, PyList> = headers_raw
        .downcast()
        .map_err(|e| PyHostError::ProtocolError(format!("headers not a list: {e}")))?;

    let mut headers = Vec::with_capacity(headers_list.len());
    for pair in headers_list.iter() {
        let pair_list: &Bound<'_, PyList> = pair
            .downcast()
            .map_err(|e| PyHostError::ProtocolError(format!("header pair not a list: {e}")))?;
        if pair_list.len() != 2 {
            return Err(PyHostError::ProtocolError("header pair must have 2 elements".into()));
        }
        let name: Vec<u8> = pair_list.get_item(0)?.extract()?;
        let value: Vec<u8> = pair_list.get_item(1)?.extract()?;
        headers.push((name, value));
    }

    let body_bytes: Vec<u8> = result
        .get_item("body")
        .map_err(|e| PyHostError::ProtocolError(format!("missing body: {e}")))?
        .extract()
        .map_err(|e| PyHostError::ProtocolError(format!("invalid body: {e}")))?;

    Ok(AsgiResponse {
        status,
        headers,
        body: Bytes::from(body_bytes),
    })
}

/// Python helper that dispatches an ASGI request and collects the response.
const ASGI_HELPER_CODE: &str = r#"
async def dispatch_asgi(app, scope, body_bytes):
    """Call the ASGI app and collect the response."""
    response = {"status": 500, "headers": [], "body": b""}
    body_sent = False

    async def receive():
        nonlocal body_sent
        if not body_sent:
            body_sent = True
            return {"type": "http.request", "body": body_bytes, "more_body": False}
        raise RuntimeError("receive called after body was consumed")

    async def send(message):
        nonlocal response
        msg_type = message["type"]
        if msg_type == "http.response.start":
            response["status"] = message["status"]
            response["headers"] = message.get("headers", [])
        elif msg_type == "http.response.body":
            body = message.get("body", b"")
            if response["body"]:
                response["body"] += body
            else:
                response["body"] = body

    await app(scope, receive, send)
    return response
"#;

/// Create a CStr from a string literal at compile time.
macro_rules! c_str {
    ($s:expr) => {{
        let boxed = Box::new(format!("{}\0", $s));
        let leaked: &'static str = Box::leak(boxed);
        unsafe { std::ffi::CStr::from_bytes_with_nul_unchecked(leaked.as_bytes()) }
    }};
}

use c_str;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asgi_helper_code_compiles() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let result = PyModule::from_code(
                py,
                c_str!(ASGI_HELPER_CODE),
                c"test_helper.py",
                c"test_helper",
            );
            assert!(result.is_ok(), "ASGI helper code failed to compile: {:?}", result.err());
        });
    }
}

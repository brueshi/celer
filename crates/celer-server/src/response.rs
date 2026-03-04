use bytes::Bytes;
use http_body_util::Full;
use hyper::Response;

use celer_runtime::Value;

/// Convert a native function return value into an HTTP response.
pub fn value_to_response(value: &Value) -> Response<Full<Bytes>> {
    match value {
        Value::Json(json) => Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(json.clone())))
            .unwrap(),
        Value::Str(s) => Response::builder()
            .status(200)
            .header("content-type", "text/plain")
            .body(Full::new(Bytes::from(s.clone())))
            .unwrap(),
        Value::I64(n) => json_response(&format!("{n}")),
        Value::F64(f) => json_response(&format!("{f}")),
        Value::Bool(b) => json_response(if *b { "true" } else { "false" }),
        Value::None => json_response("null"),
    }
}

fn json_response(body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}

/// 404 Not Found response.
pub fn not_found_response() -> Response<Full<Bytes>> {
    Response::builder()
        .status(404)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(
            r#"{"error":"not found"}"#,
        )))
        .unwrap()
}

/// 500 Internal Server Error response.
pub fn error_response(msg: &str) -> Response<Full<Bytes>> {
    let body = format!(r#"{{"error":"{}"}}"#, msg.replace('"', "\\\""));
    Response::builder()
        .status(500)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}

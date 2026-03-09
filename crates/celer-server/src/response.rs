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

/// Convert an ASGI response into a hyper HTTP response.
pub fn asgi_response_to_hyper(
    status: u16,
    headers: &[(Vec<u8>, Vec<u8>)],
    body: Bytes,
) -> Response<Full<Bytes>> {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers {
        if let (Ok(name_str), Ok(value_str)) = (
            std::str::from_utf8(name),
            hyper::header::HeaderValue::from_bytes(value),
        ) {
            builder = builder.header(name_str, value_str);
        }
    }
    builder.body(Full::new(body)).unwrap_or_else(|_| {
        Response::builder()
            .status(500)
            .body(Full::new(Bytes::from("internal error")))
            .unwrap()
    })
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

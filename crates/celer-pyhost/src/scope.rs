use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString};

/// Build an ASGI HTTP scope dict from request components.
///
/// Scope format (ASGI 3.0):
/// ```python
/// {
///     "type": "http",
///     "asgi": {"version": "3.0", "spec_version": "2.4"},
///     "http_version": "1.1",
///     "method": "GET",
///     "path": "/items/42",
///     "raw_path": b"/items/42",
///     "root_path": "",
///     "query_string": b"limit=10",
///     "headers": [(b"host", b"localhost:8000"), ...],
///     "server": ("127.0.0.1", 8000),
/// }
/// ```
pub fn build_http_scope<'py>(
    py: Python<'py>,
    method: &str,
    path: &str,
    query_string: &[u8],
    headers: &[(Vec<u8>, Vec<u8>)],
    server_host: &str,
    server_port: u16,
) -> PyResult<Bound<'py, PyDict>> {
    let scope = PyDict::new(py);

    scope.set_item("type", "http")?;

    let asgi_dict = PyDict::new(py);
    asgi_dict.set_item("version", "3.0")?;
    asgi_dict.set_item("spec_version", "2.4")?;
    scope.set_item("asgi", asgi_dict)?;

    scope.set_item("http_version", "1.1")?;
    scope.set_item("method", method)?;
    scope.set_item("path", path)?;
    scope.set_item("raw_path", PyBytes::new(py, path.as_bytes()))?;
    scope.set_item("root_path", "")?;
    scope.set_item("query_string", PyBytes::new(py, query_string))?;

    let py_headers = PyList::empty(py);
    for (name, value) in headers {
        let pair = PyList::empty(py);
        pair.append(PyBytes::new(py, name))?;
        pair.append(PyBytes::new(py, value))?;
        py_headers.append(pair)?;
    }
    scope.set_item("headers", py_headers)?;

    let server_tuple = (
        PyString::new(py, server_host),
        server_port,
    );
    scope.set_item("server", server_tuple)?;

    Ok(scope)
}

/// Build an ASGI `receive` event for an HTTP request body.
///
/// Returns: `{"type": "http.request", "body": <bytes>, "more_body": False}`
pub fn build_request_event<'py>(
    py: Python<'py>,
    body: &[u8],
) -> PyResult<Bound<'py, PyDict>> {
    let event = PyDict::new(py);
    event.set_item("type", "http.request")?;
    event.set_item("body", PyBytes::new(py, body))?;
    event.set_item("more_body", false)?;
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_scope_basic() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let scope = build_http_scope(
                py,
                "GET",
                "/test",
                b"key=val",
                &[(b"host".to_vec(), b"localhost:8000".to_vec())],
                "127.0.0.1",
                8000,
            )
            .unwrap();

            let ty: String = scope.get_item("type").unwrap().unwrap().extract().unwrap();
            assert_eq!(ty, "http");

            let method: String = scope.get_item("method").unwrap().unwrap().extract().unwrap();
            assert_eq!(method, "GET");

            let path: String = scope.get_item("path").unwrap().unwrap().extract().unwrap();
            assert_eq!(path, "/test");
        });
    }

    #[test]
    fn build_request_event_with_body() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let event = build_request_event(py, b"hello").unwrap();
            let ty: String = event.get_item("type").unwrap().unwrap().extract().unwrap();
            assert_eq!(ty, "http.request");

            let more: bool = event.get_item("more_body").unwrap().unwrap().extract().unwrap();
            assert!(!more);
        });
    }

    #[test]
    fn build_scope_empty_query() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let scope = build_http_scope(
                py, "POST", "/users", b"", &[], "0.0.0.0", 3000,
            )
            .unwrap();

            let method: String = scope.get_item("method").unwrap().unwrap().extract().unwrap();
            assert_eq!(method, "POST");
        });
    }
}

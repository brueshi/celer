use std::ffi::CString;

use pyo3::prelude::*;

use crate::error::RuntimeError;

/// Execute a Python function via CPython and return the JSON-serialized result.
pub fn run_python_function(
    source: &str,
    function_name: &str,
    arg: Option<i64>,
) -> Result<String, RuntimeError> {
    Python::with_gil(|py| {
        let locals = pyo3::types::PyDict::new(py);

        // Execute the source to define the function
        let source_c = CString::new(source.trim())
            .map_err(|e| RuntimeError::ExecutionFailed(format!("invalid source: {e}")))?;
        py.run(
            std::ffi::CStr::from_bytes_with_nul(&[source_c.as_bytes(), b"\0"].concat())
                .unwrap_or(&source_c),
            None,
            Some(&locals),
        )
        .map_err(RuntimeError::from)?;

        // Build the call expression
        let call_code = match arg {
            None => format!("import json; __result = json.dumps({}())\0", function_name),
            Some(val) => format!(
                "import json; __result = json.dumps({}({}))\0",
                function_name, val
            ),
        };
        let call_c = CString::new(call_code.trim_end_matches('\0'))
            .map_err(|e| RuntimeError::ExecutionFailed(format!("invalid call code: {e}")))?;

        py.run(&call_c, None, Some(&locals))
            .map_err(RuntimeError::from)?;

        let result: String = locals
            .get_item("__result")
            .map_err(RuntimeError::from)?
            .ok_or_else(|| {
                RuntimeError::ExecutionFailed("__result not set after execution".into())
            })?
            .extract()
            .map_err(RuntimeError::from)?;

        Ok(result)
    })
}

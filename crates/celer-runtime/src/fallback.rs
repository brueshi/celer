use std::collections::HashSet;
use std::path::Path;

use crate::cpython_runner::run_python_function;
use crate::error::RuntimeError;
use crate::native::{NativeModule, Value};

/// Dispatches function calls to either native code or CPython fallback
/// based on compilability analysis.
pub struct FallbackDispatcher {
    native: Option<NativeModule>,
    python_source: String,
    compiled_functions: HashSet<String>,
    /// Functions using JSON output-param calling convention (return dict).
    json_functions: HashSet<String>,
}

impl FallbackDispatcher {
    /// Create a new dispatcher with native module and Python source for fallback.
    pub fn new(
        native: Option<NativeModule>,
        python_source: String,
        compiled_functions: HashSet<String>,
        json_functions: HashSet<String>,
    ) -> Self {
        Self {
            native,
            python_source,
            compiled_functions,
            json_functions,
        }
    }

    /// Load native module from a shared library path.
    pub fn with_library(
        lib_path: &Path,
        python_source: String,
        compiled_functions: HashSet<String>,
        json_functions: HashSet<String>,
    ) -> Result<Self, RuntimeError> {
        let native = unsafe { NativeModule::load(lib_path)? };
        Ok(Self {
            native: Some(native),
            python_source,
            compiled_functions,
            json_functions,
        })
    }

    /// Call a function, routing to native or CPython based on compilability.
    pub fn call(&self, function_name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        if self.compiled_functions.contains(function_name)
            && let Some(native) = &self.native
        {
            let is_json = self.json_functions.contains(function_name);
            return native.call_typed(function_name, args, is_json);
        }

        // Fallback to CPython
        let arg = match args.first() {
            Some(Value::I64(v)) => Some(*v),
            None => None,
            _ => {
                return Err(RuntimeError::ExecutionFailed(
                    "CPython fallback only supports no-arg or single i64 arg".into(),
                ))
            }
        };

        let result = run_python_function(&self.python_source, function_name, arg)?;
        Ok(Value::Json(result))
    }

    pub fn is_compiled(&self, function_name: &str) -> bool {
        self.compiled_functions.contains(function_name)
    }

    pub fn compiled_count(&self) -> usize {
        self.compiled_functions.len()
    }
}

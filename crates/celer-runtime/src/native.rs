use std::path::Path;

use libloading::{Library, Symbol};

use crate::error::RuntimeError;

/// Runtime value for generic function dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    Json(String),
    None,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::I64(v) => write!(f, "{v}"),
            Value::F64(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Str(v) => write!(f, "{v}"),
            Value::Json(v) => write!(f, "{v}"),
            Value::None => write!(f, "None"),
        }
    }
}

/// A loaded native module compiled by Celer.
pub struct NativeModule {
    _lib: Library,
}

/// Function pointer type for no-arg handlers returning JSON via output params.
type NoArgFn = unsafe extern "C" fn(*mut *const u8, *mut u64);

/// Function pointer type for single-int-arg handlers returning JSON via output params.
type OneIntFn = unsafe extern "C" fn(i64, *mut *const u8, *mut u64);

/// Scalar function types for non-JSON returns.
type ScalarNoArgI64Fn = unsafe extern "C" fn() -> i64;
type ScalarOneIntI64Fn = unsafe extern "C" fn(i64) -> i64;
type ScalarTwoIntI64Fn = unsafe extern "C" fn(i64, i64) -> i64;

impl NativeModule {
    /// Load a compiled shared library (.dylib / .so).
    ///
    /// # Safety
    /// The library must be a valid Celer-compiled shared library.
    pub unsafe fn load(path: &Path) -> Result<Self, RuntimeError> {
        let lib = unsafe {
            Library::new(path).map_err(|e| {
                RuntimeError::ExecutionFailed(format!("failed to load {}: {e}", path.display()))
            })?
        };
        Ok(Self { _lib: lib })
    }

    /// Call a no-argument function that returns a JSON string.
    pub fn call_no_args(&self, name: &str) -> Result<String, RuntimeError> {
        unsafe {
            let func: Symbol<NoArgFn> = self._lib.get(name.as_bytes()).map_err(|e| {
                RuntimeError::ExecutionFailed(format!("symbol '{name}' not found: {e}"))
            })?;

            let mut ptr: *const u8 = std::ptr::null();
            let mut len: u64 = 0;

            func(&mut ptr, &mut len);

            if ptr.is_null() || len == 0 {
                return Err(RuntimeError::ExecutionFailed(
                    "function returned null pointer or zero length".into(),
                ));
            }

            let slice = std::slice::from_raw_parts(ptr, len as usize);
            let json = std::str::from_utf8(slice).map_err(|e| {
                RuntimeError::ExecutionFailed(format!("invalid UTF-8 in output: {e}"))
            })?;

            Ok(json.to_string())
        }
    }

    /// Call a function with a single i64 argument that returns a JSON string.
    pub fn call_one_int(&self, name: &str, arg: i64) -> Result<String, RuntimeError> {
        unsafe {
            let func: Symbol<OneIntFn> = self._lib.get(name.as_bytes()).map_err(|e| {
                RuntimeError::ExecutionFailed(format!("symbol '{name}' not found: {e}"))
            })?;

            let mut ptr: *const u8 = std::ptr::null();
            let mut len: u64 = 0;

            func(arg, &mut ptr, &mut len);

            if ptr.is_null() || len == 0 {
                return Err(RuntimeError::ExecutionFailed(
                    "function returned null pointer or zero length".into(),
                ));
            }

            let slice = std::slice::from_raw_parts(ptr, len as usize);
            let json = std::str::from_utf8(slice).map_err(|e| {
                RuntimeError::ExecutionFailed(format!("invalid UTF-8 in output: {e}"))
            })?;

            Ok(json.to_string())
        }
    }

    /// Type-safe call that dispatches based on known calling convention.
    pub fn call_typed(
        &self,
        name: &str,
        args: &[Value],
        is_json: bool,
    ) -> Result<Value, RuntimeError> {
        if is_json {
            self.try_json_call(name, args)
        } else {
            self.call_scalar(name, args)
        }
    }

    /// Call a function known to use scalar calling convention.
    pub fn call(&self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        self.call_scalar(name, args)
    }

    fn call_scalar(&self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        match args.len() {
            0 => self.call_scalar_no_args(name),
            1 => match &args[0] {
                Value::I64(v) => self.call_scalar_one_int(name, *v),
                _ => Err(RuntimeError::ExecutionFailed(format!(
                    "unsupported argument type for {name}"
                ))),
            },
            2 => match (&args[0], &args[1]) {
                (Value::I64(a), Value::I64(b)) => self.call_scalar_two_int(name, *a, *b),
                _ => Err(RuntimeError::ExecutionFailed(format!(
                    "unsupported argument types for {name}"
                ))),
            },
            _ => Err(RuntimeError::ExecutionFailed(format!(
                "too many arguments for native call to {name}"
            ))),
        }
    }

    fn try_json_call(&self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        match args.len() {
            0 => self.call_no_args(name).map(Value::Json),
            1 => match &args[0] {
                Value::I64(v) => self.call_one_int(name, *v).map(Value::Json),
                _ => Err(RuntimeError::ExecutionFailed(
                    "unsupported arg type for json call".into(),
                )),
            },
            _ => Err(RuntimeError::ExecutionFailed(
                "too many args for json call".into(),
            )),
        }
    }

    fn call_scalar_no_args(&self, name: &str) -> Result<Value, RuntimeError> {
        unsafe {
            let func: Symbol<ScalarNoArgI64Fn> =
                self._lib.get(name.as_bytes()).map_err(|e| {
                    RuntimeError::ExecutionFailed(format!("symbol '{name}' not found: {e}"))
                })?;
            Ok(Value::I64(func()))
        }
    }

    fn call_scalar_one_int(&self, name: &str, arg: i64) -> Result<Value, RuntimeError> {
        unsafe {
            let func: Symbol<ScalarOneIntI64Fn> =
                self._lib.get(name.as_bytes()).map_err(|e| {
                    RuntimeError::ExecutionFailed(format!("symbol '{name}' not found: {e}"))
                })?;
            Ok(Value::I64(func(arg)))
        }
    }

    fn call_scalar_two_int(&self, name: &str, a: i64, b: i64) -> Result<Value, RuntimeError> {
        unsafe {
            let func: Symbol<ScalarTwoIntI64Fn> =
                self._lib.get(name.as_bytes()).map_err(|e| {
                    RuntimeError::ExecutionFailed(format!("symbol '{name}' not found: {e}"))
                })?;
            Ok(Value::I64(func(a, b)))
        }
    }
}

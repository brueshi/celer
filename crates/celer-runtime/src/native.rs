use std::path::Path;

use libloading::{Library, Symbol};

use crate::error::RuntimeError;

/// A loaded native module compiled by Celer.
pub struct NativeModule {
    _lib: Library,
}

/// Function pointer type for no-arg handlers returning JSON via output params.
type NoArgFn = unsafe extern "C" fn(*mut *const u8, *mut u64);

/// Function pointer type for single-int-arg handlers returning JSON via output params.
type OneIntFn = unsafe extern "C" fn(i64, *mut *const u8, *mut u64);

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
}

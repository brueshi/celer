use std::path::Path;
use std::sync::Arc;
use std::thread;

use pyo3::prelude::*;
use pyo3::types::PyModule;
use tracing::{debug, info};

use crate::error::PyHostError;

/// Persistent Python interpreter that holds a reference to the user's ASGI app.
///
/// Initializes once at startup:
/// - Adds module directory to `sys.path`
/// - Imports the user's module
/// - Grabs reference to the ASGI app object
/// - Creates a dedicated asyncio event loop on a background thread
pub struct PythonHost {
    app: PyObject,
    event_loop: PyObject,
    _loop_thread: thread::JoinHandle<()>,
}

impl PythonHost {
    /// Boot the Python interpreter and import `module_name:app_attr`.
    ///
    /// `module_path` is the directory containing the module file.
    /// `module_name` is the Python module name (e.g., "main").
    /// `app_attr` is the attribute name on the module (e.g., "app").
    pub fn new(
        module_path: &Path,
        module_name: &str,
        app_attr: &str,
    ) -> Result<Self, PyHostError> {
        pyo3::prepare_freethreaded_python();

        let (app, event_loop, loop_thread) = Python::with_gil(|py| -> Result<_, PyHostError> {
            // Add module directory to sys.path
            let sys = py.import("sys").map_err(PyHostError::PythonError)?;
            let path_list = sys
                .getattr("path")
                .map_err(PyHostError::PythonError)?;

            let module_dir = module_path
                .to_str()
                .ok_or_else(|| PyHostError::InitFailed("invalid module path".into()))?;

            path_list
                .call_method1("insert", (0, module_dir))
                .map_err(PyHostError::PythonError)?;

            debug!(module_dir, "added to sys.path");

            // Import user module
            let user_module = PyModule::import(py, module_name)
                .map_err(|e| PyHostError::ImportFailed(format!("{module_name}: {e}")))?;

            info!(module_name, "imported user module");

            // Get app object
            let app = user_module
                .getattr(app_attr)
                .map_err(|e| {
                    PyHostError::AppNotFound(format!("{module_name}.{app_attr}: {e}"))
                })?
                .unbind();

            info!(app_attr, "found ASGI app object");

            // Create asyncio event loop on a background thread
            let asyncio = py.import("asyncio").map_err(PyHostError::PythonError)?;
            let loop_obj = asyncio
                .call_method0("new_event_loop")
                .map_err(PyHostError::PythonError)?
                .unbind();

            let loop_clone = loop_obj.clone_ref(py);

            // Spawn a thread to run the event loop forever
            let loop_thread = thread::Builder::new()
                .name("celer-asyncio".into())
                .spawn(move || {
                    Python::with_gil(|py| {
                        let loop_ref = loop_clone.bind(py);
                        if let Err(e) = loop_ref.call_method0("run_forever") {
                            tracing::error!("asyncio event loop crashed: {e}");
                        }
                    });
                })
                .map_err(|e| PyHostError::EventLoopError(e.to_string()))?;

            info!("asyncio event loop started on background thread");

            Ok((app, loop_obj, loop_thread))
        })?;

        Ok(Self {
            app,
            event_loop,
            _loop_thread: loop_thread,
        })
    }

    /// Get a reference to the ASGI app PyObject.
    pub fn app(&self) -> &PyObject {
        &self.app
    }

    /// Get a reference to the asyncio event loop.
    pub fn event_loop(&self) -> &PyObject {
        &self.event_loop
    }

    /// Submit a coroutine to the event loop thread-safely.
    ///
    /// Uses `asyncio.run_coroutine_threadsafe(coro, loop)` and blocks
    /// until the result is available.
    pub fn run_coroutine<F, R>(&self, make_coro: F) -> Result<R, PyHostError>
    where
        F: FnOnce(Python<'_>) -> PyResult<PyObject>,
        R: for<'py> FromPyObject<'py>,
    {
        Python::with_gil(|py| {
            let coro = make_coro(py).map_err(PyHostError::PythonError)?;
            let asyncio = py.import("asyncio").map_err(PyHostError::PythonError)?;

            let future = asyncio
                .call_method1(
                    "run_coroutine_threadsafe",
                    (coro, self.event_loop.bind(py)),
                )
                .map_err(PyHostError::PythonError)?;

            // Block until result (with GIL released during wait)
            let result = future
                .call_method0("result")
                .map_err(|e| PyHostError::DispatchFailed(e.to_string()))?;

            result.extract::<R>().map_err(PyHostError::PythonError)
        })
    }

    /// Wrap the host in an Arc for shared ownership across handlers.
    pub fn into_shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

impl Drop for PythonHost {
    fn drop(&mut self) {
        // Stop the event loop so the background thread can exit
        Python::with_gil(|py| {
            let loop_ref = self.event_loop.bind(py);
            if let Err(e) = loop_ref.call_method0("call_soon_threadsafe") {
                // call_soon_threadsafe with stop callback
                debug!("event loop stop failed (may already be stopped): {e}");
            }
            // Use the stop method via call_soon_threadsafe
            let _ = loop_ref.call_method1(
                "call_soon_threadsafe",
                (loop_ref.getattr("stop").unwrap_or(loop_ref.clone()),),
            );
        });
    }
}

use crate::compiled::CompiledModule;
use crate::config::RuntimeConfig;
use crate::error::RuntimeError;
use pyo3::prelude::*;

/// Bridge between compiled Celer modules and CPython.
pub struct CpythonBridge {
    config: RuntimeConfig,
}

impl CpythonBridge {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    /// Import a Python module by name using the embedded interpreter.
    pub fn import_module(&self, name: &str) -> Result<(), RuntimeError> {
        Python::with_gil(|py| {
            py.import(name).map_err(RuntimeError::from)?;
            Ok(())
        })
    }

    /// Execute a compiled module by loading the native shared library.
    pub fn execute(&self, module: &CompiledModule) -> Result<crate::native::Value, RuntimeError> {
        let native = unsafe { crate::NativeModule::load(&module.object_path)? };
        let entry = module
            .entry_point
            .as_deref()
            .ok_or_else(|| RuntimeError::ExecutionFailed("no entry point specified".into()))?;
        native.call(entry, &[])
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }
}

impl Default for CpythonBridge {
    fn default() -> Self {
        Self::new(RuntimeConfig::default())
    }
}

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

    /// Execute a compiled module (stub -- will invoke native code in the future).
    pub fn execute(&self, _module: &CompiledModule) -> Result<(), RuntimeError> {
        // TODO: load and execute the compiled object file
        tracing::info!("execute called (stub)");
        Ok(())
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

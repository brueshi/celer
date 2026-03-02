use std::path::Path;
use std::process::Command;

use crate::error::RuntimeError;

/// Link an object file into a shared library using the system C compiler.
pub fn link_shared(object_path: &Path, output_path: &Path) -> Result<(), RuntimeError> {
    let status = Command::new("cc")
        .arg("-shared")
        .arg("-o")
        .arg(output_path)
        .arg(object_path)
        .status()
        .map_err(|e| RuntimeError::ExecutionFailed(format!("failed to run linker: {e}")))?;

    if !status.success() {
        return Err(RuntimeError::ExecutionFailed(format!(
            "linker failed with exit code: {:?}",
            status.code()
        )));
    }

    Ok(())
}

/// Determine the shared library extension for the current platform.
pub fn shared_lib_extension() -> &'static str {
    if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    }
}

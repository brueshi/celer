//! Flask framework adapter for Celer (Phase 2 - not yet implemented).

pub mod error;

use celer_hir::Module;
use error::FlaskError;

pub struct FlaskAdapter;

impl FlaskAdapter {
    /// Detect Flask route patterns in a module. (Phase 2)
    pub fn extract_routes(_module: &Module) -> Result<Vec<()>, FlaskError> {
        Err(FlaskError::NotImplemented)
    }
}

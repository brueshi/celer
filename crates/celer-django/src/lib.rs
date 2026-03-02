//! Django framework adapter for Celer (Phase 2 - not yet implemented).

pub mod error;

use celer_hir::Module;
use error::DjangoError;

pub struct DjangoAdapter;

impl DjangoAdapter {
    /// Detect Django URL patterns in a module. (Phase 2)
    pub fn extract_routes(_module: &Module) -> Result<Vec<()>, DjangoError> {
        Err(DjangoError::NotImplemented)
    }
}

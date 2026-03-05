pub mod adapter;
pub mod error;
pub mod patterns;

use celer_adapter_core::FrameworkAdapter;
use celer_hir::Module;

pub use adapter::{detect_django, extract_routes};
pub use error::DjangoError;
pub use patterns::{normalize_django_path, DjangoPathParam};

pub struct DjangoAdapter;

impl FrameworkAdapter for DjangoAdapter {
    fn name(&self) -> &'static str {
        "Django"
    }

    fn detect(&self, module: &Module) -> bool {
        detect_django(module)
    }

    fn extract_routes(
        &self,
        module: &Module,
    ) -> Result<Vec<celer_adapter_core::RouteInfo>, Box<dyn std::error::Error>> {
        Ok(adapter::extract_routes(module)?)
    }
}

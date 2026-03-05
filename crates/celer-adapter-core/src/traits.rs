use celer_hir::Module;
use crate::route::RouteInfo;

/// Trait for framework adapters that detect and extract routes from Python modules.
pub trait FrameworkAdapter {
    /// Human-readable name of the framework.
    fn name(&self) -> &'static str;

    /// Returns true if this adapter detects its framework patterns in the module.
    fn detect(&self, module: &Module) -> bool;

    /// Extract all routes from the module.
    fn extract_routes(&self, module: &Module) -> Result<Vec<RouteInfo>, Box<dyn std::error::Error>>;
}

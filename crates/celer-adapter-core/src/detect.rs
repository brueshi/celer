use celer_hir::Module;
use crate::route::RouteInfo;
use crate::traits::FrameworkAdapter;

/// Try each adapter in priority order. Return the first that detects its framework.
pub fn detect_framework<'a>(
    module: &Module,
    adapters: &'a [Box<dyn FrameworkAdapter>],
) -> Option<&'a dyn FrameworkAdapter> {
    adapters.iter().find(|a| a.detect(module)).map(|a| a.as_ref())
}

/// Detect framework and extract routes in one step.
pub fn detect_and_extract(
    module: &Module,
    adapters: &[Box<dyn FrameworkAdapter>],
) -> Result<(String, Vec<RouteInfo>), Box<dyn std::error::Error>> {
    for adapter in adapters {
        if adapter.detect(module) {
            let routes = adapter.extract_routes(module)?;
            return Ok((adapter.name().to_string(), routes));
        }
    }
    Err("no supported framework detected".into())
}

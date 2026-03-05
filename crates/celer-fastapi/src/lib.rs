pub mod adapter;
pub mod error;
pub mod route;

pub use adapter::FastApiAdapter;
pub use error::FastApiError;
// Re-export core types for backward compatibility
pub use celer_adapter_core::{HttpMethod, ParamSource, RouteInfo, RouteParam};

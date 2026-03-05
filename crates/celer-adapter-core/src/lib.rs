pub mod detect;
pub mod error;
pub mod route;
pub mod traits;

pub use error::AdapterError;
pub use route::{HttpMethod, ParamSource, RouteInfo, RouteParam};
pub use traits::FrameworkAdapter;

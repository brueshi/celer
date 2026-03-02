use celer_hir::Function;

/// HTTP methods supported by FastAPI routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

/// Extracted route information from a FastAPI endpoint.
#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub method: HttpMethod,
    pub path: String,
    pub handler: Function,
    pub response_model: Option<String>,
}

impl RouteInfo {
    pub fn new(method: HttpMethod, path: impl Into<String>, handler: Function) -> Self {
        Self {
            method,
            path: path.into(),
            handler,
            response_model: None,
        }
    }

    pub fn with_response_model(mut self, model: impl Into<String>) -> Self {
        self.response_model = Some(model.into());
        self
    }
}

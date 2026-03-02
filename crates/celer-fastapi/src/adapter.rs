use celer_hir::{Function, Module, Statement};

use crate::error::FastApiError;
use crate::route::{HttpMethod, RouteInfo};

/// Adapter that detects FastAPI patterns in HIR modules and extracts route info.
pub struct FastApiAdapter;

impl FastApiAdapter {
    /// Scan a module for FastAPI route decorators and extract route information.
    pub fn extract_routes(module: &Module) -> Result<Vec<RouteInfo>, FastApiError> {
        let mut routes = Vec::new();
        for stmt in &module.body {
            if let Statement::FunctionDef(func) = stmt
                && let Some(route) = Self::try_extract_route(func)?
            {
                routes.push(route);
            }
        }
        Ok(routes)
    }

    fn try_extract_route(func: &Function) -> Result<Option<RouteInfo>, FastApiError> {
        for decorator in &func.decorators {
            if let Some(route) = Self::parse_decorator(decorator, func)? {
                return Ok(Some(route));
            }
        }
        Ok(None)
    }

    fn parse_decorator(
        decorator: &str,
        func: &Function,
    ) -> Result<Option<RouteInfo>, FastApiError> {
        let method = if decorator.contains(".get") {
            Some(HttpMethod::Get)
        } else if decorator.contains(".post") {
            Some(HttpMethod::Post)
        } else if decorator.contains(".put") {
            Some(HttpMethod::Put)
        } else if decorator.contains(".delete") {
            Some(HttpMethod::Delete)
        } else if decorator.contains(".patch") {
            Some(HttpMethod::Patch)
        } else {
            None
        };

        match method {
            Some(m) => {
                let path = Self::extract_path(decorator).unwrap_or_else(|| "/".to_string());
                Ok(Some(RouteInfo::new(m, path, func.clone())))
            }
            None => Ok(None),
        }
    }

    fn extract_path(decorator: &str) -> Option<String> {
        let start = decorator.find('"').or_else(|| decorator.find('\''))?;
        let quote = decorator.as_bytes()[start] as char;
        let rest = &decorator[start + 1..];
        let end = rest.find(quote)?;
        Some(rest[..end].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::{Function, Module, Statement, TypeAnnotation};

    #[test]
    fn extract_get_route() {
        let func = Function {
            name: "index".to_string(),
            params: vec![],
            return_type: TypeAnnotation::Str,
            body: vec![],
            decorators: vec!["app.get(\"/\")".to_string()],
            is_async: true,
        };
        let module = Module {
            name: "main".to_string(),
            path: "main.py".to_string(),
            body: vec![Statement::FunctionDef(func)],
        };
        let routes = FastApiAdapter::extract_routes(&module).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/");
        assert_eq!(routes[0].method, HttpMethod::Get);
    }
}

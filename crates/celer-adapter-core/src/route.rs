use celer_hir::{Function, TypeAnnotation};

/// HTTP methods supported by web framework routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpMethod {
    /// Parse an HTTP method string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "DELETE" => Some(Self::Delete),
            "PATCH" => Some(Self::Patch),
            _ => None,
        }
    }
}

/// Source of a route parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamSource {
    /// Extracted from the URL path (e.g., /items/{item_id})
    Path,
    /// Extracted from the query string
    Query,
    /// Extracted from the request body (e.g., Pydantic model)
    Body,
}

/// A single route parameter with its source and type.
#[derive(Debug, Clone)]
pub struct RouteParam {
    pub name: String,
    pub source: ParamSource,
    pub ty: TypeAnnotation,
    pub required: bool,
}

/// Extracted route information from a web framework endpoint.
#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub method: HttpMethod,
    pub path: String,
    pub handler: Function,
    pub response_model: Option<String>,
    pub params: Vec<RouteParam>,
}

impl RouteInfo {
    pub fn new(method: HttpMethod, path: impl Into<String>, handler: Function) -> Self {
        let path_str: String = path.into();
        let params = Self::extract_params(&handler, &path_str);
        Self {
            method,
            path: path_str,
            handler,
            response_model: None,
            params,
        }
    }

    pub fn with_response_model(mut self, model: impl Into<String>) -> Self {
        self.response_model = Some(model.into());
        self
    }

    fn extract_params(handler: &Function, path: &str) -> Vec<RouteParam> {
        let path_params: Vec<String> = extract_path_params(path);
        let mut params = Vec::new();

        for param in &handler.params {
            let source = if path_params.contains(&param.name) {
                ParamSource::Path
            } else if matches!(param.annotation, TypeAnnotation::Class(_)) {
                ParamSource::Body
            } else {
                ParamSource::Query
            };

            let required = param.default.is_none();

            params.push(RouteParam {
                name: param.name.clone(),
                source,
                ty: param.annotation.clone(),
                required,
            });
        }

        params
    }
}

/// Extract path parameter names from a `{param}` style path template.
pub fn extract_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            let param: String = chars.by_ref().take_while(|&c| c != '}').collect();
            if !param.is_empty() {
                params.push(param);
            }
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_path_params_basic() {
        assert_eq!(extract_path_params("/items/{item_id}"), vec!["item_id"]);
        assert_eq!(
            extract_path_params("/users/{user_id}/posts/{post_id}"),
            vec!["user_id", "post_id"]
        );
        assert!(extract_path_params("/health").is_empty());
    }

    #[test]
    fn route_param_extraction() {
        use celer_hir::Parameter;

        let handler = Function {
            name: "get_item".into(),
            params: vec![
                Parameter {
                    name: "item_id".into(),
                    annotation: TypeAnnotation::Int,
                    default: None,
                },
                Parameter {
                    name: "q".into(),
                    annotation: TypeAnnotation::Str,
                    default: Some(celer_hir::Expression::NoneLiteral),
                },
            ],
            return_type: TypeAnnotation::Dict(
                Box::new(TypeAnnotation::Str),
                Box::new(TypeAnnotation::Any),
            ),
            body: vec![],
            decorators: vec![],
            is_async: false,
        };

        let route = RouteInfo::new(HttpMethod::Get, "/items/{item_id}", handler);
        assert_eq!(route.params.len(), 2);

        assert_eq!(route.params[0].name, "item_id");
        assert_eq!(route.params[0].source, ParamSource::Path);
        assert!(route.params[0].required);

        assert_eq!(route.params[1].name, "q");
        assert_eq!(route.params[1].source, ParamSource::Query);
        assert!(!route.params[1].required);
    }

    #[test]
    fn body_param_detection() {
        use celer_hir::Parameter;

        let handler = Function {
            name: "create_item".into(),
            params: vec![Parameter {
                name: "item".into(),
                annotation: TypeAnnotation::Class("Item".into()),
                default: None,
            }],
            return_type: TypeAnnotation::Dict(
                Box::new(TypeAnnotation::Str),
                Box::new(TypeAnnotation::Any),
            ),
            body: vec![],
            decorators: vec![],
            is_async: false,
        };

        let route = RouteInfo::new(HttpMethod::Post, "/items", handler);
        assert_eq!(route.params.len(), 1);
        assert_eq!(route.params[0].source, ParamSource::Body);
    }

    #[test]
    fn http_method_from_str_loose() {
        assert_eq!(HttpMethod::from_str_loose("GET"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str_loose("get"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str_loose("Post"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::from_str_loose("unknown"), None);
    }
}

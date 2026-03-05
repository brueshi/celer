// Re-export shared types from adapter-core
pub use celer_adapter_core::{HttpMethod, ParamSource, RouteInfo, RouteParam};
pub use celer_adapter_core::route::extract_path_params;

// Flask-specific path normalization below

use celer_hir::TypeAnnotation;

/// Convert Flask-style path (`<type:param>` or `<param>`) to standard `{param}` format.
/// Also returns the extracted typed parameters for type inference.
pub fn normalize_flask_path(flask_path: &str) -> (String, Vec<FlaskPathParam>) {
    let mut normalized = String::with_capacity(flask_path.len());
    let mut params = Vec::new();
    let mut chars = flask_path.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            let segment: String = chars.by_ref().take_while(|&c| c != '>').collect();
            let (type_hint, name) = if let Some(colon_pos) = segment.find(':') {
                let t = segment[..colon_pos].trim().to_string();
                let n = segment[colon_pos + 1..].trim().to_string();
                (Some(t), n)
            } else {
                (None, segment.trim().to_string())
            };

            normalized.push('{');
            normalized.push_str(&name);
            normalized.push('}');

            params.push(FlaskPathParam {
                name,
                flask_type: type_hint,
            });
        } else {
            normalized.push(c);
        }
    }

    (normalized, params)
}

/// A path parameter extracted from Flask angle-bracket syntax.
#[derive(Debug, Clone, PartialEq)]
pub struct FlaskPathParam {
    pub name: String,
    /// Flask type converter (e.g., "int", "float", "string", "path", "uuid")
    pub flask_type: Option<String>,
}

impl FlaskPathParam {
    /// Map a Flask type converter to the HIR type annotation.
    pub fn to_type_annotation(&self) -> TypeAnnotation {
        match self.flask_type.as_deref() {
            Some("int") => TypeAnnotation::Int,
            Some("float") => TypeAnnotation::Float,
            Some("string") | Some("path") | None => TypeAnnotation::Str,
            Some("uuid") => TypeAnnotation::Str,
            Some(_) => TypeAnnotation::Any,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_simple_param() {
        let (path, params) = normalize_flask_path("/items/<item_id>");
        assert_eq!(path, "/items/{item_id}");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "item_id");
        assert_eq!(params[0].flask_type, None);
    }

    #[test]
    fn normalize_typed_param() {
        let (path, params) = normalize_flask_path("/items/<int:item_id>");
        assert_eq!(path, "/items/{item_id}");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "item_id");
        assert_eq!(params[0].flask_type, Some("int".to_string()));
        assert_eq!(params[0].to_type_annotation(), TypeAnnotation::Int);
    }

    #[test]
    fn normalize_multiple_params() {
        let (path, params) =
            normalize_flask_path("/users/<int:user_id>/posts/<int:post_id>");
        assert_eq!(path, "/users/{user_id}/posts/{post_id}");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "user_id");
        assert_eq!(params[1].name, "post_id");
    }

    #[test]
    fn normalize_no_params() {
        let (path, params) = normalize_flask_path("/health");
        assert_eq!(path, "/health");
        assert!(params.is_empty());
    }

    #[test]
    fn normalize_float_and_path_types() {
        let (_, params) = normalize_flask_path("/data/<float:ratio>/<path:filepath>");
        assert_eq!(params[0].to_type_annotation(), TypeAnnotation::Float);
        assert_eq!(params[1].to_type_annotation(), TypeAnnotation::Str);
    }

    #[test]
    fn extract_params_from_normalized_path() {
        let params = extract_path_params("/items/{item_id}");
        assert_eq!(params, vec!["item_id"]);
    }

    #[test]
    fn route_param_extraction() {
        use celer_hir::Parameter;
        use celer_hir::Function;

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
        use celer_hir::Function;

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
}

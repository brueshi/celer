use celer_adapter_core::FrameworkAdapter;
use celer_hir::{Function, Module, Statement};

use crate::error::FlaskError;
use crate::route::{normalize_flask_path, HttpMethod, RouteInfo};

/// Adapter that detects Flask patterns in HIR modules and extracts route info.
pub struct FlaskAdapter;

impl FlaskAdapter {
    /// Scan a module for Flask route decorators and extract route information.
    ///
    /// Supports both `@app.route("/path", methods=["GET"])` and
    /// Flask 2.0+ shorthand like `@app.get("/path")`.
    pub fn extract_routes_static(module: &Module) -> Result<Vec<RouteInfo>, FlaskError> {
        let mut routes = Vec::new();
        for stmt in &module.body {
            if let Statement::FunctionDef(func) = stmt {
                let mut extracted = Self::try_extract_routes(func)?;
                routes.append(&mut extracted);
            }
        }
        Ok(routes)
    }

    /// Attempt to extract route(s) from a single function's decorators.
    /// Returns multiple routes when `methods=["GET", "POST"]` is used.
    fn try_extract_routes(func: &Function) -> Result<Vec<RouteInfo>, FlaskError> {
        let mut routes = Vec::new();
        for decorator in &func.decorators {
            let mut extracted = Self::parse_decorator(decorator, func)?;
            routes.append(&mut extracted);
        }
        Ok(routes)
    }

    /// Parse a single decorator string and return route info(s).
    ///
    /// Handles three patterns:
    /// 1. `app.route("/path", methods=["GET", "POST"])` -- explicit methods
    /// 2. `app.route("/path")` -- defaults to GET
    /// 3. `app.get("/path")`, `app.post("/path")`, etc. -- Flask 2.0+ shorthand
    fn parse_decorator(
        decorator: &str,
        func: &Function,
    ) -> Result<Vec<RouteInfo>, FlaskError> {
        // Check for Flask 2.0+ shorthand: app.get, app.post, etc.
        if let Some(method) = Self::parse_shorthand_method(decorator) {
            let flask_path = Self::extract_path(decorator).unwrap_or_else(|| "/".to_string());
            let (normalized, _) = normalize_flask_path(&flask_path);
            return Ok(vec![RouteInfo::new(method, normalized, func.clone())]);
        }

        // Check for @app.route(...) pattern
        if !decorator.contains(".route") {
            return Ok(vec![]);
        }

        let flask_path = Self::extract_path(decorator).unwrap_or_else(|| "/".to_string());
        let (normalized, _) = normalize_flask_path(&flask_path);

        let methods = Self::extract_methods(decorator);
        if methods.is_empty() {
            // Default to GET when no methods specified
            return Ok(vec![RouteInfo::new(HttpMethod::Get, normalized, func.clone())]);
        }

        let routes = methods
            .into_iter()
            .map(|method| RouteInfo::new(method, normalized.clone(), func.clone()))
            .collect();
        Ok(routes)
    }

    /// Detect Flask 2.0+ shorthand decorators (app.get, app.post, etc.).
    /// Returns None if the decorator uses `.route` instead.
    fn parse_shorthand_method(decorator: &str) -> Option<HttpMethod> {
        // Avoid matching `.route` as a shorthand
        if decorator.contains(".route") {
            return None;
        }

        if decorator.contains(".get") {
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
        }
    }

    /// Extract the path string from a decorator (first quoted string).
    fn extract_path(decorator: &str) -> Option<String> {
        let start = decorator.find('"').or_else(|| decorator.find('\''))?;
        let quote = decorator.as_bytes()[start] as char;
        let rest = &decorator[start + 1..];
        let end = rest.find(quote)?;
        Some(rest[..end].to_string())
    }

    /// Extract HTTP methods from the `methods=[...]` argument in a decorator.
    fn extract_methods(decorator: &str) -> Vec<HttpMethod> {
        let methods_start = match decorator.find("methods") {
            Some(pos) => pos,
            None => return vec![],
        };

        let rest = &decorator[methods_start..];
        let bracket_start = match rest.find('[') {
            Some(pos) => pos,
            None => return vec![],
        };
        let bracket_end = match rest.find(']') {
            Some(pos) => pos,
            None => return vec![],
        };

        let inner = &rest[bracket_start + 1..bracket_end];

        inner
            .split(',')
            .filter_map(|s| {
                let trimmed = s.trim().trim_matches('"').trim_matches('\'');
                HttpMethod::from_str_loose(trimmed)
            })
            .collect()
    }
}

impl FrameworkAdapter for FlaskAdapter {
    fn name(&self) -> &'static str {
        "Flask"
    }

    fn detect(&self, module: &Module) -> bool {
        module.body.iter().any(|stmt| {
            if let Statement::FunctionDef(func) = stmt {
                func.decorators.iter().any(|d| d.contains(".route")
                    || d.contains(".get") || d.contains(".post")
                    || d.contains(".put") || d.contains(".delete")
                    || d.contains(".patch"))
            } else {
                false
            }
        })
    }

    fn extract_routes(&self, module: &Module) -> Result<Vec<celer_adapter_core::RouteInfo>, Box<dyn std::error::Error>> {
        let mut routes = Vec::new();
        for stmt in &module.body {
            if let Statement::FunctionDef(func) = stmt {
                let mut extracted = Self::try_extract_routes(func)?;
                routes.append(&mut extracted);
            }
        }
        Ok(routes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::{Function, Module, Parameter, Statement, TypeAnnotation};

    fn make_handler(name: &str, decorators: Vec<&str>) -> Function {
        Function {
            name: name.to_string(),
            params: vec![],
            return_type: TypeAnnotation::Str,
            body: vec![],
            decorators: decorators.into_iter().map(String::from).collect(),
            is_async: false,
        }
    }

    fn make_module(functions: Vec<Function>) -> Module {
        Module {
            name: "app".to_string(),
            path: "app.py".to_string(),
            body: functions.into_iter().map(Statement::FunctionDef).collect(),
        }
    }

    #[test]
    fn extract_route_default_get() {
        let func = make_handler("index", vec!["app.route(\"/\")"] );
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/");
        assert_eq!(routes[0].method, HttpMethod::Get);
        assert_eq!(routes[0].handler.name, "index");
    }

    #[test]
    fn extract_route_explicit_get() {
        let func = make_handler("index", vec!["app.route(\"/\", methods=[\"GET\"])"]);
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].method, HttpMethod::Get);
    }

    #[test]
    fn extract_route_multiple_methods() {
        let func = make_handler(
            "handle",
            vec!["app.route(\"/data\", methods=[\"GET\", \"POST\"])"],
        );
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].method, HttpMethod::Get);
        assert_eq!(routes[0].path, "/data");
        assert_eq!(routes[1].method, HttpMethod::Post);
        assert_eq!(routes[1].path, "/data");
    }

    #[test]
    fn extract_flask2_shorthand_get() {
        let func = make_handler("index", vec!["app.get(\"/\")"]);
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].method, HttpMethod::Get);
    }

    #[test]
    fn extract_flask2_shorthand_post() {
        let func = make_handler("create", vec!["app.post(\"/items\")"]);
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].method, HttpMethod::Post);
        assert_eq!(routes[0].path, "/items");
    }

    #[test]
    fn extract_flask2_shorthand_all_methods() {
        for (dec, expected) in [
            ("app.get(\"/\")", HttpMethod::Get),
            ("app.post(\"/\")", HttpMethod::Post),
            ("app.put(\"/\")", HttpMethod::Put),
            ("app.delete(\"/\")", HttpMethod::Delete),
            ("app.patch(\"/\")", HttpMethod::Patch),
        ] {
            let func = make_handler("handler", vec![dec]);
            let module = make_module(vec![func]);
            let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
            assert_eq!(routes[0].method, expected);
        }
    }

    #[test]
    fn flask_path_params_normalized() {
        let func = make_handler(
            "get_item",
            vec!["app.route(\"/items/<int:item_id>\")"],
        );
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/items/{item_id}");
    }

    #[test]
    fn flask_path_params_untyped() {
        let func = make_handler(
            "get_user",
            vec!["app.route(\"/users/<username>\")"],
        );
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes[0].path, "/users/{username}");
    }

    #[test]
    fn flask_path_with_handler_params() {
        let func = Function {
            name: "get_item".to_string(),
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
            return_type: TypeAnnotation::Str,
            body: vec![],
            decorators: vec!["app.route(\"/items/<int:item_id>\")".to_string()],
            is_async: false,
        };

        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes[0].params.len(), 2);

        assert_eq!(routes[0].params[0].name, "item_id");
        assert_eq!(routes[0].params[0].source, crate::route::ParamSource::Path);
        assert!(routes[0].params[0].required);

        assert_eq!(routes[0].params[1].name, "q");
        assert_eq!(routes[0].params[1].source, crate::route::ParamSource::Query);
        assert!(!routes[0].params[1].required);
    }

    #[test]
    fn multiple_routes_in_module() {
        let module = make_module(vec![
            make_handler("index", vec!["app.get(\"/\")"]),
            make_handler("create", vec!["app.post(\"/items\")"]),
            make_handler("health", vec!["app.route(\"/health\")"]),
        ]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes.len(), 3);
    }

    #[test]
    fn no_decorators_no_routes() {
        let func = Function {
            name: "helper".to_string(),
            params: vec![],
            return_type: TypeAnnotation::Str,
            body: vec![],
            decorators: vec![],
            is_async: false,
        };
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert!(routes.is_empty());
    }

    #[test]
    fn single_quoted_path() {
        let func = make_handler("index", vec!["app.route('/')"]);
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes[0].path, "/");
    }

    #[test]
    fn methods_with_single_quotes() {
        let func = make_handler(
            "handle",
            vec!["app.route('/data', methods=['GET', 'POST'])"],
        );
        let module = make_module(vec![func]);
        let routes = FlaskAdapter::extract_routes_static(&module).unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].method, HttpMethod::Get);
        assert_eq!(routes[1].method, HttpMethod::Post);
    }

    #[test]
    fn trait_detect_flask_route() {
        let func = make_handler("index", vec!["app.route(\"/\")"]);
        let module = make_module(vec![func]);
        let adapter = FlaskAdapter;
        assert!(adapter.detect(&module));
        let routes = adapter.extract_routes(&module).unwrap();
        assert_eq!(routes.len(), 1);
    }
}

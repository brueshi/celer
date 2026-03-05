use celer_adapter_core::{HttpMethod, RouteInfo};
use celer_hir::{Expression, Function, Module, Statement};

use crate::error::DjangoError;
use crate::patterns::normalize_django_path;

/// Extract routes from Django-style `urlpatterns = [path(...)]` declarations.
pub fn extract_routes(module: &Module) -> Result<Vec<RouteInfo>, DjangoError> {
    let urlpatterns = find_urlpatterns(module)?;
    let functions = collect_functions(module);
    let mut routes = Vec::new();

    for element in urlpatterns {
        if let Some(route) = parse_path_call(element, &functions)? {
            routes.push(route);
        }
    }

    Ok(routes)
}

/// Find the `urlpatterns` list assignment in the module.
fn find_urlpatterns(module: &Module) -> Result<&[Expression], DjangoError> {
    for stmt in &module.body {
        if let Statement::Assign { target, value, .. } = stmt
            && target == "urlpatterns"
        {
            if let Expression::List { elements, .. } = value {
                return Ok(elements);
            }
            return Err(DjangoError::InvalidUrlPattern(
                "urlpatterns must be a list".to_string(),
            ));
        }
    }
    Err(DjangoError::NoUrlPatterns)
}

/// Collect all top-level function definitions by name.
fn collect_functions(module: &Module) -> Vec<&Function> {
    module
        .body
        .iter()
        .filter_map(|stmt| {
            if let Statement::FunctionDef(func) = stmt {
                Some(func)
            } else {
                None
            }
        })
        .collect()
}

/// Parse a single `path("route/", view_func)` call expression into a RouteInfo.
fn parse_path_call(
    expr: &Expression,
    functions: &[&Function],
) -> Result<Option<RouteInfo>, DjangoError> {
    let Expression::Call { func, args, .. } = expr else {
        return Ok(None);
    };

    // Check if this is a path() call
    let is_path = match func.as_ref() {
        Expression::Name { id, .. } => id == "path",
        _ => false,
    };

    if !is_path {
        return Ok(None);
    }

    // path() requires at least 2 args: path pattern and view function
    if args.len() < 2 {
        return Err(DjangoError::InvalidUrlPattern(
            "path() requires at least 2 arguments".to_string(),
        ));
    }

    // First arg: route pattern string
    let pattern = match &args[0] {
        Expression::StringLiteral(s) => s.clone(),
        _ => {
            return Err(DjangoError::InvalidUrlPattern(
                "first argument to path() must be a string literal".to_string(),
            ));
        }
    };

    // Second arg: view function name
    let view_name = match &args[1] {
        Expression::Name { id, .. } => id.clone(),
        _ => {
            return Err(DjangoError::InvalidUrlPattern(
                "second argument to path() must be a function reference".to_string(),
            ));
        }
    };

    // Look up the function definition
    let handler = functions
        .iter()
        .find(|f| f.name == view_name)
        .ok_or_else(|| {
            DjangoError::InvalidUrlPattern(format!("view function '{view_name}' not found"))
        })?;

    // Normalize path and extract typed parameters
    let (normalized_path, _path_params) = normalize_django_path(&pattern);

    // Django routes default to GET (method dispatch happens in the view)
    Ok(Some(RouteInfo::new(
        HttpMethod::Get,
        normalized_path,
        (*handler).clone(),
    )))
}

/// Check if a module contains Django URL patterns.
pub fn detect_django(module: &Module) -> bool {
    module.body.iter().any(|stmt| {
        matches!(
            stmt,
            Statement::Assign { target, .. } if target == "urlpatterns"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::{Parameter, TypeAnnotation};

    fn make_view(name: &str, params: Vec<Parameter>) -> Function {
        Function {
            name: name.to_string(),
            params,
            return_type: TypeAnnotation::Dict(
                Box::new(TypeAnnotation::Str),
                Box::new(TypeAnnotation::Any),
            ),
            body: vec![],
            decorators: vec![],
            is_async: false,
        }
    }

    fn make_path_call(pattern: &str, view_name: &str) -> Expression {
        Expression::Call {
            func: Box::new(Expression::Name {
                id: "path".to_string(),
                ty: TypeAnnotation::Any,
            }),
            args: vec![
                Expression::StringLiteral(pattern.to_string()),
                Expression::Name {
                    id: view_name.to_string(),
                    ty: TypeAnnotation::Any,
                },
            ],
            ty: TypeAnnotation::Any,
        }
    }

    fn make_django_module(views: Vec<Function>, patterns: Vec<Expression>) -> Module {
        let mut body: Vec<Statement> = views.into_iter().map(Statement::FunctionDef).collect();
        body.push(Statement::Assign {
            target: "urlpatterns".to_string(),
            annotation: None,
            value: Expression::List {
                elements: patterns,
                ty: TypeAnnotation::Any,
            },
        });
        Module {
            name: "urls".to_string(),
            path: "urls.py".to_string(),
            body,
        }
    }

    #[test]
    fn detect_django_module() {
        let module = make_django_module(vec![], vec![]);
        assert!(detect_django(&module));
    }

    #[test]
    fn detect_non_django_module() {
        let module = Module {
            name: "app".to_string(),
            path: "app.py".to_string(),
            body: vec![],
        };
        assert!(!detect_django(&module));
    }

    #[test]
    fn extract_simple_route() {
        let view = make_view("index", vec![]);
        let module = make_django_module(
            vec![view],
            vec![make_path_call("", "index")],
        );
        let routes = extract_routes(&module).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/");
        assert_eq!(routes[0].method, HttpMethod::Get);
        assert_eq!(routes[0].handler.name, "index");
    }

    #[test]
    fn extract_route_with_int_param() {
        let view = make_view(
            "get_item",
            vec![Parameter {
                name: "item_id".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
        );
        let module = make_django_module(
            vec![view],
            vec![make_path_call("items/<int:item_id>/", "get_item")],
        );
        let routes = extract_routes(&module).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/items/{item_id}");
        assert_eq!(routes[0].params.len(), 1);
        assert_eq!(
            routes[0].params[0].source,
            celer_adapter_core::ParamSource::Path
        );
    }

    #[test]
    fn extract_multiple_routes() {
        let index = make_view("index", vec![]);
        let detail = make_view(
            "detail",
            vec![Parameter {
                name: "pk".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
        );
        let module = make_django_module(
            vec![index, detail],
            vec![
                make_path_call("", "index"),
                make_path_call("items/<int:pk>/", "detail"),
            ],
        );
        let routes = extract_routes(&module).unwrap();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].handler.name, "index");
        assert_eq!(routes[1].handler.name, "detail");
    }

    #[test]
    fn missing_view_function() {
        let module = make_django_module(
            vec![],
            vec![make_path_call("items/", "nonexistent")],
        );
        let result = extract_routes(&module);
        assert!(result.is_err());
    }

    #[test]
    fn no_urlpatterns() {
        let module = Module {
            name: "app".to_string(),
            path: "app.py".to_string(),
            body: vec![Statement::FunctionDef(make_view("index", vec![]))],
        };
        let result = extract_routes(&module);
        assert!(result.is_err());
    }
}

use std::collections::HashMap;

/// A compiled route entry linking an HTTP endpoint to a native handler.
pub struct CompiledRoute {
    pub handler_name: String,
    pub is_json: bool,
    /// Path parameter names in order of appearance.
    pub path_params: Vec<String>,
    /// Parameter types for value conversion.
    pub param_types: Vec<ParamType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamType {
    Int,
    Str,
}

/// Disposition of a matched route: native AOT or ASGI fallback.
pub enum RouteDisposition<'a> {
    /// Route is compiled to native code, bypass Python entirely.
    Native(&'a CompiledRoute),
    /// Route is not compiled, forward to Python ASGI app.
    Asgi,
}

/// Route table matching HTTP requests to compiled handlers.
pub struct Router {
    routes: Vec<RouteEntry>,
}

struct RouteEntry {
    method: String,
    segments: Vec<PathSegment>,
    route: CompiledRoute,
}

#[derive(Debug, Clone)]
enum PathSegment {
    Literal(String),
    Param(String),
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Register a compiled route with the given HTTP method and path pattern.
    ///
    /// Path patterns use `{name}` for dynamic segments (e.g. `/items/{item_id}`).
    pub fn add_route(&mut self, method: &str, path: &str, route: CompiledRoute) {
        let segments = parse_segments(path);
        self.routes.push(RouteEntry {
            method: method.to_uppercase(),
            segments,
            route,
        });
    }

    /// Match an incoming request for hybrid routing.
    ///
    /// Returns `Native` with route if a compiled handler exists,
    /// or `Asgi` if the request should be forwarded to Python.
    pub fn match_hybrid(
        &self,
        method: &str,
        path: &str,
    ) -> (RouteDisposition<'_>, HashMap<String, String>) {
        match self.match_route(method, path) {
            Some((route, params)) => (RouteDisposition::Native(route), params),
            None => (RouteDisposition::Asgi, HashMap::new()),
        }
    }

    /// Match an incoming request against registered routes.
    ///
    /// Returns the matched route and extracted path parameters on success.
    pub fn match_route(
        &self,
        method: &str,
        path: &str,
    ) -> Option<(&CompiledRoute, HashMap<String, String>)> {
        let incoming: Vec<&str> = path
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let method_upper = method.to_uppercase();

        for entry in &self.routes {
            if entry.method != method_upper {
                continue;
            }
            if let Some(params) = match_segments(&entry.segments, &incoming) {
                return Some((&entry.route, params));
            }
        }
        None
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a path pattern string into typed segments.
fn parse_segments(path: &str) -> Vec<PathSegment> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|seg| {
            if seg.starts_with('{') && seg.ends_with('}') {
                PathSegment::Param(seg[1..seg.len() - 1].to_string())
            } else {
                PathSegment::Literal(seg.to_string())
            }
        })
        .collect()
}

/// Try to match incoming path segments against a route pattern.
/// Returns extracted parameter map on success.
fn match_segments(
    pattern: &[PathSegment],
    incoming: &[&str],
) -> Option<HashMap<String, String>> {
    if pattern.len() != incoming.len() {
        return None;
    }

    let mut params = HashMap::new();
    for (seg, value) in pattern.iter().zip(incoming.iter()) {
        match seg {
            PathSegment::Literal(expected) => {
                if expected != value {
                    return None;
                }
            }
            PathSegment::Param(name) => {
                params.insert(name.clone(), (*value).to_string());
            }
        }
    }
    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(name: &str, params: &[(&str, ParamType)]) -> CompiledRoute {
        CompiledRoute {
            handler_name: name.to_string(),
            is_json: true,
            path_params: params.iter().map(|(n, _)| n.to_string()).collect(),
            param_types: params.iter().map(|(_, t)| t.clone()).collect(),
        }
    }

    #[test]
    fn literal_path_match() {
        let mut router = Router::new();
        router.add_route("GET", "/health", route("health_check", &[]));

        let (matched, params) = router.match_route("GET", "/health").unwrap();
        assert_eq!(matched.handler_name, "health_check");
        assert!(params.is_empty());
    }

    #[test]
    fn literal_path_trailing_slash() {
        let mut router = Router::new();
        router.add_route("GET", "/health", route("health_check", &[]));

        // Trailing slash should still match
        let result = router.match_route("GET", "/health/");
        assert!(result.is_some());
    }

    #[test]
    fn single_path_param() {
        let mut router = Router::new();
        router.add_route(
            "GET",
            "/items/{item_id}",
            route("get_item", &[("item_id", ParamType::Int)]),
        );

        let (matched, params) = router.match_route("GET", "/items/42").unwrap();
        assert_eq!(matched.handler_name, "get_item");
        assert_eq!(params.get("item_id").unwrap(), "42");
    }

    #[test]
    fn multi_path_params() {
        let mut router = Router::new();
        router.add_route(
            "GET",
            "/users/{user_id}/posts/{post_id}",
            route(
                "get_user_post",
                &[("user_id", ParamType::Int), ("post_id", ParamType::Int)],
            ),
        );

        let (matched, params) = router.match_route("GET", "/users/7/posts/99").unwrap();
        assert_eq!(matched.handler_name, "get_user_post");
        assert_eq!(params.get("user_id").unwrap(), "7");
        assert_eq!(params.get("post_id").unwrap(), "99");
    }

    #[test]
    fn method_discrimination() {
        let mut router = Router::new();
        router.add_route("GET", "/items", route("list_items", &[]));
        router.add_route("POST", "/items", route("create_item", &[]));

        let (get_match, _) = router.match_route("GET", "/items").unwrap();
        assert_eq!(get_match.handler_name, "list_items");

        let (post_match, _) = router.match_route("POST", "/items").unwrap();
        assert_eq!(post_match.handler_name, "create_item");
    }

    #[test]
    fn no_match_wrong_method() {
        let mut router = Router::new();
        router.add_route("GET", "/items", route("list_items", &[]));

        assert!(router.match_route("DELETE", "/items").is_none());
    }

    #[test]
    fn no_match_wrong_path() {
        let mut router = Router::new();
        router.add_route("GET", "/items", route("list_items", &[]));

        assert!(router.match_route("GET", "/users").is_none());
    }

    #[test]
    fn no_match_extra_segments() {
        let mut router = Router::new();
        router.add_route("GET", "/items", route("list_items", &[]));

        assert!(router.match_route("GET", "/items/42/extra").is_none());
    }

    #[test]
    fn case_insensitive_method() {
        let mut router = Router::new();
        router.add_route("GET", "/health", route("health_check", &[]));

        assert!(router.match_route("get", "/health").is_some());
        assert!(router.match_route("Get", "/health").is_some());
    }

    #[test]
    fn hybrid_native_match() {
        let mut router = Router::new();
        router.add_route("GET", "/health", route("health_check", &[]));

        let (disposition, _) = router.match_hybrid("GET", "/health");
        assert!(matches!(disposition, RouteDisposition::Native(r) if r.handler_name == "health_check"));
    }

    #[test]
    fn hybrid_asgi_fallback() {
        let mut router = Router::new();
        router.add_route("GET", "/health", route("health_check", &[]));

        let (disposition, _) = router.match_hybrid("POST", "/users");
        assert!(matches!(disposition, RouteDisposition::Asgi));
    }

    #[test]
    fn root_path_match() {
        let mut router = Router::new();
        router.add_route("GET", "/", route("index", &[]));

        let (matched, params) = router.match_route("GET", "/").unwrap();
        assert_eq!(matched.handler_name, "index");
        assert!(params.is_empty());
    }
}

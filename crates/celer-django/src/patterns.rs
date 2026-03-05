use celer_hir::TypeAnnotation;

/// A parsed Django path converter (e.g., `<int:id>` -> DjangoPathParam { name: "id", converter: Some("int") }).
#[derive(Debug, Clone, PartialEq)]
pub struct DjangoPathParam {
    pub name: String,
    pub converter: Option<String>,
}

impl DjangoPathParam {
    /// Map a Django path converter to the HIR type annotation.
    pub fn to_type_annotation(&self) -> TypeAnnotation {
        match self.converter.as_deref() {
            Some("int") => TypeAnnotation::Int,
            Some("str") | None => TypeAnnotation::Str,
            Some("slug") => TypeAnnotation::Str,
            Some("uuid") => TypeAnnotation::Str,
            Some("path") => TypeAnnotation::Str,
            Some(_) => TypeAnnotation::Any,
        }
    }
}

/// Normalize a Django path pattern to standard `{param}` format.
///
/// Django uses `<converter:name>` or `<name>` syntax.
/// Examples:
///   - `users/<int:id>/` -> `users/{id}`
///   - `items/<slug:slug>/` -> `items/{slug}`
///   - `files/<path:filepath>` -> `files/{filepath}`
pub fn normalize_django_path(django_path: &str) -> (String, Vec<DjangoPathParam>) {
    let mut normalized = String::with_capacity(django_path.len());
    let mut params = Vec::new();
    let mut chars = django_path.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            let segment: String = chars.by_ref().take_while(|&c| c != '>').collect();
            let (converter, name) = if let Some(colon_pos) = segment.find(':') {
                let conv = segment[..colon_pos].trim().to_string();
                let n = segment[colon_pos + 1..].trim().to_string();
                (Some(conv), n)
            } else {
                (None, segment.trim().to_string())
            };

            normalized.push('{');
            normalized.push_str(&name);
            normalized.push('}');

            params.push(DjangoPathParam { name, converter });
        } else {
            normalized.push(c);
        }
    }

    // Ensure path starts with /
    if !normalized.starts_with('/') {
        normalized.insert(0, '/');
    }

    // Remove trailing slash for consistency with other adapters
    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    (normalized, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_path() {
        let (path, params) = normalize_django_path("items/");
        assert_eq!(path, "/items");
        assert!(params.is_empty());
    }

    #[test]
    fn typed_param() {
        let (path, params) = normalize_django_path("items/<int:item_id>/");
        assert_eq!(path, "/items/{item_id}");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "item_id");
        assert_eq!(params[0].converter, Some("int".to_string()));
        assert_eq!(params[0].to_type_annotation(), TypeAnnotation::Int);
    }

    #[test]
    fn untyped_param() {
        let (path, params) = normalize_django_path("users/<username>/");
        assert_eq!(path, "/users/{username}");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "username");
        assert_eq!(params[0].converter, None);
        assert_eq!(params[0].to_type_annotation(), TypeAnnotation::Str);
    }

    #[test]
    fn multiple_params() {
        let (path, params) = normalize_django_path("users/<int:user_id>/posts/<int:post_id>/");
        assert_eq!(path, "/users/{user_id}/posts/{post_id}");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn slug_and_uuid() {
        let (_, params) = normalize_django_path("articles/<slug:slug>/<uuid:id>/");
        assert_eq!(params[0].to_type_annotation(), TypeAnnotation::Str);
        assert_eq!(params[1].to_type_annotation(), TypeAnnotation::Str);
    }

    #[test]
    fn path_converter() {
        let (_, params) = normalize_django_path("files/<path:filepath>");
        assert_eq!(params[0].to_type_annotation(), TypeAnnotation::Str);
    }

    #[test]
    fn root_path() {
        let (path, params) = normalize_django_path("");
        assert_eq!(path, "/");
        assert!(params.is_empty());
    }

    #[test]
    fn already_prefixed() {
        let (path, _) = normalize_django_path("/api/items/");
        assert_eq!(path, "/api/items");
    }
}

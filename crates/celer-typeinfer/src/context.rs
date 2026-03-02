use std::collections::HashMap;

use celer_hir::TypeAnnotation;

/// Scoped symbol table for tracking variable types during inference.
pub struct TypeContext {
    scopes: Vec<HashMap<String, TypeAnnotation>>,
}

impl TypeContext {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define(&mut self, name: String, ty: TypeAnnotation) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Look up a variable by walking scopes from innermost to outermost.
    pub fn lookup(&self, name: &str) -> Option<&TypeAnnotation> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }
}

impl Default for TypeContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_and_lookup() {
        let mut ctx = TypeContext::new();
        ctx.define("x".into(), TypeAnnotation::Int);
        assert_eq!(ctx.lookup("x"), Some(&TypeAnnotation::Int));
    }

    #[test]
    fn lookup_missing_returns_none() {
        let ctx = TypeContext::new();
        assert_eq!(ctx.lookup("missing"), None);
    }

    #[test]
    fn inner_scope_shadows_outer() {
        let mut ctx = TypeContext::new();
        ctx.define("x".into(), TypeAnnotation::Int);

        ctx.push_scope();
        ctx.define("x".into(), TypeAnnotation::Str);
        assert_eq!(ctx.lookup("x"), Some(&TypeAnnotation::Str));

        ctx.pop_scope();
        assert_eq!(ctx.lookup("x"), Some(&TypeAnnotation::Int));
    }

    #[test]
    fn inner_scope_falls_through_to_outer() {
        let mut ctx = TypeContext::new();
        ctx.define("outer_var".into(), TypeAnnotation::Float);

        ctx.push_scope();
        // inner scope does not define outer_var, lookup should find it in outer
        assert_eq!(ctx.lookup("outer_var"), Some(&TypeAnnotation::Float));

        ctx.pop_scope();
    }

    #[test]
    fn inner_scope_var_not_visible_after_pop() {
        let mut ctx = TypeContext::new();

        ctx.push_scope();
        ctx.define("temp".into(), TypeAnnotation::Bool);
        assert_eq!(ctx.lookup("temp"), Some(&TypeAnnotation::Bool));

        ctx.pop_scope();
        assert_eq!(ctx.lookup("temp"), None);
    }

    #[test]
    fn multiple_nested_scopes() {
        let mut ctx = TypeContext::new();
        ctx.define("a".into(), TypeAnnotation::Int);

        ctx.push_scope();
        ctx.define("b".into(), TypeAnnotation::Str);

        ctx.push_scope();
        ctx.define("c".into(), TypeAnnotation::Float);

        // All three visible from innermost scope
        assert_eq!(ctx.lookup("a"), Some(&TypeAnnotation::Int));
        assert_eq!(ctx.lookup("b"), Some(&TypeAnnotation::Str));
        assert_eq!(ctx.lookup("c"), Some(&TypeAnnotation::Float));

        ctx.pop_scope();
        assert_eq!(ctx.lookup("c"), None);
        assert_eq!(ctx.lookup("b"), Some(&TypeAnnotation::Str));

        ctx.pop_scope();
        assert_eq!(ctx.lookup("b"), None);
        assert_eq!(ctx.lookup("a"), Some(&TypeAnnotation::Int));
    }
}

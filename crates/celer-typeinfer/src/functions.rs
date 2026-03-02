use std::collections::HashMap;

use celer_hir::TypeAnnotation;

/// Represents a resolved function signature for type checking call sites.
pub struct FunctionSignature {
    pub name: String,
    pub params: Vec<(String, TypeAnnotation)>,
    pub return_type: TypeAnnotation,
}

/// Registry of known function signatures, populated during inference.
pub struct FunctionRegistry {
    signatures: HashMap<String, FunctionSignature>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self {
            signatures: HashMap::new(),
        }
    }

    pub fn register(&mut self, sig: FunctionSignature) {
        self.signatures.insert(sig.name.clone(), sig);
    }

    pub fn lookup(&self, name: &str) -> Option<&FunctionSignature> {
        self.signatures.get(name)
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_lookup() {
        let mut reg = FunctionRegistry::new();
        reg.register(FunctionSignature {
            name: "foo".into(),
            params: vec![("x".into(), TypeAnnotation::Int)],
            return_type: TypeAnnotation::Str,
        });
        let sig = reg.lookup("foo").unwrap();
        assert_eq!(sig.return_type, TypeAnnotation::Str);
        assert_eq!(sig.params.len(), 1);
    }

    #[test]
    fn lookup_missing_returns_none() {
        let reg = FunctionRegistry::new();
        assert!(reg.lookup("nonexistent").is_none());
    }
}

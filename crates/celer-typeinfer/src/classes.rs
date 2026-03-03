use std::collections::HashMap;

use celer_hir::ClassDef;
use celer_hir::TypeAnnotation;

/// Registry of known class definitions for type resolution.
pub struct ClassRegistry {
    classes: HashMap<String, ClassDef>,
}

impl ClassRegistry {
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
        }
    }

    pub fn register(&mut self, class: ClassDef) {
        self.classes.insert(class.name.clone(), class);
    }

    pub fn lookup(&self, name: &str) -> Option<&ClassDef> {
        self.classes.get(name)
    }

    pub fn field_type(&self, class_name: &str, field_name: &str) -> Option<&TypeAnnotation> {
        self.classes.get(class_name)?.field_type(field_name)
    }
}

impl Default for ClassRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_lookup_class() {
        let mut reg = ClassRegistry::new();
        let mut cls = ClassDef::new("User");
        cls.fields.push(("name".into(), TypeAnnotation::Str));
        cls.fields.push(("age".into(), TypeAnnotation::Int));
        reg.register(cls);

        let found = reg.lookup("User").unwrap();
        assert_eq!(found.name, "User");
        assert_eq!(found.field_type("name"), Some(&TypeAnnotation::Str));
        assert_eq!(found.field_type("age"), Some(&TypeAnnotation::Int));
        assert_eq!(found.field_type("missing"), None);
    }

    #[test]
    fn lookup_missing_returns_none() {
        let reg = ClassRegistry::new();
        assert!(reg.lookup("Missing").is_none());
    }

    #[test]
    fn field_type_shorthand() {
        let mut reg = ClassRegistry::new();
        let mut cls = ClassDef::new("Item");
        cls.fields.push(("price".into(), TypeAnnotation::Float));
        reg.register(cls);

        assert_eq!(
            reg.field_type("Item", "price"),
            Some(&TypeAnnotation::Float)
        );
        assert_eq!(reg.field_type("Item", "missing"), None);
        assert_eq!(reg.field_type("Missing", "price"), None);
    }
}

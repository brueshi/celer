use serde::{Deserialize, Serialize};

use crate::types::TypeAnnotation;

/// Metadata for a parsed class definition, used by the type system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassDef {
    pub name: String,
    pub bases: Vec<String>,
    pub fields: Vec<(String, TypeAnnotation)>,
    pub methods: Vec<String>,
}

impl ClassDef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            bases: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
        }
    }

    pub fn field_type(&self, field_name: &str) -> Option<&TypeAnnotation> {
        self.fields
            .iter()
            .find(|(name, _)| name == field_name)
            .map(|(_, ty)| ty)
    }

    pub fn has_base(&self, base_name: &str) -> bool {
        self.bases.iter().any(|b| b == base_name)
    }

    pub fn is_pydantic_model(&self) -> bool {
        self.bases
            .iter()
            .any(|b| b == "BaseModel" || b.ends_with(".BaseModel"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_type_lookup() {
        let mut cls = ClassDef::new("User");
        cls.fields.push(("name".into(), TypeAnnotation::Str));
        cls.fields.push(("age".into(), TypeAnnotation::Int));

        assert_eq!(cls.field_type("name"), Some(&TypeAnnotation::Str));
        assert_eq!(cls.field_type("age"), Some(&TypeAnnotation::Int));
        assert_eq!(cls.field_type("missing"), None);
    }

    #[test]
    fn has_base_check() {
        let mut cls = ClassDef::new("User");
        cls.bases.push("BaseModel".into());

        assert!(cls.has_base("BaseModel"));
        assert!(!cls.has_base("Other"));
    }

    #[test]
    fn pydantic_model_detection() {
        let mut cls = ClassDef::new("User");
        assert!(!cls.is_pydantic_model());

        cls.bases.push("BaseModel".into());
        assert!(cls.is_pydantic_model());

        let mut cls2 = ClassDef::new("Item");
        cls2.bases.push("pydantic.BaseModel".into());
        assert!(cls2.is_pydantic_model());
    }
}

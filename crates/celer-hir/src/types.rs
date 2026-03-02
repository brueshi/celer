use serde::{Deserialize, Serialize};

/// Represents a Python type annotation in Celer's HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeAnnotation {
    Int,
    Float,
    Str,
    Bool,
    Bytes,
    None,
    List(Box<TypeAnnotation>),
    Dict(Box<TypeAnnotation>, Box<TypeAnnotation>),
    Tuple(Vec<TypeAnnotation>),
    Set(Box<TypeAnnotation>),
    Optional(Box<TypeAnnotation>),
    Union(Vec<TypeAnnotation>),
    Class(String),
    Callable {
        params: Vec<TypeAnnotation>,
        ret: Box<TypeAnnotation>,
    },
    Any,
    /// Pre-inference placeholder. The inference engine resolves these.
    Unknown,
}

impl TypeAnnotation {
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }

    pub fn is_resolved(&self) -> bool {
        match self {
            Self::Unknown => false,
            Self::List(inner) | Self::Optional(inner) | Self::Set(inner) => inner.is_resolved(),
            Self::Dict(k, v) => k.is_resolved() && v.is_resolved(),
            Self::Tuple(elems) | Self::Union(elems) => elems.iter().all(|t| t.is_resolved()),
            Self::Callable { params, ret } => {
                params.iter().all(|t| t.is_resolved()) && ret.is_resolved()
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_types() {
        assert!(TypeAnnotation::Int.is_numeric());
        assert!(TypeAnnotation::Float.is_numeric());
        assert!(!TypeAnnotation::Str.is_numeric());
    }

    #[test]
    fn resolution_tracking() {
        assert!(!TypeAnnotation::Unknown.is_resolved());
        assert!(TypeAnnotation::Int.is_resolved());

        let list_unknown = TypeAnnotation::List(Box::new(TypeAnnotation::Unknown));
        assert!(!list_unknown.is_resolved());

        let list_int = TypeAnnotation::List(Box::new(TypeAnnotation::Int));
        assert!(list_int.is_resolved());
    }
}

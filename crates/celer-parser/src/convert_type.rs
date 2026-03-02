use rustpython_parser::ast;

use crate::error::ParseError;
use celer_hir::TypeAnnotation;

/// Convert a Python type annotation expression to a HIR TypeAnnotation.
pub fn convert_annotation(expr: &ast::Expr) -> Result<TypeAnnotation, ParseError> {
    match expr {
        ast::Expr::Name(name) => convert_simple_name(&name.id),
        ast::Expr::Subscript(sub) => convert_subscript(sub),
        ast::Expr::Constant(c) => match &c.value {
            ast::Constant::None => Ok(TypeAnnotation::None),
            _ => Err(ParseError::UnsupportedFeature(
                "non-None constant in type annotation".into(),
            )),
        },
        ast::Expr::Attribute(attr) => {
            // e.g. typing.Optional -- treat as Class for now
            Ok(TypeAnnotation::Class(format!(
                "{}.{}",
                expr_to_dotted(&attr.value),
                attr.attr
            )))
        }
        _ => Err(ParseError::UnsupportedFeature(format!(
            "type annotation: {expr:?}"
        ))),
    }
}

fn convert_simple_name(name: &str) -> Result<TypeAnnotation, ParseError> {
    match name {
        "int" => Ok(TypeAnnotation::Int),
        "float" => Ok(TypeAnnotation::Float),
        "str" => Ok(TypeAnnotation::Str),
        "bool" => Ok(TypeAnnotation::Bool),
        "bytes" => Ok(TypeAnnotation::Bytes),
        "None" => Ok(TypeAnnotation::None),
        "dict" => Ok(TypeAnnotation::Dict(
            Box::new(TypeAnnotation::Any),
            Box::new(TypeAnnotation::Any),
        )),
        "list" => Ok(TypeAnnotation::List(Box::new(TypeAnnotation::Any))),
        "tuple" => Ok(TypeAnnotation::Tuple(vec![TypeAnnotation::Any])),
        "set" => Ok(TypeAnnotation::Set(Box::new(TypeAnnotation::Any))),
        "Any" => Ok(TypeAnnotation::Any),
        other => Ok(TypeAnnotation::Class(other.to_string())),
    }
}

fn convert_subscript(sub: &ast::ExprSubscript) -> Result<TypeAnnotation, ParseError> {
    let base_name = match sub.value.as_ref() {
        ast::Expr::Name(n) => n.id.to_string(),
        ast::Expr::Attribute(attr) => format!("{}.{}", expr_to_dotted(&attr.value), attr.attr),
        _ => {
            return Err(ParseError::UnsupportedFeature(
                "complex subscript base in type annotation".into(),
            ));
        }
    };

    match base_name.as_str() {
        "list" | "List" => {
            let inner = convert_annotation(&sub.slice)?;
            Ok(TypeAnnotation::List(Box::new(inner)))
        }
        "dict" | "Dict" => convert_dict_subscript(&sub.slice),
        "tuple" | "Tuple" => convert_tuple_subscript(&sub.slice),
        "set" | "Set" => {
            let inner = convert_annotation(&sub.slice)?;
            Ok(TypeAnnotation::Set(Box::new(inner)))
        }
        "Optional" | "typing.Optional" => {
            let inner = convert_annotation(&sub.slice)?;
            Ok(TypeAnnotation::Optional(Box::new(inner)))
        }
        "Union" | "typing.Union" => convert_union_subscript(&sub.slice),
        _ => Ok(TypeAnnotation::Class(base_name)),
    }
}

fn convert_dict_subscript(slice: &ast::Expr) -> Result<TypeAnnotation, ParseError> {
    match slice {
        ast::Expr::Tuple(t) if t.elts.len() == 2 => {
            let key = convert_annotation(&t.elts[0])?;
            let val = convert_annotation(&t.elts[1])?;
            Ok(TypeAnnotation::Dict(Box::new(key), Box::new(val)))
        }
        _ => Ok(TypeAnnotation::Dict(
            Box::new(TypeAnnotation::Any),
            Box::new(TypeAnnotation::Any),
        )),
    }
}

fn convert_tuple_subscript(slice: &ast::Expr) -> Result<TypeAnnotation, ParseError> {
    match slice {
        ast::Expr::Tuple(t) => {
            let elems: Result<Vec<_>, _> = t.elts.iter().map(convert_annotation).collect();
            Ok(TypeAnnotation::Tuple(elems?))
        }
        other => {
            let inner = convert_annotation(other)?;
            Ok(TypeAnnotation::Tuple(vec![inner]))
        }
    }
}

fn convert_union_subscript(slice: &ast::Expr) -> Result<TypeAnnotation, ParseError> {
    match slice {
        ast::Expr::Tuple(t) => {
            let variants: Result<Vec<_>, _> = t.elts.iter().map(convert_annotation).collect();
            Ok(TypeAnnotation::Union(variants?))
        }
        other => {
            let inner = convert_annotation(other)?;
            Ok(TypeAnnotation::Union(vec![inner]))
        }
    }
}

fn expr_to_dotted(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::Name(n) => n.id.to_string(),
        ast::Expr::Attribute(a) => format!("{}.{}", expr_to_dotted(&a.value), a.attr),
        _ => "<unknown>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_types() {
        assert_eq!(convert_simple_name("int").unwrap(), TypeAnnotation::Int);
        assert_eq!(convert_simple_name("str").unwrap(), TypeAnnotation::Str);
        assert_eq!(convert_simple_name("float").unwrap(), TypeAnnotation::Float);
        assert_eq!(convert_simple_name("bool").unwrap(), TypeAnnotation::Bool);
    }

    #[test]
    fn bare_dict_resolves_to_any_any() {
        assert_eq!(
            convert_simple_name("dict").unwrap(),
            TypeAnnotation::Dict(Box::new(TypeAnnotation::Any), Box::new(TypeAnnotation::Any))
        );
    }

    #[test]
    fn unknown_class_name() {
        assert_eq!(
            convert_simple_name("MyModel").unwrap(),
            TypeAnnotation::Class("MyModel".into())
        );
    }
}

use celer_hir::Expression;

use crate::error::CodegenError;

/// Result of analyzing a dict expression for JSON emission.
pub enum JsonPlan {
    /// All keys and values are string/int/float/bool literals -- emit as a
    /// global constant string with zero runtime cost.
    Static(String),
    /// At least one value references a runtime variable. Store a `snprintf`
    /// format string and the list of dynamic argument references.
    Dynamic { format: String, args: Vec<DynArg> },
}

/// A dynamic argument that must be passed to snprintf at runtime.
pub struct DynArg {
    pub name: String,
    pub fmt: ArgFormat,
}

/// printf-family format specifier for a dynamic argument.
pub enum ArgFormat {
    I64,
    F64,
    Str,
    Bool,
}

/// Analyze a Dict expression and produce a JsonPlan.
pub fn plan_dict(keys: &[Expression], values: &[Expression]) -> Result<JsonPlan, CodegenError> {
    let mut is_static = true;
    let mut dyn_args: Vec<DynArg> = Vec::new();

    // Pre-scan: determine whether any value is dynamic
    for val in values.iter() {
        if !is_literal(val) {
            is_static = false;
            break;
        }
    }

    if is_static {
        let json = build_static_json(keys, values)?;
        return Ok(JsonPlan::Static(json));
    }

    // Dynamic path: build format string and collect argument descriptors
    let mut fmt_parts: Vec<String> = Vec::new();
    fmt_parts.push("{".to_string());

    for (i, (key, val)) in keys.iter().zip(values.iter()).enumerate() {
        if i > 0 {
            fmt_parts.push(", ".to_string());
        }
        let key_str = extract_string_literal(key)?;
        fmt_parts.push(format!("\"{}\"", escape_json(&key_str)));
        fmt_parts.push(": ".to_string());

        match val {
            Expression::IntLiteral(n) => {
                fmt_parts.push(n.to_string());
            }
            Expression::FloatLiteral(f) => {
                fmt_parts.push(format!("{f}"));
            }
            Expression::StringLiteral(s) => {
                fmt_parts.push(format!("\"{}\"", escape_json(s)));
            }
            Expression::BoolLiteral(b) => {
                fmt_parts.push(if *b { "true" } else { "false" }.to_string());
            }
            Expression::NoneLiteral => {
                fmt_parts.push("null".to_string());
            }
            Expression::Name { id, ty, .. } => {
                let arg_fmt = match ty {
                    celer_hir::TypeAnnotation::Int => ArgFormat::I64,
                    celer_hir::TypeAnnotation::Float => ArgFormat::F64,
                    celer_hir::TypeAnnotation::Str => ArgFormat::Str,
                    celer_hir::TypeAnnotation::Bool => ArgFormat::Bool,
                    _ => {
                        return Err(CodegenError::UnsupportedType(format!(
                            "dynamic dict value type: {ty:?}"
                        )));
                    }
                };
                let specifier = match &arg_fmt {
                    ArgFormat::I64 => "%lld",
                    ArgFormat::F64 => "%g",
                    ArgFormat::Str => "\"%s\"",
                    ArgFormat::Bool => "%s",
                };
                fmt_parts.push(specifier.to_string());
                dyn_args.push(DynArg {
                    name: id.clone(),
                    fmt: arg_fmt,
                });
            }
            _ => {
                return Err(CodegenError::UnsupportedExpression(format!(
                    "unsupported dict value expression: {val:?}"
                )));
            }
        }
    }

    fmt_parts.push("}".to_string());
    let format = fmt_parts.concat();
    Ok(JsonPlan::Dynamic {
        format,
        args: dyn_args,
    })
}

fn build_static_json(keys: &[Expression], values: &[Expression]) -> Result<String, CodegenError> {
    let mut parts: Vec<String> = Vec::new();
    parts.push("{".to_string());

    for (i, (key, val)) in keys.iter().zip(values.iter()).enumerate() {
        if i > 0 {
            parts.push(", ".to_string());
        }
        let key_str = extract_string_literal(key)?;
        parts.push(format!("\"{}\"", escape_json(&key_str)));
        parts.push(": ".to_string());

        match val {
            Expression::IntLiteral(n) => parts.push(n.to_string()),
            Expression::FloatLiteral(f) => parts.push(format!("{f}")),
            Expression::StringLiteral(s) => {
                parts.push(format!("\"{}\"", escape_json(s)));
            }
            Expression::BoolLiteral(b) => {
                parts.push(if *b { "true" } else { "false" }.to_string());
            }
            Expression::NoneLiteral => parts.push("null".to_string()),
            _ => {
                return Err(CodegenError::UnsupportedExpression(format!(
                    "expected literal in static dict: {val:?}"
                )));
            }
        }
    }

    parts.push("}".to_string());
    Ok(parts.concat())
}

fn is_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::IntLiteral(_)
            | Expression::FloatLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BoolLiteral(_)
            | Expression::NoneLiteral
    )
}

fn extract_string_literal(expr: &Expression) -> Result<String, CodegenError> {
    match expr {
        Expression::StringLiteral(s) => Ok(s.clone()),
        _ => Err(CodegenError::UnsupportedExpression(
            "dict key must be a string literal".to_string(),
        )),
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::TypeAnnotation;

    #[test]
    fn static_dict_plan() {
        let keys = vec![Expression::StringLiteral("message".into())];
        let values = vec![Expression::StringLiteral("hello".into())];
        let plan = plan_dict(&keys, &values).unwrap();
        match plan {
            JsonPlan::Static(s) => {
                assert_eq!(s, r#"{"message": "hello"}"#);
            }
            _ => panic!("expected static plan"),
        }
    }

    #[test]
    fn dynamic_dict_plan() {
        let keys = vec![
            Expression::StringLiteral("item_id".into()),
            Expression::StringLiteral("name".into()),
        ];
        let values = vec![
            Expression::Name {
                id: "item_id".into(),
                ty: TypeAnnotation::Int,
            },
            Expression::StringLiteral("test".into()),
        ];
        let plan = plan_dict(&keys, &values).unwrap();
        match plan {
            JsonPlan::Dynamic { format, args } => {
                assert!(format.contains("%lld"));
                assert_eq!(args.len(), 1);
                assert_eq!(args[0].name, "item_id");
            }
            _ => panic!("expected dynamic plan"),
        }
    }
}

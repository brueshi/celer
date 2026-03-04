use std::collections::HashMap;

use celer_runtime::Value;

use crate::router::ParamType;

/// Convert extracted path parameters into typed native function arguments.
///
/// Parameters are returned in the same order as `param_names`.
pub fn convert_params(
    params: &HashMap<String, String>,
    param_names: &[String],
    param_types: &[ParamType],
) -> Result<Vec<Value>, String> {
    let mut args = Vec::with_capacity(param_names.len());

    for (name, ty) in param_names.iter().zip(param_types.iter()) {
        let raw = params
            .get(name)
            .ok_or_else(|| format!("missing path parameter: {name}"))?;

        let value = match ty {
            ParamType::Int => {
                let n: i64 = raw
                    .parse()
                    .map_err(|_| format!("invalid integer for parameter '{name}': {raw}"))?;
                Value::I64(n)
            }
            ParamType::Str => Value::Str(raw.clone()),
        };
        args.push(value);
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_int_param() {
        let mut params = HashMap::new();
        params.insert("item_id".to_string(), "42".to_string());

        let result = convert_params(
            &params,
            &["item_id".to_string()],
            &[ParamType::Int],
        )
        .unwrap();

        assert_eq!(result, vec![Value::I64(42)]);
    }

    #[test]
    fn convert_str_param() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "alice".to_string());

        let result = convert_params(
            &params,
            &["name".to_string()],
            &[ParamType::Str],
        )
        .unwrap();

        assert_eq!(result, vec![Value::Str("alice".to_string())]);
    }

    #[test]
    fn convert_multiple_params() {
        let mut params = HashMap::new();
        params.insert("user_id".to_string(), "7".to_string());
        params.insert("slug".to_string(), "hello-world".to_string());

        let result = convert_params(
            &params,
            &["user_id".to_string(), "slug".to_string()],
            &[ParamType::Int, ParamType::Str],
        )
        .unwrap();

        assert_eq!(
            result,
            vec![Value::I64(7), Value::Str("hello-world".to_string())]
        );
    }

    #[test]
    fn missing_param_errors() {
        let params = HashMap::new();

        let err = convert_params(
            &params,
            &["item_id".to_string()],
            &[ParamType::Int],
        )
        .unwrap_err();

        assert!(err.contains("missing path parameter"));
    }

    #[test]
    fn invalid_int_errors() {
        let mut params = HashMap::new();
        params.insert("item_id".to_string(), "not_a_number".to_string());

        let err = convert_params(
            &params,
            &["item_id".to_string()],
            &[ParamType::Int],
        )
        .unwrap_err();

        assert!(err.contains("invalid integer"));
    }
}

/// A benchmark workload definition.
#[derive(Debug, Clone)]
pub struct Workload {
    pub name: String,
    pub python_source: String,
    pub function_name: String,
    /// Argument to pass (None for no-arg functions, Some(i64) for int-arg functions)
    pub arg: Option<i64>,
    pub expected_output_contains: String,
}

impl Workload {
    pub fn builtin_workloads() -> Vec<Workload> {
        vec![
            Workload {
                name: "json-serialize-static".into(),
                python_source: STATIC_JSON_SOURCE.into(),
                function_name: "root".into(),
                arg: None,
                expected_output_contains: r#""message""#.into(),
            },
            Workload {
                name: "json-serialize-dynamic".into(),
                python_source: DYNAMIC_JSON_SOURCE.into(),
                function_name: "get_item".into(),
                arg: Some(42),
                expected_output_contains: r#""item_id""#.into(),
            },
        ]
    }
}

const STATIC_JSON_SOURCE: &str = r#"
def root() -> dict:
    return {"message": "hello"}
"#;

const DYNAMIC_JSON_SOURCE: &str = r#"
def get_item(item_id: int) -> dict:
    return {"item_id": item_id, "name": "test"}
"#;

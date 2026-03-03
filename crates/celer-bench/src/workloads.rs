/// Whether the compiled function returns JSON via output params or a scalar directly.
#[derive(Debug, Clone, PartialEq)]
pub enum ReturnKind {
    /// Function returns JSON string via (ptr, len) output params.
    Json,
    /// Function returns a scalar i64 directly.
    ScalarI64,
}

/// A benchmark workload definition.
#[derive(Debug, Clone)]
pub struct Workload {
    pub name: String,
    pub python_source: String,
    pub function_name: String,
    /// Argument to pass (None for no-arg functions, Some(i64) for int-arg functions)
    pub arg: Option<i64>,
    pub expected_output_contains: String,
    pub return_kind: ReturnKind,
}

/// Workload category for filtering.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkloadCategory {
    Json,
    Compute,
    BusinessLogic,
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
                return_kind: ReturnKind::Json,
            },
            Workload {
                name: "json-serialize-dynamic".into(),
                python_source: DYNAMIC_JSON_SOURCE.into(),
                function_name: "get_item".into(),
                arg: Some(42),
                expected_output_contains: r#""item_id""#.into(),
                return_kind: ReturnKind::Json,
            },
        ]
    }

    pub fn compute_workloads() -> Vec<Workload> {
        vec![Workload {
            name: "fibonacci".into(),
            python_source: FIBONACCI_SOURCE.into(),
            function_name: "fib".into(),
            arg: Some(30),
            expected_output_contains: "832040".into(),
            return_kind: ReturnKind::ScalarI64,
        }]
    }

    pub fn business_logic_workloads() -> Vec<Workload> {
        vec![Workload {
            name: "business-logic".into(),
            python_source: BUSINESS_LOGIC_SOURCE.into(),
            function_name: "calculate_price".into(),
            arg: Some(100),
            expected_output_contains: "price".into(),
            return_kind: ReturnKind::Json,
        }]
    }

    pub fn all_workloads() -> Vec<Workload> {
        let mut all = Self::builtin_workloads();
        all.extend(Self::compute_workloads());
        all.extend(Self::business_logic_workloads());
        all
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

const FIBONACCI_SOURCE: &str = r#"
def fib(n: int) -> int:
    a = 0
    b = 1
    i = 0
    while i < n:
        t = a + b
        a = b
        b = t
        i = i + 1
    return a
"#;

const BUSINESS_LOGIC_SOURCE: &str = r#"
def apply_discount(price: int, threshold: int) -> int:
    if price > threshold:
        return price * 90 // 100
    return price

def calculate_price(base_price: int) -> dict:
    final_price = apply_discount(base_price, 50)
    return {"price": final_price, "currency": "USD"}
"#;

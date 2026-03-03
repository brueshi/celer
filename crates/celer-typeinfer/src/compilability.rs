use std::collections::HashMap;

use celer_hir::{BinaryOp, Expression, Function, Module, Statement, TypeAnnotation, UnaryOp};

use crate::context::TypeContext;

/// Compilability status for a function.
#[derive(Debug, Clone, PartialEq)]
pub enum Compilability {
    /// Fully compilable to native code.
    Full,
    /// Partially compilable; some features require fallback.
    Partial(Vec<String>),
    /// Not compilable at all.
    NotCompilable(String),
}

/// Per-function compilability result.
#[derive(Debug, Clone)]
pub struct FunctionCompilability {
    pub name: String,
    pub status: Compilability,
}

/// Module-level compilability report.
#[derive(Debug, Clone)]
pub struct CompilabilityReport {
    pub functions: Vec<FunctionCompilability>,
}

impl CompilabilityReport {
    pub fn compilable_functions(&self) -> Vec<&str> {
        self.functions
            .iter()
            .filter(|f| matches!(f.status, Compilability::Full))
            .map(|f| f.name.as_str())
            .collect()
    }

    pub fn skipped_functions(&self) -> Vec<(&str, &str)> {
        self.functions
            .iter()
            .filter_map(|f| match &f.status {
                Compilability::NotCompilable(reason) => Some((f.name.as_str(), reason.as_str())),
                _ => None,
            })
            .collect()
    }
}

pub struct CompilabilityAnalyzer<'a> {
    ctx: &'a TypeContext,
}

impl<'a> CompilabilityAnalyzer<'a> {
    pub fn new(ctx: &'a TypeContext) -> Self {
        Self { ctx }
    }

    /// Analyze an entire module and produce a compilability report.
    pub fn analyze_module(&self, module: &Module) -> CompilabilityReport {
        let mut functions = Vec::new();

        // First pass: collect all function names for call resolution
        let mut known_functions: HashMap<String, bool> = HashMap::new();
        for stmt in &module.body {
            if let Statement::FunctionDef(func) = stmt {
                known_functions.insert(func.name.clone(), true);
            }
        }

        // Second pass: analyze each function
        for stmt in &module.body {
            if let Statement::FunctionDef(func) = stmt {
                let status = self.analyze_function(func, &known_functions);
                functions.push(FunctionCompilability {
                    name: func.name.clone(),
                    status,
                });
            }
        }

        CompilabilityReport { functions }
    }

    pub fn analyze_function(
        &self,
        func: &Function,
        known_functions: &HashMap<String, bool>,
    ) -> Compilability {
        let mut issues = Vec::new();

        // Check return type is compilable
        if !self.is_compilable_type(&func.return_type) {
            return Compilability::NotCompilable(format!(
                "unsupported return type: {:?}",
                func.return_type
            ));
        }

        // Check all parameters have compilable types
        for param in &func.params {
            if !self.is_compilable_type(&param.annotation) {
                return Compilability::NotCompilable(format!(
                    "unsupported parameter type for '{}': {:?}",
                    param.name, param.annotation
                ));
            }
        }

        // Check body statements
        for stmt in &func.body {
            self.check_statement(stmt, known_functions, &mut issues);
        }

        if issues.is_empty() {
            Compilability::Full
        } else {
            Compilability::Partial(issues)
        }
    }

    fn is_compilable_type(&self, ty: &TypeAnnotation) -> bool {
        matches!(
            ty,
            TypeAnnotation::Int
                | TypeAnnotation::Float
                | TypeAnnotation::Bool
                | TypeAnnotation::Str
                | TypeAnnotation::None
                | TypeAnnotation::Dict(_, _)
                | TypeAnnotation::Unknown
        )
    }

    fn check_statement(
        &self,
        stmt: &Statement,
        known_functions: &HashMap<String, bool>,
        issues: &mut Vec<String>,
    ) {
        match stmt {
            Statement::Return { value } => {
                if let Some(expr) = value {
                    self.check_expression(expr, known_functions, issues);
                }
            }
            Statement::Assign { value, .. } => {
                self.check_expression(value, known_functions, issues);
            }
            Statement::AugAssign { value, .. } => {
                self.check_expression(value, known_functions, issues);
            }
            Statement::If { test, body, orelse } => {
                self.check_expression(test, known_functions, issues);
                for s in body {
                    self.check_statement(s, known_functions, issues);
                }
                for s in orelse {
                    self.check_statement(s, known_functions, issues);
                }
            }
            Statement::While { test, body } => {
                self.check_expression(test, known_functions, issues);
                for s in body {
                    self.check_statement(s, known_functions, issues);
                }
            }
            Statement::For { iter, body, .. } => {
                // Only range() loops are compilable
                if !self.is_range_call(iter) {
                    issues.push("for loop with non-range() iterator".into());
                }
                for s in body {
                    self.check_statement(s, known_functions, issues);
                }
            }
            Statement::Expr(expr) => {
                self.check_expression(expr, known_functions, issues);
            }
            Statement::Pass | Statement::Break | Statement::Continue => {}
            Statement::Raise { .. } => {
                issues.push("raise statement not natively compilable".into());
            }
            Statement::Assert { .. } => {
                issues.push("assert statement not natively compilable".into());
            }
            Statement::FunctionDef(_) => {
                issues.push("nested function definitions not supported".into());
            }
            Statement::ClassDef { .. } => {
                issues.push("class definitions not natively compilable".into());
            }
            Statement::Import { .. } | Statement::ImportFrom { .. } => {
                issues.push("import statements not natively compilable".into());
            }
        }
    }

    fn check_expression(
        &self,
        expr: &Expression,
        known_functions: &HashMap<String, bool>,
        issues: &mut Vec<String>,
    ) {
        match expr {
            Expression::IntLiteral(_)
            | Expression::FloatLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BoolLiteral(_)
            | Expression::NoneLiteral => {}
            Expression::Name { ty, .. } => {
                if matches!(ty, TypeAnnotation::Class(_)) {
                    issues.push("class instance variables not natively compilable".into());
                }
            }
            Expression::BinaryOp {
                left, right, op, ..
            } => {
                self.check_binary_op_compilable(op, issues);
                self.check_expression(left, known_functions, issues);
                self.check_expression(right, known_functions, issues);
            }
            Expression::UnaryOp { operand, op, .. } => {
                self.check_unary_op_compilable(op, issues);
                self.check_expression(operand, known_functions, issues);
            }
            Expression::Call { func, args, .. } => {
                // Check if it's a known compilable function
                if let Expression::Name { id, .. } = func.as_ref() {
                    if id != "range"
                        && !known_functions.contains_key(id.as_str())
                        && self.ctx.lookup_function(id).is_none()
                    {
                        issues.push(format!("call to unknown function: {id}"));
                    }
                } else {
                    issues.push("indirect function calls not supported".into());
                }
                for arg in args {
                    self.check_expression(arg, known_functions, issues);
                }
            }
            Expression::Dict { keys, values, .. } => {
                for k in keys {
                    self.check_expression(k, known_functions, issues);
                }
                for v in values {
                    self.check_expression(v, known_functions, issues);
                }
            }
            Expression::Attribute { .. } => {
                issues.push("attribute access not natively compilable".into());
            }
            Expression::Subscript { .. } => {
                issues.push("subscript access not natively compilable".into());
            }
            Expression::List { .. } => {
                issues.push("list expressions not natively compilable".into());
            }
            Expression::Tuple { .. } => {
                issues.push("tuple expressions not natively compilable".into());
            }
            Expression::IfExpr {
                test,
                body,
                orelse,
                ..
            } => {
                self.check_expression(test, known_functions, issues);
                self.check_expression(body, known_functions, issues);
                self.check_expression(orelse, known_functions, issues);
            }
            Expression::Lambda { .. } => {
                issues.push("lambda expressions not natively compilable".into());
            }
        }
    }

    fn check_binary_op_compilable(&self, _op: &BinaryOp, _issues: &mut Vec<String>) {
        // All binary ops are compilable for numeric types
    }

    fn check_unary_op_compilable(&self, _op: &UnaryOp, _issues: &mut Vec<String>) {
        // All unary ops are compilable for numeric types
    }

    fn is_range_call(&self, expr: &Expression) -> bool {
        matches!(
            expr,
            Expression::Call { func, args, .. }
            if matches!(func.as_ref(), Expression::Name { id, .. } if id == "range")
                && (args.len() == 1 || args.len() == 2 || args.len() == 3)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::{Function, Module, Parameter};

    fn make_ctx() -> TypeContext {
        TypeContext::new()
    }

    #[test]
    fn simple_arithmetic_is_fully_compilable() {
        let func = Function {
            name: "add".into(),
            params: vec![
                Parameter {
                    name: "a".into(),
                    annotation: TypeAnnotation::Int,
                    default: None,
                },
                Parameter {
                    name: "b".into(),
                    annotation: TypeAnnotation::Int,
                    default: None,
                },
            ],
            return_type: TypeAnnotation::Int,
            body: vec![Statement::Return {
                value: Some(Expression::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expression::Name {
                        id: "a".into(),
                        ty: TypeAnnotation::Int,
                    }),
                    right: Box::new(Expression::Name {
                        id: "b".into(),
                        ty: TypeAnnotation::Int,
                    }),
                    ty: TypeAnnotation::Int,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let ctx = make_ctx();
        let analyzer = CompilabilityAnalyzer::new(&ctx);
        let known = HashMap::from([("add".into(), true)]);
        let status = analyzer.analyze_function(&func, &known);
        assert_eq!(status, Compilability::Full);
    }

    #[test]
    fn function_with_unknown_call_is_partial() {
        let func = Function {
            name: "process".into(),
            params: vec![],
            return_type: TypeAnnotation::Int,
            body: vec![Statement::Return {
                value: Some(Expression::Call {
                    func: Box::new(Expression::Name {
                        id: "external_fn".into(),
                        ty: TypeAnnotation::Unknown,
                    }),
                    args: vec![],
                    ty: TypeAnnotation::Unknown,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let ctx = make_ctx();
        let analyzer = CompilabilityAnalyzer::new(&ctx);
        let known = HashMap::from([("process".into(), true)]);
        let status = analyzer.analyze_function(&func, &known);
        assert!(matches!(status, Compilability::Partial(_)));
    }

    #[test]
    fn module_analysis_categorizes_functions() {
        let mut module = Module::new("test", "test.py");

        // Compilable function
        module.body.push(Statement::FunctionDef(Function {
            name: "add".into(),
            params: vec![
                Parameter {
                    name: "a".into(),
                    annotation: TypeAnnotation::Int,
                    default: None,
                },
                Parameter {
                    name: "b".into(),
                    annotation: TypeAnnotation::Int,
                    default: None,
                },
            ],
            return_type: TypeAnnotation::Int,
            body: vec![Statement::Return {
                value: Some(Expression::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expression::Name {
                        id: "a".into(),
                        ty: TypeAnnotation::Int,
                    }),
                    right: Box::new(Expression::Name {
                        id: "b".into(),
                        ty: TypeAnnotation::Int,
                    }),
                    ty: TypeAnnotation::Int,
                }),
            }],
            decorators: vec![],
            is_async: false,
        }));

        // Non-compilable function (uses list)
        module.body.push(Statement::FunctionDef(Function {
            name: "make_list".into(),
            params: vec![],
            return_type: TypeAnnotation::List(Box::new(TypeAnnotation::Int)),
            body: vec![Statement::Return {
                value: Some(Expression::List {
                    elements: vec![Expression::IntLiteral(1)],
                    ty: TypeAnnotation::List(Box::new(TypeAnnotation::Int)),
                }),
            }],
            decorators: vec![],
            is_async: false,
        }));

        let ctx = make_ctx();
        let analyzer = CompilabilityAnalyzer::new(&ctx);
        let report = analyzer.analyze_module(&module);

        assert_eq!(report.functions.len(), 2);
        assert_eq!(report.compilable_functions(), vec!["add"]);
        assert_eq!(report.skipped_functions().len(), 1);
    }

    #[test]
    fn while_loop_is_compilable() {
        let func = Function {
            name: "fib".into(),
            params: vec![Parameter {
                name: "n".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Int,
            body: vec![
                Statement::Assign {
                    target: "a".into(),
                    annotation: None,
                    value: Expression::IntLiteral(0),
                },
                Statement::While {
                    test: Expression::BinaryOp {
                        op: BinaryOp::Lt,
                        left: Box::new(Expression::Name {
                            id: "a".into(),
                            ty: TypeAnnotation::Int,
                        }),
                        right: Box::new(Expression::Name {
                            id: "n".into(),
                            ty: TypeAnnotation::Int,
                        }),
                        ty: TypeAnnotation::Bool,
                    },
                    body: vec![Statement::AugAssign {
                        target: "a".into(),
                        op: BinaryOp::Add,
                        value: Expression::IntLiteral(1),
                    }],
                },
                Statement::Return {
                    value: Some(Expression::Name {
                        id: "a".into(),
                        ty: TypeAnnotation::Int,
                    }),
                },
            ],
            decorators: vec![],
            is_async: false,
        };

        let ctx = make_ctx();
        let analyzer = CompilabilityAnalyzer::new(&ctx);
        let known = HashMap::from([("fib".into(), true)]);
        let status = analyzer.analyze_function(&func, &known);
        assert_eq!(status, Compilability::Full);
    }

    #[test]
    fn inter_function_calls_are_compilable() {
        let mut module = Module::new("test", "test.py");

        module.body.push(Statement::FunctionDef(Function {
            name: "helper".into(),
            params: vec![Parameter {
                name: "x".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Int,
            body: vec![Statement::Return {
                value: Some(Expression::Name {
                    id: "x".into(),
                    ty: TypeAnnotation::Int,
                }),
            }],
            decorators: vec![],
            is_async: false,
        }));

        module.body.push(Statement::FunctionDef(Function {
            name: "caller".into(),
            params: vec![],
            return_type: TypeAnnotation::Int,
            body: vec![Statement::Return {
                value: Some(Expression::Call {
                    func: Box::new(Expression::Name {
                        id: "helper".into(),
                        ty: TypeAnnotation::Unknown,
                    }),
                    args: vec![Expression::IntLiteral(42)],
                    ty: TypeAnnotation::Int,
                }),
            }],
            decorators: vec![],
            is_async: false,
        }));

        let ctx = make_ctx();
        let analyzer = CompilabilityAnalyzer::new(&ctx);
        let report = analyzer.analyze_module(&module);

        assert_eq!(report.compilable_functions().len(), 2);
    }
}

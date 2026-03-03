use celer_hir::{BinaryOp, Expression, Module, Statement, TypeAnnotation, UnaryOp};

use crate::context::TypeContext;
use crate::error::TypeError;
use crate::functions::FunctionSignature;

fn resolve_binary_op_type(
    op: &BinaryOp,
    left: &TypeAnnotation,
    right: &TypeAnnotation,
) -> Result<TypeAnnotation, TypeError> {
    use BinaryOp::*;
    match op {
        Add | Sub | Mul | Mod | FloorDiv | Pow => match (left, right) {
            (TypeAnnotation::Int, TypeAnnotation::Int) => Ok(TypeAnnotation::Int),
            (TypeAnnotation::Float, TypeAnnotation::Float)
            | (TypeAnnotation::Int, TypeAnnotation::Float)
            | (TypeAnnotation::Float, TypeAnnotation::Int) => Ok(TypeAnnotation::Float),
            (TypeAnnotation::Str, TypeAnnotation::Str) if matches!(op, Add) => {
                Ok(TypeAnnotation::Str)
            }
            _ => Err(TypeError::BinaryOpMismatch {
                op: format!("{op:?}"),
                left: format!("{left:?}"),
                right: format!("{right:?}"),
            }),
        },
        Div => Ok(TypeAnnotation::Float),
        Eq | NotEq | Lt | LtEq | Gt | GtEq => Ok(TypeAnnotation::Bool),
        And | Or => Ok(TypeAnnotation::Bool),
        BitAnd | BitOr | BitXor | LShift | RShift => match (left, right) {
            (TypeAnnotation::Int, TypeAnnotation::Int) => Ok(TypeAnnotation::Int),
            _ => Err(TypeError::BinaryOpMismatch {
                op: format!("{op:?}"),
                left: format!("{left:?}"),
                right: format!("{right:?}"),
            }),
        },
    }
}

fn resolve_unary_op_type(
    op: &UnaryOp,
    operand: &TypeAnnotation,
) -> Result<TypeAnnotation, TypeError> {
    use UnaryOp::*;
    match op {
        Neg | Pos => match operand {
            TypeAnnotation::Int => Ok(TypeAnnotation::Int),
            TypeAnnotation::Float => Ok(TypeAnnotation::Float),
            _ => Err(TypeError::InferenceFailure(format!(
                "cannot negate {operand:?}"
            ))),
        },
        Not => Ok(TypeAnnotation::Bool),
        BitNot => match operand {
            TypeAnnotation::Int => Ok(TypeAnnotation::Int),
            _ => Err(TypeError::InferenceFailure(format!(
                "cannot bitwise-not {operand:?}"
            ))),
        },
    }
}

/// Walks a HIR module and resolves Unknown types where possible.
pub struct InferenceEngine {
    ctx: TypeContext,
}

impl InferenceEngine {
    pub fn new() -> Self {
        Self {
            ctx: TypeContext::new(),
        }
    }

    /// Run type inference on a module, resolving Unknown types.
    pub fn infer_module(&mut self, module: &mut Module) -> Result<(), TypeError> {
        for stmt in &mut module.body {
            self.infer_statement(stmt)?;
        }
        Ok(())
    }

    fn infer_statement(&mut self, stmt: &mut Statement) -> Result<(), TypeError> {
        match stmt {
            Statement::Assign {
                target,
                annotation,
                value,
            } => {
                let inferred = self.infer_expression(value)?;
                let resolved = if let Some(ann) = annotation {
                    if *ann != TypeAnnotation::Unknown {
                        ann.clone()
                    } else {
                        inferred
                    }
                } else {
                    inferred
                };
                self.ctx.define(target.clone(), resolved);
                Ok(())
            }
            Statement::Return { value } => {
                if let Some(expr) = value {
                    self.infer_expression(expr)?;
                }
                Ok(())
            }
            Statement::If { test, body, orelse } => {
                self.infer_expression(test)?;
                self.ctx.push_scope();
                for s in body {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();
                self.ctx.push_scope();
                for s in orelse {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();
                Ok(())
            }
            Statement::While { test, body } => {
                self.infer_expression(test)?;
                self.ctx.push_scope();
                for s in body {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();
                Ok(())
            }
            Statement::For { target, iter, body } => {
                self.infer_expression(iter)?;
                self.ctx.push_scope();
                let iter_ty = self.infer_expression(iter)?;
                let elem_ty = match iter_ty {
                    TypeAnnotation::List(inner) => *inner,
                    TypeAnnotation::Set(inner) => *inner,
                    _ => TypeAnnotation::Unknown,
                };
                self.ctx.define(target.clone(), elem_ty);
                for s in body {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();
                Ok(())
            }
            Statement::FunctionDef(func) => {
                // Build and register the function signature
                let params: Vec<(String, TypeAnnotation)> = func
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), p.annotation.clone()))
                    .collect();

                let sig = FunctionSignature {
                    name: func.name.clone(),
                    params: params.clone(),
                    return_type: func.return_type.clone(),
                };
                self.ctx.define_function(sig);

                // Also define the function name as a Callable in the variable scope
                let param_types: Vec<TypeAnnotation> =
                    params.iter().map(|(_, ty)| ty.clone()).collect();
                self.ctx.define(
                    func.name.clone(),
                    TypeAnnotation::Callable {
                        params: param_types,
                        ret: Box::new(func.return_type.clone()),
                    },
                );

                // Infer the function body in a new scope with parameters defined
                self.ctx.push_scope();
                for (name, ty) in &params {
                    self.ctx.define(name.clone(), ty.clone());
                }
                for s in &mut func.body {
                    self.infer_statement(s)?;
                }
                self.ctx.pop_scope();

                Ok(())
            }
            Statement::Expr(expr) => {
                self.infer_expression(expr)?;
                Ok(())
            }
            Statement::AugAssign { target, value, .. } => {
                self.infer_expression(value)?;
                if self.ctx.lookup(target).is_none() {
                    return Err(TypeError::UndefinedVariable(target.clone()));
                }
                Ok(())
            }
            Statement::Assert { test, msg } => {
                self.infer_expression(test)?;
                if let Some(m) = msg {
                    self.infer_expression(m)?;
                }
                Ok(())
            }
            Statement::Raise { value } => {
                if let Some(expr) = value {
                    self.infer_expression(expr)?;
                }
                Ok(())
            }
            Statement::ClassDef {
                name,
                bases,
                methods,
                fields,
            } => {
                let class_def = celer_hir::ClassDef {
                    name: name.clone(),
                    bases: bases.clone(),
                    fields: fields.clone(),
                    methods: methods.iter().map(|m| m.name.clone()).collect(),
                };
                self.ctx.define_class(class_def);
                self.ctx
                    .define(name.clone(), TypeAnnotation::Class(name.clone()));

                // Infer method bodies in a scoped context
                for method in methods {
                    self.ctx.push_scope();
                    for p in &method.params {
                        if p.name == "self" {
                            self.ctx
                                .define(p.name.clone(), TypeAnnotation::Class(name.clone()));
                        } else {
                            self.ctx.define(p.name.clone(), p.annotation.clone());
                        }
                    }
                    for s in &mut method.body.clone() {
                        let _ = self.infer_statement(&mut s.clone());
                    }
                    self.ctx.pop_scope();
                }

                Ok(())
            }
            // Statements that don't carry type information yet
            Statement::Import { .. }
            | Statement::ImportFrom { .. }
            | Statement::Pass
            | Statement::Break
            | Statement::Continue => Ok(()),
        }
    }

    fn infer_expression(&self, expr: &mut Expression) -> Result<TypeAnnotation, TypeError> {
        match expr {
            Expression::Name { id, ty } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                let resolved = self
                    .ctx
                    .lookup(id)
                    .cloned()
                    .ok_or_else(|| TypeError::UndefinedVariable(id.clone()))?;
                *ty = resolved.clone();
                Ok(resolved)
            }
            Expression::List { elements, ty } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                let elem_ty = self.infer_homogeneous_type(elements)?;
                let resolved = TypeAnnotation::List(Box::new(elem_ty));
                *ty = resolved.clone();
                Ok(resolved)
            }
            Expression::Dict { keys, values, ty } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                let key_ty = self.infer_homogeneous_type(keys)?;
                let val_ty = self.infer_homogeneous_type(values)?;
                let resolved = TypeAnnotation::Dict(Box::new(key_ty), Box::new(val_ty));
                *ty = resolved.clone();
                Ok(resolved)
            }
            Expression::Tuple { elements, ty } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                let mut elem_types = Vec::with_capacity(elements.len());
                for e in elements {
                    elem_types.push(self.infer_expression(e)?);
                }
                let resolved = TypeAnnotation::Tuple(elem_types);
                *ty = resolved.clone();
                Ok(resolved)
            }
            Expression::Call { func, args, ty } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                for arg in args.iter_mut() {
                    self.infer_expression(arg)?;
                }
                if let Expression::Name { id, .. } = func.as_mut() {
                    // Check if this is a class constructor call
                    if self.ctx.lookup_class(id).is_some() {
                        let resolved = TypeAnnotation::Class(id.clone());
                        *ty = resolved.clone();
                        return Ok(resolved);
                    }
                    // Check function registry
                    if let Some(sig) = self.ctx.lookup_function(id) {
                        let resolved = sig.return_type.clone();
                        *ty = resolved.clone();
                        return Ok(resolved);
                    }
                    if let Some(TypeAnnotation::Callable { ret, .. }) = self.ctx.lookup(id) {
                        let resolved = *ret.clone();
                        *ty = resolved.clone();
                        return Ok(resolved);
                    }
                }
                Ok(TypeAnnotation::Unknown)
            }
            Expression::BinaryOp {
                op,
                left,
                right,
                ty,
            } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                let left_ty = self.infer_expression(left)?;
                let right_ty = self.infer_expression(right)?;
                let resolved = resolve_binary_op_type(op, &left_ty, &right_ty)?;
                *ty = resolved.clone();
                Ok(resolved)
            }
            Expression::UnaryOp { op, operand, ty } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                let operand_ty = self.infer_expression(operand)?;
                let resolved = resolve_unary_op_type(op, &operand_ty)?;
                *ty = resolved.clone();
                Ok(resolved)
            }
            Expression::IfExpr {
                test,
                body,
                orelse,
                ty,
            } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                self.infer_expression(test)?;
                let body_ty = self.infer_expression(body)?;
                let else_ty = self.infer_expression(orelse)?;
                let resolved = if body_ty == else_ty {
                    body_ty
                } else {
                    TypeAnnotation::Any
                };
                *ty = resolved.clone();
                Ok(resolved)
            }
            Expression::Attribute { value, attr, ty } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                let val_ty = self.infer_expression(value)?;
                let resolved = match &val_ty {
                    TypeAnnotation::Class(class_name) => self
                        .ctx
                        .class_field_type(class_name, attr)
                        .cloned()
                        .unwrap_or(TypeAnnotation::Unknown),
                    _ => TypeAnnotation::Unknown,
                };
                *ty = resolved.clone();
                Ok(resolved)
            }
            Expression::Subscript {
                value, index, ty, ..
            } => {
                if *ty != TypeAnnotation::Unknown {
                    return Ok(ty.clone());
                }
                let val_ty = self.infer_expression(value)?;
                self.infer_expression(index)?;
                let resolved = match val_ty {
                    TypeAnnotation::List(inner) => *inner,
                    TypeAnnotation::Dict(_, v) => *v,
                    _ => TypeAnnotation::Unknown,
                };
                *ty = resolved.clone();
                Ok(resolved)
            }
            _ => Ok(expr.ty().clone()),
        }
    }

    /// Infer a single type from a homogeneous collection of expressions.
    /// Returns Unknown for empty collections.
    fn infer_homogeneous_type(
        &self,
        elements: &mut [Expression],
    ) -> Result<TypeAnnotation, TypeError> {
        if elements.is_empty() {
            return Ok(TypeAnnotation::Unknown);
        }
        let first = self.infer_expression(&mut elements[0])?;
        for elem in &mut elements[1..] {
            let ty = self.infer_expression(elem)?;
            if ty != first {
                return Ok(TypeAnnotation::Any);
            }
        }
        Ok(first)
    }
}

impl Default for InferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::{Function, Module, Parameter};

    #[test]
    fn infer_assign_from_literal() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "x".into(),
            annotation: None,
            value: Expression::IntLiteral(42),
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("x"), Some(&TypeAnnotation::Int));
    }

    #[test]
    fn infer_respects_explicit_annotation() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "y".into(),
            annotation: Some(TypeAnnotation::Float),
            value: Expression::IntLiteral(1),
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("y"), Some(&TypeAnnotation::Float));
    }

    #[test]
    fn infer_resolves_unknown_annotation() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "z".into(),
            annotation: Some(TypeAnnotation::Unknown),
            value: Expression::StringLiteral("hello".into()),
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("z"), Some(&TypeAnnotation::Str));
    }

    #[test]
    fn infer_name_lookup() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "a".into(),
            annotation: None,
            value: Expression::IntLiteral(10),
        });
        module.body.push(Statement::Assign {
            target: "b".into(),
            annotation: None,
            value: Expression::Name {
                id: "a".into(),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("b"), Some(&TypeAnnotation::Int));
    }

    // -- New tests for Workstream 2 --

    #[test]
    fn infer_function_def_registers_signature() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::FunctionDef(Function {
            name: "get_item".into(),
            params: vec![Parameter {
                name: "item_id".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Str,
            body: vec![Statement::Return {
                value: Some(Expression::StringLiteral("item".into())),
            }],
            decorators: vec![],
            is_async: false,
        }));

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();

        // Function should be registered
        let sig = engine.ctx.lookup_function("get_item").unwrap();
        assert_eq!(sig.return_type, TypeAnnotation::Str);
        assert_eq!(sig.params[0].0, "item_id");
        assert_eq!(sig.params[0].1, TypeAnnotation::Int);
    }

    #[test]
    fn infer_function_param_resolves_in_body() {
        // def process(item_id: int) -> str:
        //     x = item_id   <-- x should resolve to Int
        //     return "ok"
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::FunctionDef(Function {
            name: "process".into(),
            params: vec![Parameter {
                name: "item_id".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Str,
            body: vec![
                Statement::Assign {
                    target: "x".into(),
                    annotation: None,
                    value: Expression::Name {
                        id: "item_id".into(),
                        ty: TypeAnnotation::Unknown,
                    },
                },
                Statement::Return {
                    value: Some(Expression::StringLiteral("ok".into())),
                },
            ],
            decorators: vec![],
            is_async: false,
        }));

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        // item_id param was in scope during body inference -- no error means it resolved
    }

    #[test]
    fn infer_dict_string_keys() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "d".into(),
            annotation: None,
            value: Expression::Dict {
                keys: vec![
                    Expression::StringLiteral("a".into()),
                    Expression::StringLiteral("b".into()),
                ],
                values: vec![Expression::IntLiteral(1), Expression::IntLiteral(2)],
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(
            engine.ctx.lookup("d"),
            Some(&TypeAnnotation::Dict(
                Box::new(TypeAnnotation::Str),
                Box::new(TypeAnnotation::Int),
            ))
        );
    }

    #[test]
    fn infer_dict_mixed_values() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "d".into(),
            annotation: None,
            value: Expression::Dict {
                keys: vec![
                    Expression::StringLiteral("x".into()),
                    Expression::StringLiteral("y".into()),
                ],
                values: vec![
                    Expression::IntLiteral(1),
                    Expression::StringLiteral("two".into()),
                ],
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(
            engine.ctx.lookup("d"),
            Some(&TypeAnnotation::Dict(
                Box::new(TypeAnnotation::Str),
                Box::new(TypeAnnotation::Any),
            ))
        );
    }

    #[test]
    fn infer_call_resolves_return_type() {
        let mut module = Module::new("test", "test.py");

        // Define: def greet(name: str) -> str: return "hi"
        module.body.push(Statement::FunctionDef(Function {
            name: "greet".into(),
            params: vec![Parameter {
                name: "name".into(),
                annotation: TypeAnnotation::Str,
                default: None,
            }],
            return_type: TypeAnnotation::Str,
            body: vec![Statement::Return {
                value: Some(Expression::StringLiteral("hi".into())),
            }],
            decorators: vec![],
            is_async: false,
        }));

        // result = greet("world")
        module.body.push(Statement::Assign {
            target: "result".into(),
            annotation: None,
            value: Expression::Call {
                func: Box::new(Expression::Name {
                    id: "greet".into(),
                    ty: TypeAnnotation::Unknown,
                }),
                args: vec![Expression::StringLiteral("world".into())],
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("result"), Some(&TypeAnnotation::Str));
    }

    #[test]
    fn infer_list_expression() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "nums".into(),
            annotation: None,
            value: Expression::List {
                elements: vec![
                    Expression::IntLiteral(1),
                    Expression::IntLiteral(2),
                    Expression::IntLiteral(3),
                ],
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(
            engine.ctx.lookup("nums"),
            Some(&TypeAnnotation::List(Box::new(TypeAnnotation::Int)))
        );
    }

    #[test]
    fn infer_tuple_expression() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "pair".into(),
            annotation: None,
            value: Expression::Tuple {
                elements: vec![
                    Expression::IntLiteral(1),
                    Expression::StringLiteral("hello".into()),
                ],
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(
            engine.ctx.lookup("pair"),
            Some(&TypeAnnotation::Tuple(vec![
                TypeAnnotation::Int,
                TypeAnnotation::Str,
            ]))
        );
    }

    #[test]
    fn infer_empty_list() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "empty".into(),
            annotation: None,
            value: Expression::List {
                elements: vec![],
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(
            engine.ctx.lookup("empty"),
            Some(&TypeAnnotation::List(Box::new(TypeAnnotation::Unknown)))
        );
    }

    #[test]
    fn full_pipeline_function_and_call() {
        // Simulates:
        //   def add(a: int, b: int) -> int:
        //       return a
        //   result = add(1, 2)
        //   names = ["alice", "bob"]
        //   config = {"host": "localhost", "port": "8080"}

        let mut module = Module::new("test", "test.py");

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
                value: Some(Expression::Name {
                    id: "a".into(),
                    ty: TypeAnnotation::Unknown,
                }),
            }],
            decorators: vec![],
            is_async: false,
        }));

        module.body.push(Statement::Assign {
            target: "result".into(),
            annotation: None,
            value: Expression::Call {
                func: Box::new(Expression::Name {
                    id: "add".into(),
                    ty: TypeAnnotation::Unknown,
                }),
                args: vec![Expression::IntLiteral(1), Expression::IntLiteral(2)],
                ty: TypeAnnotation::Unknown,
            },
        });

        module.body.push(Statement::Assign {
            target: "names".into(),
            annotation: None,
            value: Expression::List {
                elements: vec![
                    Expression::StringLiteral("alice".into()),
                    Expression::StringLiteral("bob".into()),
                ],
                ty: TypeAnnotation::Unknown,
            },
        });

        module.body.push(Statement::Assign {
            target: "config".into(),
            annotation: None,
            value: Expression::Dict {
                keys: vec![
                    Expression::StringLiteral("host".into()),
                    Expression::StringLiteral("port".into()),
                ],
                values: vec![
                    Expression::StringLiteral("localhost".into()),
                    Expression::StringLiteral("8080".into()),
                ],
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();

        // Function call resolves to Int
        assert_eq!(engine.ctx.lookup("result"), Some(&TypeAnnotation::Int));

        // List of strings
        assert_eq!(
            engine.ctx.lookup("names"),
            Some(&TypeAnnotation::List(Box::new(TypeAnnotation::Str)))
        );

        // Dict(Str, Str)
        assert_eq!(
            engine.ctx.lookup("config"),
            Some(&TypeAnnotation::Dict(
                Box::new(TypeAnnotation::Str),
                Box::new(TypeAnnotation::Str),
            ))
        );

        // Function registered in registry
        let sig = engine.ctx.lookup_function("add").unwrap();
        assert_eq!(sig.return_type, TypeAnnotation::Int);
        assert_eq!(sig.params.len(), 2);
    }

    // -- Phase 2a: BinaryOp / UnaryOp inference tests --

    #[test]
    fn infer_binary_int_add() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "r".into(),
            annotation: None,
            value: Expression::BinaryOp {
                op: celer_hir::BinaryOp::Add,
                left: Box::new(Expression::IntLiteral(1)),
                right: Box::new(Expression::IntLiteral(2)),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("r"), Some(&TypeAnnotation::Int));
    }

    #[test]
    fn infer_binary_int_float_yields_float() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "r".into(),
            annotation: None,
            value: Expression::BinaryOp {
                op: celer_hir::BinaryOp::Mul,
                left: Box::new(Expression::IntLiteral(3)),
                right: Box::new(Expression::FloatLiteral(2.0)),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("r"), Some(&TypeAnnotation::Float));
    }

    #[test]
    fn infer_binary_div_always_float() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "r".into(),
            annotation: None,
            value: Expression::BinaryOp {
                op: celer_hir::BinaryOp::Div,
                left: Box::new(Expression::IntLiteral(10)),
                right: Box::new(Expression::IntLiteral(3)),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("r"), Some(&TypeAnnotation::Float));
    }

    #[test]
    fn infer_comparison_yields_bool() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "r".into(),
            annotation: None,
            value: Expression::BinaryOp {
                op: celer_hir::BinaryOp::Lt,
                left: Box::new(Expression::IntLiteral(1)),
                right: Box::new(Expression::IntLiteral(2)),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("r"), Some(&TypeAnnotation::Bool));
    }

    #[test]
    fn infer_unary_neg_preserves_type() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "a".into(),
            annotation: None,
            value: Expression::UnaryOp {
                op: celer_hir::UnaryOp::Neg,
                operand: Box::new(Expression::IntLiteral(5)),
                ty: TypeAnnotation::Unknown,
            },
        });
        module.body.push(Statement::Assign {
            target: "b".into(),
            annotation: None,
            value: Expression::UnaryOp {
                op: celer_hir::UnaryOp::Neg,
                operand: Box::new(Expression::FloatLiteral(3.14)),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("a"), Some(&TypeAnnotation::Int));
        assert_eq!(engine.ctx.lookup("b"), Some(&TypeAnnotation::Float));
    }

    #[test]
    fn infer_unary_not_yields_bool() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "r".into(),
            annotation: None,
            value: Expression::UnaryOp {
                op: celer_hir::UnaryOp::Not,
                operand: Box::new(Expression::BoolLiteral(true)),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("r"), Some(&TypeAnnotation::Bool));
    }

    #[test]
    fn infer_bitwise_on_ints() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "r".into(),
            annotation: None,
            value: Expression::BinaryOp {
                op: celer_hir::BinaryOp::BitAnd,
                left: Box::new(Expression::IntLiteral(0xFF)),
                right: Box::new(Expression::IntLiteral(0x0F)),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("r"), Some(&TypeAnnotation::Int));
    }

    #[test]
    fn infer_string_concat() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "r".into(),
            annotation: None,
            value: Expression::BinaryOp {
                op: celer_hir::BinaryOp::Add,
                left: Box::new(Expression::StringLiteral("hello".into())),
                right: Box::new(Expression::StringLiteral(" world".into())),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("r"), Some(&TypeAnnotation::Str));
    }

    #[test]
    fn infer_if_expr() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::Assign {
            target: "r".into(),
            annotation: None,
            value: Expression::IfExpr {
                test: Box::new(Expression::BoolLiteral(true)),
                body: Box::new(Expression::IntLiteral(1)),
                orelse: Box::new(Expression::IntLiteral(2)),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("r"), Some(&TypeAnnotation::Int));
    }

    // -- Phase 2b: ClassDef / Attribute / Constructor tests --

    #[test]
    fn infer_class_def_registers_class() {
        let mut module = Module::new("test", "test.py");
        module.body.push(Statement::ClassDef {
            name: "User".into(),
            bases: vec!["BaseModel".into()],
            fields: vec![
                ("name".into(), TypeAnnotation::Str),
                ("age".into(), TypeAnnotation::Int),
            ],
            methods: vec![],
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();

        assert_eq!(
            engine.ctx.lookup("User"),
            Some(&TypeAnnotation::Class("User".into()))
        );
        assert!(engine.ctx.lookup_class("User").is_some());
    }

    #[test]
    fn infer_class_constructor_call() {
        let mut module = Module::new("test", "test.py");

        module.body.push(Statement::ClassDef {
            name: "User".into(),
            bases: vec!["BaseModel".into()],
            fields: vec![("name".into(), TypeAnnotation::Str)],
            methods: vec![],
        });

        module.body.push(Statement::Assign {
            target: "u".into(),
            annotation: None,
            value: Expression::Call {
                func: Box::new(Expression::Name {
                    id: "User".into(),
                    ty: TypeAnnotation::Unknown,
                }),
                args: vec![Expression::StringLiteral("alice".into())],
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(
            engine.ctx.lookup("u"),
            Some(&TypeAnnotation::Class("User".into()))
        );
    }

    #[test]
    fn infer_attribute_access_on_class() {
        let mut module = Module::new("test", "test.py");

        module.body.push(Statement::ClassDef {
            name: "Item".into(),
            bases: vec![],
            fields: vec![
                ("price".into(), TypeAnnotation::Float),
                ("name".into(), TypeAnnotation::Str),
            ],
            methods: vec![],
        });

        module.body.push(Statement::Assign {
            target: "item".into(),
            annotation: None,
            value: Expression::Call {
                func: Box::new(Expression::Name {
                    id: "Item".into(),
                    ty: TypeAnnotation::Unknown,
                }),
                args: vec![],
                ty: TypeAnnotation::Unknown,
            },
        });

        module.body.push(Statement::Assign {
            target: "p".into(),
            annotation: None,
            value: Expression::Attribute {
                value: Box::new(Expression::Name {
                    id: "item".into(),
                    ty: TypeAnnotation::Unknown,
                }),
                attr: "price".into(),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("p"), Some(&TypeAnnotation::Float));
    }

    #[test]
    fn infer_attribute_unknown_field() {
        let mut module = Module::new("test", "test.py");

        module.body.push(Statement::ClassDef {
            name: "Item".into(),
            bases: vec![],
            fields: vec![("price".into(), TypeAnnotation::Float)],
            methods: vec![],
        });

        module.body.push(Statement::Assign {
            target: "item".into(),
            annotation: None,
            value: Expression::Call {
                func: Box::new(Expression::Name {
                    id: "Item".into(),
                    ty: TypeAnnotation::Unknown,
                }),
                args: vec![],
                ty: TypeAnnotation::Unknown,
            },
        });

        module.body.push(Statement::Assign {
            target: "x".into(),
            annotation: None,
            value: Expression::Attribute {
                value: Box::new(Expression::Name {
                    id: "item".into(),
                    ty: TypeAnnotation::Unknown,
                }),
                attr: "nonexistent".into(),
                ty: TypeAnnotation::Unknown,
            },
        });

        let mut engine = InferenceEngine::new();
        engine.infer_module(&mut module).unwrap();
        assert_eq!(engine.ctx.lookup("x"), Some(&TypeAnnotation::Unknown));
    }
}

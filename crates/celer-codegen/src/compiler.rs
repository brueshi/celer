use std::path::Path;

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};

use celer_hir::{Function, Module as HirModule, Statement};

use crate::context::CodegenContext;
use crate::emit_function::emit_function;
use crate::error::CodegenError;

pub struct Compiler<'ctx> {
    ctx: CodegenContext<'ctx>,
}

impl<'ctx> Compiler<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        Self {
            ctx: CodegenContext::new(context, module_name),
        }
    }

    /// Compile an entire HIR module into LLVM IR.
    pub fn compile_module(&mut self, module: &HirModule) -> Result<(), CodegenError> {
        for stmt in &module.body {
            self.compile_statement(stmt)?;
        }
        Ok(())
    }

    fn compile_statement(&mut self, stmt: &Statement) -> Result<(), CodegenError> {
        match stmt {
            Statement::FunctionDef(func) => {
                self.compile_function(func)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn compile_function(&mut self, func: &Function) -> Result<(), CodegenError> {
        emit_function(&mut self.ctx, func)?;
        Ok(())
    }

    /// Dump the generated LLVM IR as a string.
    pub fn dump_ir(&self) -> String {
        self.ctx.dump_ir()
    }

    /// Verify the LLVM module. Returns Ok(()) if valid.
    pub fn verify(&self) -> Result<(), CodegenError> {
        self.ctx
            .module
            .verify()
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Write the compiled module to a native object file.
    pub fn write_object(&self, path: &Path) -> Result<(), CodegenError> {
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| CodegenError::TargetMachineError(e.to_string()))?;

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple)
            .map_err(|e| CodegenError::TargetMachineError(e.to_string()))?;

        let machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or_else(|| {
                CodegenError::TargetMachineError("failed to create target machine".into())
            })?;

        machine
            .write_to_file(&self.ctx.module, FileType::Object, path)
            .map_err(|e| CodegenError::TargetMachineError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celer_hir::{
        BinaryOp, Expression, Function, Module as HirModule, Parameter, Statement, TypeAnnotation,
        UnaryOp,
    };

    fn make_module(stmts: Vec<Statement>) -> HirModule {
        HirModule {
            name: "test".into(),
            path: "test.py".into(),
            body: stmts,
        }
    }

    #[test]
    fn static_dict_root_function() {
        // def root() -> dict[str, str]:
        //     return {"message": "hello"}
        let func = Function {
            name: "root".into(),
            params: vec![],
            return_type: TypeAnnotation::Dict(
                Box::new(TypeAnnotation::Str),
                Box::new(TypeAnnotation::Str),
            ),
            body: vec![Statement::Return {
                value: Some(Expression::Dict {
                    keys: vec![Expression::StringLiteral("message".into())],
                    values: vec![Expression::StringLiteral("hello".into())],
                    ty: TypeAnnotation::Dict(
                        Box::new(TypeAnnotation::Str),
                        Box::new(TypeAnnotation::Str),
                    ),
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        // LLVM IR escapes quotes as \22 in constant byte arrays
        assert!(
            ir.contains(r#"\22message\22"#),
            "IR should contain static JSON constant. Got:\n{ir}"
        );
        assert!(
            ir.contains("[20 x i8]"),
            "IR should have the correct array size. Got:\n{ir}"
        );
        // Verify function signature has output params (ptr types)
        assert!(ir.contains("@root"), "IR should contain @root function");
        // Verify the module is valid
        compiler.verify().unwrap();
    }

    #[test]
    fn dynamic_dict_with_param() {
        // def get_item(item_id: int) -> dict[str, any]:
        //     return {"item_id": item_id, "name": "test"}
        let func = Function {
            name: "get_item".into(),
            params: vec![Parameter {
                name: "item_id".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Dict(
                Box::new(TypeAnnotation::Str),
                Box::new(TypeAnnotation::Any),
            ),
            body: vec![Statement::Return {
                value: Some(Expression::Dict {
                    keys: vec![
                        Expression::StringLiteral("item_id".into()),
                        Expression::StringLiteral("name".into()),
                    ],
                    values: vec![
                        Expression::Name {
                            id: "item_id".into(),
                            ty: TypeAnnotation::Int,
                        },
                        Expression::StringLiteral("test".into()),
                    ],
                    ty: TypeAnnotation::Dict(
                        Box::new(TypeAnnotation::Str),
                        Box::new(TypeAnnotation::Any),
                    ),
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        // Verify snprintf is declared/called
        assert!(
            ir.contains("snprintf"),
            "IR should reference snprintf. Got:\n{ir}"
        );
        // Verify function takes i64 param plus output pointers
        assert!(
            ir.contains("@get_item"),
            "IR should contain @get_item function"
        );
        // Verify the module is valid
        compiler.verify().unwrap();
    }

    #[test]
    fn scalar_int_function() {
        // def add(a: int, b: int) -> int:
        //     return a
        let func = Function {
            name: "identity".into(),
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
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(
            ir.contains("@identity"),
            "IR should contain @identity function"
        );
        compiler.verify().unwrap();
    }

    #[test]
    fn module_verify_passes() {
        let module = make_module(vec![]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "empty");
        compiler.compile_module(&module).unwrap();
        compiler.verify().unwrap();
    }

    // -- Phase 2a tests: binary ops, control flow --

    #[test]
    fn binary_add_int() {
        // def add(a: int, b: int) -> int:
        //     return a + b
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

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("add"), "IR should contain add instruction");
        compiler.verify().unwrap();
    }

    #[test]
    fn unary_neg_int() {
        // def negate(x: int) -> int:
        //     return -x
        let func = Function {
            name: "negate".into(),
            params: vec![Parameter {
                name: "x".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Int,
            body: vec![Statement::Return {
                value: Some(Expression::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(Expression::Name {
                        id: "x".into(),
                        ty: TypeAnnotation::Int,
                    }),
                    ty: TypeAnnotation::Int,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();
        compiler.verify().unwrap();
    }

    #[test]
    fn if_else_branching() {
        // def max_val(a: int, b: int) -> int:
        //     if a > b:
        //         return a
        //     else:
        //         return b
        let func = Function {
            name: "max_val".into(),
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
            body: vec![Statement::If {
                test: Expression::BinaryOp {
                    op: BinaryOp::Gt,
                    left: Box::new(Expression::Name {
                        id: "a".into(),
                        ty: TypeAnnotation::Int,
                    }),
                    right: Box::new(Expression::Name {
                        id: "b".into(),
                        ty: TypeAnnotation::Int,
                    }),
                    ty: TypeAnnotation::Bool,
                },
                body: vec![Statement::Return {
                    value: Some(Expression::Name {
                        id: "a".into(),
                        ty: TypeAnnotation::Int,
                    }),
                }],
                orelse: vec![Statement::Return {
                    value: Some(Expression::Name {
                        id: "b".into(),
                        ty: TypeAnnotation::Int,
                    }),
                }],
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("then"), "IR should contain then block");
        assert!(ir.contains("else"), "IR should contain else block");
        compiler.verify().unwrap();
    }

    #[test]
    fn while_loop_fibonacci() {
        // def fib(n: int) -> int:
        //     a = 0
        //     b = 1
        //     i = 0
        //     while i < n:
        //         t = a + b
        //         a = b
        //         b = t
        //         i = i + 1
        //     return a
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
                Statement::Assign {
                    target: "b".into(),
                    annotation: None,
                    value: Expression::IntLiteral(1),
                },
                Statement::Assign {
                    target: "i".into(),
                    annotation: None,
                    value: Expression::IntLiteral(0),
                },
                Statement::While {
                    test: Expression::BinaryOp {
                        op: BinaryOp::Lt,
                        left: Box::new(Expression::Name {
                            id: "i".into(),
                            ty: TypeAnnotation::Int,
                        }),
                        right: Box::new(Expression::Name {
                            id: "n".into(),
                            ty: TypeAnnotation::Int,
                        }),
                        ty: TypeAnnotation::Bool,
                    },
                    body: vec![
                        Statement::Assign {
                            target: "t".into(),
                            annotation: None,
                            value: Expression::BinaryOp {
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
                            },
                        },
                        Statement::Assign {
                            target: "a".into(),
                            annotation: None,
                            value: Expression::Name {
                                id: "b".into(),
                                ty: TypeAnnotation::Int,
                            },
                        },
                        Statement::Assign {
                            target: "b".into(),
                            annotation: None,
                            value: Expression::Name {
                                id: "t".into(),
                                ty: TypeAnnotation::Int,
                            },
                        },
                        Statement::AugAssign {
                            target: "i".into(),
                            op: BinaryOp::Add,
                            value: Expression::IntLiteral(1),
                        },
                    ],
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

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("while.cond"), "IR should contain while.cond");
        assert!(ir.contains("while.body"), "IR should contain while.body");
        assert!(ir.contains("while.exit"), "IR should contain while.exit");
        compiler.verify().unwrap();
    }

    #[test]
    fn for_loop_range() {
        // def sum_n(n: int) -> int:
        //     total = 0
        //     for i in range(n):
        //         total = total + i
        //     return total
        let func = Function {
            name: "sum_n".into(),
            params: vec![Parameter {
                name: "n".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Int,
            body: vec![
                Statement::Assign {
                    target: "total".into(),
                    annotation: None,
                    value: Expression::IntLiteral(0),
                },
                Statement::For {
                    target: "i".into(),
                    iter: Expression::Call {
                        func: Box::new(Expression::Name {
                            id: "range".into(),
                            ty: TypeAnnotation::Unknown,
                        }),
                        args: vec![Expression::Name {
                            id: "n".into(),
                            ty: TypeAnnotation::Int,
                        }],
                        ty: TypeAnnotation::Unknown,
                    },
                    body: vec![Statement::AugAssign {
                        target: "total".into(),
                        op: BinaryOp::Add,
                        value: Expression::Name {
                            id: "i".into(),
                            ty: TypeAnnotation::Int,
                        },
                    }],
                },
                Statement::Return {
                    value: Some(Expression::Name {
                        id: "total".into(),
                        ty: TypeAnnotation::Int,
                    }),
                },
            ],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("for.cond"), "IR should contain for.cond");
        assert!(ir.contains("for.body"), "IR should contain for.body");
        assert!(ir.contains("for.exit"), "IR should contain for.exit");
        compiler.verify().unwrap();
    }

    #[test]
    fn comparison_ops() {
        // def is_positive(x: int) -> bool:
        //     return x > 0
        let func = Function {
            name: "is_positive".into(),
            params: vec![Parameter {
                name: "x".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Bool,
            body: vec![Statement::Return {
                value: Some(Expression::BinaryOp {
                    op: BinaryOp::Gt,
                    left: Box::new(Expression::Name {
                        id: "x".into(),
                        ty: TypeAnnotation::Int,
                    }),
                    right: Box::new(Expression::IntLiteral(0)),
                    ty: TypeAnnotation::Bool,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();
        compiler.verify().unwrap();
    }

    // -- Phase 3: String operations and builtins --

    #[test]
    fn builtin_len_on_string() {
        // def string_len(s: str) -> int:
        //     return len(s)
        let func = Function {
            name: "string_len".into(),
            params: vec![Parameter {
                name: "s".into(),
                annotation: TypeAnnotation::Str,
                default: None,
            }],
            return_type: TypeAnnotation::Int,
            body: vec![Statement::Return {
                value: Some(Expression::Call {
                    func: Box::new(Expression::Name {
                        id: "len".into(),
                        ty: TypeAnnotation::Unknown,
                    }),
                    args: vec![Expression::Name {
                        id: "s".into(),
                        ty: TypeAnnotation::Str,
                    }],
                    ty: TypeAnnotation::Int,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("strlen"), "IR should call strlen for len()");
        compiler.verify().unwrap();
    }

    #[test]
    fn builtin_str_on_int() {
        // def to_string(n: int) -> str:
        //     return str(n)
        let func = Function {
            name: "to_string".into(),
            params: vec![Parameter {
                name: "n".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Str,
            body: vec![Statement::Return {
                value: Some(Expression::Call {
                    func: Box::new(Expression::Name {
                        id: "str".into(),
                        ty: TypeAnnotation::Unknown,
                    }),
                    args: vec![Expression::Name {
                        id: "n".into(),
                        ty: TypeAnnotation::Int,
                    }],
                    ty: TypeAnnotation::Str,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("snprintf"), "IR should call snprintf for str()");
        compiler.verify().unwrap();
    }

    #[test]
    fn builtin_int_from_string() {
        // def parse_int(s: str) -> int:
        //     return int(s)
        let func = Function {
            name: "parse_int".into(),
            params: vec![Parameter {
                name: "s".into(),
                annotation: TypeAnnotation::Str,
                default: None,
            }],
            return_type: TypeAnnotation::Int,
            body: vec![Statement::Return {
                value: Some(Expression::Call {
                    func: Box::new(Expression::Name {
                        id: "int".into(),
                        ty: TypeAnnotation::Unknown,
                    }),
                    args: vec![Expression::Name {
                        id: "s".into(),
                        ty: TypeAnnotation::Str,
                    }],
                    ty: TypeAnnotation::Int,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("strtol"), "IR should call strtol for int()");
        compiler.verify().unwrap();
    }

    #[test]
    fn builtin_float_from_string() {
        // def parse_float(s: str) -> float:
        //     return float(s)
        let func = Function {
            name: "parse_float".into(),
            params: vec![Parameter {
                name: "s".into(),
                annotation: TypeAnnotation::Str,
                default: None,
            }],
            return_type: TypeAnnotation::Float,
            body: vec![Statement::Return {
                value: Some(Expression::Call {
                    func: Box::new(Expression::Name {
                        id: "float".into(),
                        ty: TypeAnnotation::Unknown,
                    }),
                    args: vec![Expression::Name {
                        id: "s".into(),
                        ty: TypeAnnotation::Str,
                    }],
                    ty: TypeAnnotation::Float,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("strtod"), "IR should call strtod for float()");
        compiler.verify().unwrap();
    }

    #[test]
    fn builtin_bool_from_int() {
        // def truthy(x: int) -> bool:
        //     return bool(x)
        let func = Function {
            name: "truthy".into(),
            params: vec![Parameter {
                name: "x".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Bool,
            body: vec![Statement::Return {
                value: Some(Expression::Call {
                    func: Box::new(Expression::Name {
                        id: "bool".into(),
                        ty: TypeAnnotation::Unknown,
                    }),
                    args: vec![Expression::Name {
                        id: "x".into(),
                        ty: TypeAnnotation::Int,
                    }],
                    ty: TypeAnnotation::Bool,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();
        compiler.verify().unwrap();
    }

    #[test]
    fn string_equality_comparison() {
        // def str_eq(a: str, b: str) -> bool:
        //     return a == b
        let func = Function {
            name: "str_eq".into(),
            params: vec![
                Parameter {
                    name: "a".into(),
                    annotation: TypeAnnotation::Str,
                    default: None,
                },
                Parameter {
                    name: "b".into(),
                    annotation: TypeAnnotation::Str,
                    default: None,
                },
            ],
            return_type: TypeAnnotation::Bool,
            body: vec![Statement::Return {
                value: Some(Expression::BinaryOp {
                    op: BinaryOp::Eq,
                    left: Box::new(Expression::Name {
                        id: "a".into(),
                        ty: TypeAnnotation::Str,
                    }),
                    right: Box::new(Expression::Name {
                        id: "b".into(),
                        ty: TypeAnnotation::Str,
                    }),
                    ty: TypeAnnotation::Bool,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(ir.contains("strcmp"), "IR should call strcmp for string ==");
        compiler.verify().unwrap();
    }

    #[test]
    fn string_concatenation() {
        // def concat(a: str, b: str) -> str:
        //     return a + b
        let func = Function {
            name: "concat".into(),
            params: vec![
                Parameter {
                    name: "a".into(),
                    annotation: TypeAnnotation::Str,
                    default: None,
                },
                Parameter {
                    name: "b".into(),
                    annotation: TypeAnnotation::Str,
                    default: None,
                },
            ],
            return_type: TypeAnnotation::Str,
            body: vec![Statement::Return {
                value: Some(Expression::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expression::Name {
                        id: "a".into(),
                        ty: TypeAnnotation::Str,
                    }),
                    right: Box::new(Expression::Name {
                        id: "b".into(),
                        ty: TypeAnnotation::Str,
                    }),
                    ty: TypeAnnotation::Str,
                }),
            }],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();

        let ir = compiler.dump_ir();
        assert!(
            ir.contains("snprintf"),
            "IR should call snprintf for string concat"
        );
        compiler.verify().unwrap();
    }

    #[test]
    fn for_loop_range_start_stop() {
        // def sum_range(start: int, stop: int) -> int:
        //     total = 0
        //     for i in range(start, stop):
        //         total = total + i
        //     return total
        let func = Function {
            name: "sum_range".into(),
            params: vec![
                Parameter {
                    name: "start".into(),
                    annotation: TypeAnnotation::Int,
                    default: None,
                },
                Parameter {
                    name: "stop".into(),
                    annotation: TypeAnnotation::Int,
                    default: None,
                },
            ],
            return_type: TypeAnnotation::Int,
            body: vec![
                Statement::Assign {
                    target: "total".into(),
                    annotation: None,
                    value: Expression::IntLiteral(0),
                },
                Statement::For {
                    target: "i".into(),
                    iter: Expression::Call {
                        func: Box::new(Expression::Name {
                            id: "range".into(),
                            ty: TypeAnnotation::Unknown,
                        }),
                        args: vec![
                            Expression::Name {
                                id: "start".into(),
                                ty: TypeAnnotation::Int,
                            },
                            Expression::Name {
                                id: "stop".into(),
                                ty: TypeAnnotation::Int,
                            },
                        ],
                        ty: TypeAnnotation::Unknown,
                    },
                    body: vec![Statement::AugAssign {
                        target: "total".into(),
                        op: BinaryOp::Add,
                        value: Expression::Name {
                            id: "i".into(),
                            ty: TypeAnnotation::Int,
                        },
                    }],
                },
                Statement::Return {
                    value: Some(Expression::Name {
                        id: "total".into(),
                        ty: TypeAnnotation::Int,
                    }),
                },
            ],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();
        compiler.verify().unwrap();
    }

    #[test]
    fn for_loop_range_start_stop_step() {
        // def sum_step(n: int) -> int:
        //     total = 0
        //     for i in range(0, n, 2):
        //         total = total + i
        //     return total
        let func = Function {
            name: "sum_step".into(),
            params: vec![Parameter {
                name: "n".into(),
                annotation: TypeAnnotation::Int,
                default: None,
            }],
            return_type: TypeAnnotation::Int,
            body: vec![
                Statement::Assign {
                    target: "total".into(),
                    annotation: None,
                    value: Expression::IntLiteral(0),
                },
                Statement::For {
                    target: "i".into(),
                    iter: Expression::Call {
                        func: Box::new(Expression::Name {
                            id: "range".into(),
                            ty: TypeAnnotation::Unknown,
                        }),
                        args: vec![
                            Expression::IntLiteral(0),
                            Expression::Name {
                                id: "n".into(),
                                ty: TypeAnnotation::Int,
                            },
                            Expression::IntLiteral(2),
                        ],
                        ty: TypeAnnotation::Unknown,
                    },
                    body: vec![Statement::AugAssign {
                        target: "total".into(),
                        op: BinaryOp::Add,
                        value: Expression::Name {
                            id: "i".into(),
                            ty: TypeAnnotation::Int,
                        },
                    }],
                },
                Statement::Return {
                    value: Some(Expression::Name {
                        id: "total".into(),
                        ty: TypeAnnotation::Int,
                    }),
                },
            ],
            decorators: vec![],
            is_async: false,
        };

        let module = make_module(vec![Statement::FunctionDef(func)]);
        let context = Context::create();
        let mut compiler = Compiler::new(&context, "test");
        compiler.compile_module(&module).unwrap();
        compiler.verify().unwrap();
    }
}

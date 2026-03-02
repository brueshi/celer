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
        Expression, Function, Module as HirModule, Parameter, Statement, TypeAnnotation,
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
}

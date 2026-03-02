use celer_hir::{Function, Module as HirModule, Statement, TypeAnnotation};
use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;

use crate::context::CodegenContext;
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

    pub fn compile_module(&mut self, module: &HirModule) -> Result<(), CodegenError> {
        for stmt in &module.body {
            self.compile_statement(stmt)?;
        }
        Ok(())
    }

    fn compile_statement(&mut self, stmt: &Statement) -> Result<(), CodegenError> {
        match stmt {
            Statement::FunctionDef(func) => self.compile_function(func),
            _ => Ok(()), // TODO: handle remaining statements
        }
    }

    fn compile_function(&mut self, func: &Function) -> Result<(), CodegenError> {
        let ret_type = self.resolve_type(&func.return_type)?;

        let fn_type = match ret_type {
            Some(BasicTypeEnum::IntType(t)) => t.fn_type(&[], false),
            Some(BasicTypeEnum::FloatType(t)) => t.fn_type(&[], false),
            None => self.ctx.context.void_type().fn_type(&[], false),
            _ => {
                return Err(CodegenError::UnsupportedType(format!(
                    "{:?}",
                    func.return_type
                )));
            }
        };

        let _fn_val = self.ctx.module.add_function(&func.name, fn_type, None);
        Ok(())
    }

    fn resolve_type(
        &self,
        ty: &TypeAnnotation,
    ) -> Result<Option<BasicTypeEnum<'ctx>>, CodegenError> {
        match ty {
            TypeAnnotation::Int => Ok(Some(self.ctx.context.i64_type().into())),
            TypeAnnotation::Float => Ok(Some(self.ctx.context.f64_type().into())),
            TypeAnnotation::Bool => Ok(Some(self.ctx.context.bool_type().into())),
            TypeAnnotation::None => Ok(None),
            TypeAnnotation::Unknown => Err(CodegenError::UnresolvedType),
            _ => Err(CodegenError::UnsupportedType(format!("{ty:?}"))),
        }
    }

    pub fn dump_ir(&self) -> String {
        self.ctx.dump_ir()
    }
}

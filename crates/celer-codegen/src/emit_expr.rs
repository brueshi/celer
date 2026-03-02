use inkwell::AddressSpace;
use inkwell::values::BasicValueEnum;

use celer_hir::Expression;

use crate::context::CodegenContext;
use crate::error::CodegenError;

/// Emit LLVM IR for a scalar expression and return the resulting value.
/// Dict expressions are not handled here -- they use the JSON emitter path.
pub fn emit_expression<'ctx>(
    ctx: &CodegenContext<'ctx>,
    expr: &Expression,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    match expr {
        Expression::IntLiteral(n) => Ok(ctx.context.i64_type().const_int(*n as u64, true).into()),
        Expression::FloatLiteral(f) => Ok(ctx.context.f64_type().const_float(*f).into()),
        Expression::BoolLiteral(b) => Ok(ctx
            .context
            .bool_type()
            .const_int(if *b { 1 } else { 0 }, false)
            .into()),
        Expression::StringLiteral(s) => {
            let gv = ctx
                .builder
                .build_global_string_ptr(s, "str")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(gv.as_pointer_value().into())
        }
        Expression::NoneLiteral => {
            // Represent None as a null pointer
            let null = ctx.context.ptr_type(AddressSpace::default()).const_null();
            Ok(null.into())
        }
        Expression::Name { id, ty, .. } => {
            let ptr = ctx
                .get_local(id)
                .ok_or_else(|| CodegenError::UndefinedVariable(id.clone()))?;
            let llvm_ty = crate::types::resolve_type(ctx.context, ty)?.ok_or_else(|| {
                CodegenError::UnsupportedType(format!("cannot load variable of type {ty:?}"))
            })?;
            let val = ctx
                .builder
                .build_load(llvm_ty, ptr, id)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(val)
        }
        _ => Err(CodegenError::UnsupportedExpression(format!("{expr:?}"))),
    }
}

use inkwell::values::BasicValueEnum;
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use celer_hir::{BinaryOp, Expression, TypeAnnotation, UnaryOp};

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
        Expression::BinaryOp {
            op,
            left,
            right,
            ty,
        } => emit_binary_op(ctx, op, left, right, ty),
        Expression::UnaryOp { op, operand, ty } => emit_unary_op(ctx, op, operand, ty),
        Expression::Call { func, args, .. } => emit_call(ctx, func, args),
        _ => Err(CodegenError::UnsupportedExpression(format!("{expr:?}"))),
    }
}

fn emit_binary_op<'ctx>(
    ctx: &CodegenContext<'ctx>,
    op: &BinaryOp,
    left: &Expression,
    right: &Expression,
    ty: &TypeAnnotation,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let lhs = emit_expression(ctx, left)?;
    let rhs = emit_expression(ctx, right)?;
    emit_binary_op_values(ctx, op, lhs, rhs, ty)
}

/// Perform a binary operation on two already-emitted LLVM values.
pub fn emit_binary_op_values<'ctx>(
    ctx: &CodegenContext<'ctx>,
    op: &BinaryOp,
    lhs: BasicValueEnum<'ctx>,
    rhs: BasicValueEnum<'ctx>,
    result_ty: &TypeAnnotation,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let left_ty = left_type_from_value(lhs);

    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Mod | BinaryOp::FloorDiv => {
            if left_ty == ValType::Int {
                let l = lhs.into_int_value();
                let r = rhs.into_int_value();
                let res = match op {
                    BinaryOp::Add => ctx
                        .builder
                        .build_int_add(l, r, "add")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    BinaryOp::Sub => ctx
                        .builder
                        .build_int_sub(l, r, "sub")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    BinaryOp::Mul => ctx
                        .builder
                        .build_int_mul(l, r, "mul")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    BinaryOp::FloorDiv => ctx
                        .builder
                        .build_int_signed_div(l, r, "floordiv")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    BinaryOp::Mod => ctx
                        .builder
                        .build_int_signed_rem(l, r, "mod")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    _ => unreachable!(),
                };
                Ok(res.into())
            } else {
                let l = lhs.into_float_value();
                let r = rhs.into_float_value();
                let res = match op {
                    BinaryOp::Add => ctx
                        .builder
                        .build_float_add(l, r, "fadd")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    BinaryOp::Sub => ctx
                        .builder
                        .build_float_sub(l, r, "fsub")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    BinaryOp::Mul => ctx
                        .builder
                        .build_float_mul(l, r, "fmul")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    BinaryOp::FloorDiv => ctx
                        .builder
                        .build_float_div(l, r, "fdiv")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    BinaryOp::Mod => ctx
                        .builder
                        .build_float_rem(l, r, "fmod")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                    _ => unreachable!(),
                };
                Ok(res.into())
            }
        }
        BinaryOp::Div => {
            // True division always returns f64
            if left_ty == ValType::Int {
                let l = ctx
                    .builder
                    .build_signed_int_to_float(
                        lhs.into_int_value(),
                        ctx.context.f64_type(),
                        "l2f",
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                let r = ctx
                    .builder
                    .build_signed_int_to_float(
                        rhs.into_int_value(),
                        ctx.context.f64_type(),
                        "r2f",
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                let res = ctx
                    .builder
                    .build_float_div(l, r, "div")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                Ok(res.into())
            } else {
                let l = lhs.into_float_value();
                let r = rhs.into_float_value();
                let res = ctx
                    .builder
                    .build_float_div(l, r, "fdiv")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                Ok(res.into())
            }
        }
        BinaryOp::Pow => {
            // For integer pow, use repeated multiplication via llvm.powi or cast to float
            // Simplification: always use float pow
            let _result_ty = result_ty;
            if left_ty == ValType::Int {
                let l = ctx
                    .builder
                    .build_signed_int_to_float(
                        lhs.into_int_value(),
                        ctx.context.f64_type(),
                        "l2f",
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                let r = ctx
                    .builder
                    .build_signed_int_to_float(
                        rhs.into_int_value(),
                        ctx.context.f64_type(),
                        "r2f",
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                let pow_fn = declare_pow(ctx);
                let res = ctx
                    .builder
                    .build_call(pow_fn, &[l.into(), r.into()], "pow")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .try_as_basic_value()
                    .unwrap_basic();
                // Convert back to int if result type is Int
                if *result_ty == TypeAnnotation::Int {
                    let int_val = ctx
                        .builder
                        .build_float_to_signed_int(
                            res.into_float_value(),
                            ctx.context.i64_type(),
                            "f2i",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                    Ok(int_val.into())
                } else {
                    Ok(res)
                }
            } else {
                let l = lhs.into_float_value();
                let r = rhs.into_float_value();
                let pow_fn = declare_pow(ctx);
                let res = ctx
                    .builder
                    .build_call(pow_fn, &[l.into(), r.into()], "pow")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .try_as_basic_value()
                    .unwrap_basic();
                Ok(res)
            }
        }
        BinaryOp::Eq | BinaryOp::NotEq | BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt
        | BinaryOp::GtEq => {
            if left_ty == ValType::Int {
                let l = lhs.into_int_value();
                let r = rhs.into_int_value();
                let pred = match op {
                    BinaryOp::Eq => IntPredicate::EQ,
                    BinaryOp::NotEq => IntPredicate::NE,
                    BinaryOp::Lt => IntPredicate::SLT,
                    BinaryOp::LtEq => IntPredicate::SLE,
                    BinaryOp::Gt => IntPredicate::SGT,
                    BinaryOp::GtEq => IntPredicate::SGE,
                    _ => unreachable!(),
                };
                let cmp = ctx
                    .builder
                    .build_int_compare(pred, l, r, "cmp")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                Ok(cmp.into())
            } else {
                let l = lhs.into_float_value();
                let r = rhs.into_float_value();
                let pred = match op {
                    BinaryOp::Eq => FloatPredicate::OEQ,
                    BinaryOp::NotEq => FloatPredicate::ONE,
                    BinaryOp::Lt => FloatPredicate::OLT,
                    BinaryOp::LtEq => FloatPredicate::OLE,
                    BinaryOp::Gt => FloatPredicate::OGT,
                    BinaryOp::GtEq => FloatPredicate::OGE,
                    _ => unreachable!(),
                };
                let cmp = ctx
                    .builder
                    .build_float_compare(pred, l, r, "fcmp")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                Ok(cmp.into())
            }
        }
        BinaryOp::And | BinaryOp::Or => {
            // Simple bitwise and/or on booleans (non-short-circuit for now)
            let l = lhs.into_int_value();
            let r = rhs.into_int_value();
            let res = match op {
                BinaryOp::And => ctx
                    .builder
                    .build_and(l, r, "and")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                BinaryOp::Or => ctx
                    .builder
                    .build_or(l, r, "or")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                _ => unreachable!(),
            };
            Ok(res.into())
        }
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
            let l = lhs.into_int_value();
            let r = rhs.into_int_value();
            let res = match op {
                BinaryOp::BitAnd => ctx
                    .builder
                    .build_and(l, r, "bitand")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                BinaryOp::BitOr => ctx
                    .builder
                    .build_or(l, r, "bitor")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                BinaryOp::BitXor => ctx
                    .builder
                    .build_xor(l, r, "bitxor")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?,
                _ => unreachable!(),
            };
            Ok(res.into())
        }
        BinaryOp::LShift => {
            let l = lhs.into_int_value();
            let r = rhs.into_int_value();
            let res = ctx
                .builder
                .build_left_shift(l, r, "lshift")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(res.into())
        }
        BinaryOp::RShift => {
            let l = lhs.into_int_value();
            let r = rhs.into_int_value();
            let res = ctx
                .builder
                .build_right_shift(l, r, true, "rshift")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(res.into())
        }
    }
}

fn emit_unary_op<'ctx>(
    ctx: &CodegenContext<'ctx>,
    op: &UnaryOp,
    operand: &Expression,
    _ty: &TypeAnnotation,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let val = emit_expression(ctx, operand)?;
    match op {
        UnaryOp::Neg => {
            if val.is_int_value() {
                let res = ctx
                    .builder
                    .build_int_neg(val.into_int_value(), "neg")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                Ok(res.into())
            } else {
                let res = ctx
                    .builder
                    .build_float_neg(val.into_float_value(), "fneg")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                Ok(res.into())
            }
        }
        UnaryOp::Pos => Ok(val),
        UnaryOp::Not => {
            let bool_val = val.into_int_value();
            let res = ctx
                .builder
                .build_not(bool_val, "not")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(res.into())
        }
        UnaryOp::BitNot => {
            let int_val = val.into_int_value();
            let res = ctx
                .builder
                .build_not(int_val, "bitnot")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            Ok(res.into())
        }
    }
}

fn emit_call<'ctx>(
    ctx: &CodegenContext<'ctx>,
    func: &Expression,
    args: &[Expression],
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let func_name = match func {
        Expression::Name { id, .. } => id.as_str(),
        _ => {
            return Err(CodegenError::UnsupportedExpression(
                "only named function calls supported".into(),
            ))
        }
    };

    // Skip built-in functions like range() -- they are handled in for-loop emission
    let fn_val = ctx
        .module
        .get_function(func_name)
        .ok_or_else(|| CodegenError::UndefinedFunction(func_name.to_string()))?;

    let mut llvm_args: Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> = Vec::new();
    for arg in args {
        let val = emit_expression(ctx, arg)?;
        llvm_args.push(val.into());
    }

    let call = ctx
        .builder
        .build_call(fn_val, &llvm_args, "call")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    Ok(call.try_as_basic_value().unwrap_basic())
}

/// Convert a value to an i1 boolean for conditional branches.
pub fn ensure_bool<'ctx>(
    ctx: &CodegenContext<'ctx>,
    val: BasicValueEnum<'ctx>,
) -> Result<inkwell::values::IntValue<'ctx>, CodegenError> {
    if val.is_int_value() {
        let int_val = val.into_int_value();
        let bit_width = int_val.get_type().get_bit_width();
        if bit_width == 1 {
            return Ok(int_val);
        }
        // Non-zero check for wider integers
        let zero = int_val.get_type().const_int(0, false);
        let cmp = ctx
            .builder
            .build_int_compare(IntPredicate::NE, int_val, zero, "tobool")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(cmp)
    } else if val.is_float_value() {
        let fval = val.into_float_value();
        let zero = fval.get_type().const_float(0.0);
        let cmp = ctx
            .builder
            .build_float_compare(FloatPredicate::ONE, fval, zero, "ftobool")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(cmp)
    } else {
        Err(CodegenError::UnsupportedType(
            "cannot convert to bool".into(),
        ))
    }
}

fn declare_pow<'ctx>(ctx: &CodegenContext<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
    if let Some(f) = ctx.module.get_function("pow") {
        return f;
    }
    let f64_ty = ctx.context.f64_type();
    let fn_ty = f64_ty.fn_type(&[f64_ty.into(), f64_ty.into()], false);
    ctx.module.add_function("pow", fn_ty, None)
}

#[derive(PartialEq)]
enum ValType {
    Int,
    Float,
}

fn left_type_from_value(val: BasicValueEnum<'_>) -> ValType {
    if val.is_int_value() {
        ValType::Int
    } else {
        ValType::Float
    }
}

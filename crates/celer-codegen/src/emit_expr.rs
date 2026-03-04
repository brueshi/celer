use std::sync::atomic::{AtomicU32, Ordering};

use inkwell::values::BasicValueEnum;
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use celer_hir::{BinaryOp, Expression, TypeAnnotation, UnaryOp};

use crate::context::CodegenContext;
use crate::error::CodegenError;

/// Global counter for unique buffer names across all codegen invocations.
static BUFFER_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Size of scratch buffers used by str() and string concat.
const STR_BUF_SIZE: u64 = 256;

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
        Expression::List { elements, ty } => {
            crate::emit_collection::emit_list(ctx, elements, ty)
        }
        Expression::Tuple { elements, ty } => {
            crate::emit_collection::emit_tuple(ctx, elements, ty)
        }
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
            // String concatenation: pointer + pointer with Add
            if left_ty == ValType::Ptr && matches!(op, BinaryOp::Add) {
                return emit_string_concat(ctx, lhs, rhs);
            }
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
            // String comparison via strcmp
            if left_ty == ValType::Ptr
                && matches!(op, BinaryOp::Eq | BinaryOp::NotEq)
            {
                return emit_string_compare(ctx, op, lhs, rhs);
            }
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

/// Builtin function names handled directly in codegen.
const BUILTINS: &[&str] = &["len", "str", "int", "float", "bool"];

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

    // Check builtins before module lookup
    if BUILTINS.contains(&func_name) {
        return emit_builtin_call(ctx, func_name, args);
    }

    // Fall through to module-defined functions
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

/// Emit code for Python builtin functions: len, str, int, float, bool.
fn emit_builtin_call<'ctx>(
    ctx: &CodegenContext<'ctx>,
    name: &str,
    args: &[Expression],
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    if args.len() != 1 {
        return Err(CodegenError::UnsupportedExpression(format!(
            "builtin {name}() expects exactly 1 argument, got {}",
            args.len()
        )));
    }

    // len() needs the original expression for list/tuple type dispatch
    if name == "len" {
        return emit_builtin_len_dispatch(ctx, &args[0]);
    }

    let arg = emit_expression(ctx, &args[0])?;

    match name {
        "len" => unreachable!(),
        "str" => emit_builtin_str(ctx, arg),
        "int" => emit_builtin_int(ctx, arg),
        "float" => emit_builtin_float(ctx, arg),
        "bool" => emit_builtin_bool(ctx, arg),
        _ => Err(CodegenError::UndefinedFunction(name.to_string())),
    }
}

/// Dispatch len() based on the argument type: string, list, or tuple.
fn emit_builtin_len_dispatch<'ctx>(
    ctx: &CodegenContext<'ctx>,
    arg_expr: &Expression,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let arg_ty = arg_expr.ty();
    match arg_ty {
        TypeAnnotation::List(_) => crate::emit_collection::emit_list_len(ctx, arg_expr),
        TypeAnnotation::Tuple(elems) => crate::emit_collection::emit_tuple_len(ctx, elems.len()),
        _ => {
            let arg = emit_expression(ctx, arg_expr)?;
            emit_builtin_len(ctx, arg)
        }
    }
}

/// `len(s)` where s is a string pointer -> calls strlen, returns i64.
fn emit_builtin_len<'ctx>(
    ctx: &CodegenContext<'ctx>,
    arg: BasicValueEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let strlen_fn = declare_strlen(ctx);
    let result = ctx
        .builder
        .build_call(strlen_fn, &[arg.into()], "strlen")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        .try_as_basic_value()
        .unwrap_basic();
    // strlen returns i64 (size_t), which matches our Int type
    Ok(result)
}

/// `str(n)` where n is an integer -> snprintf into a global buffer, returns pointer.
fn emit_builtin_str<'ctx>(
    ctx: &CodegenContext<'ctx>,
    arg: BasicValueEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let snprintf_fn = declare_snprintf(ctx);
    let i64_ty = ctx.context.i64_type();

    // Allocate a unique global buffer
    let buf_id = BUFFER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let buf_name = format!("str_buf_{buf_id}");
    let buf_ty = ctx.context.i8_type().array_type(STR_BUF_SIZE as u32);
    let buf_global = ctx.module.add_global(buf_ty, None, &buf_name);
    buf_global.set_linkage(inkwell::module::Linkage::Private);
    buf_global.set_initializer(&buf_ty.const_zero());
    let buf = buf_global.as_pointer_value();

    // Format string: "%lld" for i64
    let fmt_global = ctx
        .builder
        .build_global_string_ptr("%lld", "str_fmt")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    let buf_size = i64_ty.const_int(STR_BUF_SIZE, false);

    // If arg is float, use "%g" format
    if arg.is_float_value() {
        let fmt_f = ctx
            .builder
            .build_global_string_ptr("%g", "str_fmt_f")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        ctx.builder
            .build_call(
                snprintf_fn,
                &[
                    buf.into(),
                    buf_size.into(),
                    fmt_f.as_pointer_value().into(),
                    arg.into(),
                ],
                "snprintf",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    } else if arg.is_pointer_value() {
        // str(s) where s is already a string, return as-is
        return Ok(arg);
    } else {
        // Integer path
        ctx.builder
            .build_call(
                snprintf_fn,
                &[
                    buf.into(),
                    buf_size.into(),
                    fmt_global.as_pointer_value().into(),
                    arg.into(),
                ],
                "snprintf",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    }

    // Return pointer to buffer (acts as a C string)
    Ok(buf.into())
}

/// `int(s)` where s is a string -> calls strtol, returns i64.
fn emit_builtin_int<'ctx>(
    ctx: &CodegenContext<'ctx>,
    arg: BasicValueEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    if arg.is_int_value() {
        return Ok(arg);
    }
    if arg.is_float_value() {
        let int_val = ctx
            .builder
            .build_float_to_signed_int(
                arg.into_float_value(),
                ctx.context.i64_type(),
                "f2i",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        return Ok(int_val.into());
    }
    // String to int via strtol
    let strtol_fn = declare_strtol(ctx);
    let null_ptr = ctx
        .context
        .ptr_type(AddressSpace::default())
        .const_null();
    let base = ctx.context.i32_type().const_int(10, false);

    let result = ctx
        .builder
        .build_call(strtol_fn, &[arg.into(), null_ptr.into(), base.into()], "strtol")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        .try_as_basic_value()
        .unwrap_basic();
    Ok(result)
}

/// `float(s)` where s is a string -> calls strtod, returns f64.
fn emit_builtin_float<'ctx>(
    ctx: &CodegenContext<'ctx>,
    arg: BasicValueEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    if arg.is_float_value() {
        return Ok(arg);
    }
    if arg.is_int_value() {
        let fval = ctx
            .builder
            .build_signed_int_to_float(
                arg.into_int_value(),
                ctx.context.f64_type(),
                "i2f",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        return Ok(fval.into());
    }
    // String to float via strtod
    let strtod_fn = declare_strtod(ctx);
    let null_ptr = ctx
        .context
        .ptr_type(AddressSpace::default())
        .const_null();

    let result = ctx
        .builder
        .build_call(strtod_fn, &[arg.into(), null_ptr.into()], "strtod")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        .try_as_basic_value()
        .unwrap_basic();
    Ok(result)
}

/// `bool(x)` -> convert to i1 using ensure_bool logic.
fn emit_builtin_bool<'ctx>(
    ctx: &CodegenContext<'ctx>,
    arg: BasicValueEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    if arg.is_pointer_value() {
        // For strings: bool("") is False, bool("x") is True
        // Check strlen != 0
        let strlen_fn = declare_strlen(ctx);
        let len = ctx
            .builder
            .build_call(strlen_fn, &[arg.into()], "strlen")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .try_as_basic_value()
            .unwrap_basic();
        let zero = ctx.context.i64_type().const_int(0, false);
        let cmp = ctx
            .builder
            .build_int_compare(IntPredicate::NE, len.into_int_value(), zero, "strbool")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        return Ok(cmp.into());
    }
    let bool_val = ensure_bool(ctx, arg)?;
    Ok(bool_val.into())
}

/// Emit strcmp-based string comparison for Eq / NotEq.
fn emit_string_compare<'ctx>(
    ctx: &CodegenContext<'ctx>,
    op: &BinaryOp,
    lhs: BasicValueEnum<'ctx>,
    rhs: BasicValueEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let strcmp_fn = declare_strcmp(ctx);
    let result = ctx
        .builder
        .build_call(strcmp_fn, &[lhs.into(), rhs.into()], "strcmp")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        .try_as_basic_value()
        .unwrap_basic();

    let zero = ctx.context.i32_type().const_int(0, false);
    let pred = match op {
        BinaryOp::Eq => IntPredicate::EQ,
        BinaryOp::NotEq => IntPredicate::NE,
        _ => unreachable!(),
    };
    let cmp = ctx
        .builder
        .build_int_compare(pred, result.into_int_value(), zero, "strcmp_cmp")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    Ok(cmp.into())
}

/// Emit string concatenation via snprintf("%s%s", left, right).
fn emit_string_concat<'ctx>(
    ctx: &CodegenContext<'ctx>,
    lhs: BasicValueEnum<'ctx>,
    rhs: BasicValueEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let snprintf_fn = declare_snprintf(ctx);
    let i64_ty = ctx.context.i64_type();

    let buf_id = BUFFER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let buf_name = format!("concat_buf_{buf_id}");
    let buf_ty = ctx.context.i8_type().array_type(STR_BUF_SIZE as u32);
    let buf_global = ctx.module.add_global(buf_ty, None, &buf_name);
    buf_global.set_linkage(inkwell::module::Linkage::Private);
    buf_global.set_initializer(&buf_ty.const_zero());
    let buf = buf_global.as_pointer_value();

    let fmt = ctx
        .builder
        .build_global_string_ptr("%s%s", "concat_fmt")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    let buf_size = i64_ty.const_int(STR_BUF_SIZE, false);
    ctx.builder
        .build_call(
            snprintf_fn,
            &[
                buf.into(),
                buf_size.into(),
                fmt.as_pointer_value().into(),
                lhs.into(),
                rhs.into(),
            ],
            "concat",
        )
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    Ok(buf.into())
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
    Ptr,
}

fn left_type_from_value(val: BasicValueEnum<'_>) -> ValType {
    if val.is_int_value() {
        ValType::Int
    } else if val.is_pointer_value() {
        ValType::Ptr
    } else {
        ValType::Float
    }
}

// -- C library function declarations --

fn declare_strlen<'ctx>(ctx: &CodegenContext<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
    if let Some(f) = ctx.module.get_function("strlen") {
        return f;
    }
    let i64_ty = ctx.context.i64_type();
    let ptr_ty = ctx.context.ptr_type(AddressSpace::default());
    let fn_ty = i64_ty.fn_type(&[ptr_ty.into()], false);
    ctx.module.add_function("strlen", fn_ty, None)
}

fn declare_strcmp<'ctx>(ctx: &CodegenContext<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
    if let Some(f) = ctx.module.get_function("strcmp") {
        return f;
    }
    let i32_ty = ctx.context.i32_type();
    let ptr_ty = ctx.context.ptr_type(AddressSpace::default());
    let fn_ty = i32_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
    ctx.module.add_function("strcmp", fn_ty, None)
}

fn declare_snprintf<'ctx>(ctx: &CodegenContext<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
    if let Some(f) = ctx.module.get_function("snprintf") {
        return f;
    }
    let i32_ty = ctx.context.i32_type();
    let i64_ty = ctx.context.i64_type();
    let ptr_ty = ctx.context.ptr_type(AddressSpace::default());
    let fn_ty = i32_ty.fn_type(&[ptr_ty.into(), i64_ty.into(), ptr_ty.into()], true);
    ctx.module.add_function("snprintf", fn_ty, None)
}

fn declare_strtol<'ctx>(ctx: &CodegenContext<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
    if let Some(f) = ctx.module.get_function("strtol") {
        return f;
    }
    let i64_ty = ctx.context.i64_type();
    let ptr_ty = ctx.context.ptr_type(AddressSpace::default());
    let i32_ty = ctx.context.i32_type();
    let fn_ty = i64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i32_ty.into()], false);
    ctx.module.add_function("strtol", fn_ty, None)
}

fn declare_strtod<'ctx>(ctx: &CodegenContext<'ctx>) -> inkwell::values::FunctionValue<'ctx> {
    if let Some(f) = ctx.module.get_function("strtod") {
        return f;
    }
    let f64_ty = ctx.context.f64_type();
    let ptr_ty = ctx.context.ptr_type(AddressSpace::default());
    let fn_ty = f64_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false);
    ctx.module.add_function("strtod", fn_ty, None)
}

use inkwell::values::FunctionValue;
use inkwell::IntPredicate;

use celer_hir::{BinaryOp, Expression, Statement};

use crate::context::CodegenContext;
use crate::emit_expr::{emit_binary_op_values, emit_expression, ensure_bool};
use crate::error::CodegenError;

/// Emit LLVM IR for a single statement within a function body.
pub fn emit_statement<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    fn_val: FunctionValue<'ctx>,
    stmt: &Statement,
) -> Result<(), CodegenError> {
    match stmt {
        Statement::Return { value: Some(expr) } => {
            let val = emit_expression(ctx, expr)?;
            ctx.builder
                .build_return(Some(&val))
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        Statement::Return { value: None } => {
            ctx.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        Statement::Assign { target, value, .. } => {
            emit_assign(ctx, target, value)?;
        }
        Statement::AugAssign { target, op, value } => {
            emit_aug_assign(ctx, target, op, value)?;
        }
        Statement::If { test, body, orelse } => {
            emit_if(ctx, fn_val, test, body, orelse)?;
        }
        Statement::While { test, body } => {
            emit_while(ctx, fn_val, test, body)?;
        }
        Statement::For { target, iter, body } => {
            emit_for(ctx, fn_val, target, iter, body)?;
        }
        Statement::Break => {
            let loop_ctx = ctx
                .current_loop()
                .ok_or_else(|| CodegenError::LlvmError("break outside loop".into()))?;
            let exit_bb = loop_ctx.exit_bb;
            ctx.builder
                .build_unconditional_branch(exit_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        Statement::Continue => {
            let loop_ctx = ctx
                .current_loop()
                .ok_or_else(|| CodegenError::LlvmError("continue outside loop".into()))?;
            let cond_bb = loop_ctx.cond_bb;
            ctx.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        Statement::Expr(_) => {
            // Side-effect expression; ignore result
        }
        _ => {}
    }
    Ok(())
}

pub fn emit_assign<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    target: &str,
    value: &Expression,
) -> Result<(), CodegenError> {
    let val = emit_expression(ctx, value)?;
    let ptr = match ctx.get_local(target) {
        Some(existing) => existing,
        None => {
            let alloca = ctx
                .builder
                .build_alloca(val.get_type(), target)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.set_local(target, alloca);
            alloca
        }
    };
    ctx.builder
        .build_store(ptr, val)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    Ok(())
}

fn emit_aug_assign<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    target: &str,
    op: &BinaryOp,
    value: &Expression,
) -> Result<(), CodegenError> {
    let ptr = ctx
        .get_local(target)
        .ok_or_else(|| CodegenError::UndefinedVariable(target.to_string()))?;

    let rhs = emit_expression(ctx, value)?;

    // Load the current value with the same type as rhs
    let current = ctx
        .builder
        .build_load(rhs.get_type(), ptr, "cur")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Determine result type annotation for the op
    let result_ty = if rhs.is_int_value() {
        celer_hir::TypeAnnotation::Int
    } else {
        celer_hir::TypeAnnotation::Float
    };

    let result = emit_binary_op_values(ctx, op, current, rhs, &result_ty)?;

    ctx.builder
        .build_store(ptr, result)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    Ok(())
}

fn emit_if<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    fn_val: FunctionValue<'ctx>,
    test: &Expression,
    body: &[Statement],
    orelse: &[Statement],
) -> Result<(), CodegenError> {
    let cond = emit_expression(ctx, test)?;
    let cond_val = ensure_bool(ctx, cond)?;

    let then_bb = ctx.context.append_basic_block(fn_val, "then");
    let else_bb = ctx.context.append_basic_block(fn_val, "else");
    let merge_bb = ctx.context.append_basic_block(fn_val, "ifmerge");

    ctx.builder
        .build_conditional_branch(cond_val, then_bb, else_bb)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Then block
    ctx.builder.position_at_end(then_bb);
    for s in body {
        emit_statement(ctx, fn_val, s)?;
    }
    if ctx
        .builder
        .get_insert_block()
        .unwrap()
        .get_terminator()
        .is_none()
    {
        ctx.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    }

    // Else block
    ctx.builder.position_at_end(else_bb);
    for s in orelse {
        emit_statement(ctx, fn_val, s)?;
    }
    if ctx
        .builder
        .get_insert_block()
        .unwrap()
        .get_terminator()
        .is_none()
    {
        ctx.builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    }

    ctx.builder.position_at_end(merge_bb);
    Ok(())
}

fn emit_while<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    fn_val: FunctionValue<'ctx>,
    test: &Expression,
    body: &[Statement],
) -> Result<(), CodegenError> {
    let cond_bb = ctx.context.append_basic_block(fn_val, "while.cond");
    let body_bb = ctx.context.append_basic_block(fn_val, "while.body");
    let exit_bb = ctx.context.append_basic_block(fn_val, "while.exit");

    ctx.builder
        .build_unconditional_branch(cond_bb)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Condition
    ctx.builder.position_at_end(cond_bb);
    let cond = emit_expression(ctx, test)?;
    let cond_val = ensure_bool(ctx, cond)?;
    ctx.builder
        .build_conditional_branch(cond_val, body_bb, exit_bb)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Body
    ctx.builder.position_at_end(body_bb);
    ctx.push_loop(cond_bb, exit_bb);
    for s in body {
        emit_statement(ctx, fn_val, s)?;
    }
    ctx.pop_loop();
    if ctx
        .builder
        .get_insert_block()
        .unwrap()
        .get_terminator()
        .is_none()
    {
        ctx.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    }

    ctx.builder.position_at_end(exit_bb);
    Ok(())
}

fn emit_for<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    fn_val: FunctionValue<'ctx>,
    target: &str,
    iter: &Expression,
    body: &[Statement],
) -> Result<(), CodegenError> {
    // Extract start, stop, step from range() call
    let (start_val, stop_val, step_val) = match iter {
        Expression::Call { func, args, .. } => {
            if let Expression::Name { id, .. } = func.as_ref() {
                if id == "range" && (1..=3).contains(&args.len()) {
                    match args.len() {
                        1 => {
                            let stop = emit_expression(ctx, &args[0])?;
                            let i64_ty = ctx.context.i64_type();
                            (
                                i64_ty.const_int(0, false).into(),
                                stop,
                                i64_ty.const_int(1, false).into(),
                            )
                        }
                        2 => {
                            let start = emit_expression(ctx, &args[0])?;
                            let stop = emit_expression(ctx, &args[1])?;
                            let i64_ty = ctx.context.i64_type();
                            (start, stop, i64_ty.const_int(1, false).into())
                        }
                        3 => {
                            let start = emit_expression(ctx, &args[0])?;
                            let stop = emit_expression(ctx, &args[1])?;
                            let step = emit_expression(ctx, &args[2])?;
                            (start, stop, step)
                        }
                        _ => unreachable!(),
                    }
                } else {
                    return Err(CodegenError::UnsupportedExpression(
                        "only range() for loops supported".into(),
                    ));
                }
            } else {
                return Err(CodegenError::UnsupportedExpression(
                    "only range() for loops supported".into(),
                ));
            }
        }
        _ => {
            return Err(CodegenError::UnsupportedExpression(
                "only range() for loops supported".into(),
            ))
        }
    };

    let i64_ty = ctx.context.i64_type();
    let counter = ctx
        .builder
        .build_alloca(i64_ty, target)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    ctx.builder
        .build_store(counter, start_val)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    ctx.set_local(target, counter);

    let cond_bb = ctx.context.append_basic_block(fn_val, "for.cond");
    let body_bb = ctx.context.append_basic_block(fn_val, "for.body");
    let exit_bb = ctx.context.append_basic_block(fn_val, "for.exit");

    ctx.builder
        .build_unconditional_branch(cond_bb)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Condition: counter < stop
    ctx.builder.position_at_end(cond_bb);
    let cur = ctx
        .builder
        .build_load(i64_ty, counter, "i")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    let cmp = ctx
        .builder
        .build_int_compare(
            IntPredicate::SLT,
            cur.into_int_value(),
            stop_val.into_int_value(),
            "cmp",
        )
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    ctx.builder
        .build_conditional_branch(cmp, body_bb, exit_bb)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Body
    ctx.builder.position_at_end(body_bb);
    ctx.push_loop(cond_bb, exit_bb);
    for s in body {
        emit_statement(ctx, fn_val, s)?;
    }
    ctx.pop_loop();

    // Increment counter by step
    if ctx
        .builder
        .get_insert_block()
        .unwrap()
        .get_terminator()
        .is_none()
    {
        let cur = ctx
            .builder
            .build_load(i64_ty, counter, "i_inc")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        let next = ctx
            .builder
            .build_int_add(cur.into_int_value(), step_val.into_int_value(), "next")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        ctx.builder
            .build_store(counter, next)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        ctx.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    }

    ctx.builder.position_at_end(exit_bb);
    Ok(())
}

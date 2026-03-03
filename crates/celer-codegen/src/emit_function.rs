use inkwell::AddressSpace;
use inkwell::types::BasicMetadataTypeEnum;
use inkwell::values::FunctionValue;

use celer_hir::{Expression, Function, Statement};

use crate::context::CodegenContext;
use crate::emit_json::{ArgFormat, JsonPlan, plan_dict};
use crate::emit_stmt::emit_statement;
use crate::error::CodegenError;
use crate::types::{is_json_return_type, resolve_type};

const SNPRINTF_BUF_SIZE: u64 = 256;

/// Emit a complete LLVM function from HIR.
pub fn emit_function<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    func: &Function,
) -> Result<FunctionValue<'ctx>, CodegenError> {
    let fn_val = if is_json_return_type(&func.return_type) {
        emit_json_function_decl(ctx, func)?
    } else {
        emit_scalar_function_decl(ctx, func)?
    };

    // Create entry basic block
    let entry = ctx.context.append_basic_block(fn_val, "entry");
    ctx.builder.position_at_end(entry);
    ctx.clear_locals();

    if is_json_return_type(&func.return_type) {
        // Bind user params (skipping the two output pointer params at the end)
        let user_param_count = func.params.len();
        for (i, param) in func.params.iter().enumerate() {
            let llvm_param = fn_val
                .get_nth_param(i as u32)
                .ok_or_else(|| CodegenError::LlvmError(format!("missing param {i}")))?;
            let alloca = ctx
                .builder
                .build_alloca(llvm_param.get_type(), &param.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_store(alloca, llvm_param)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.set_local(&param.name, alloca);
        }

        // Emit body statements up to the return
        let mut return_expr: Option<&Expression> = None;
        for stmt in &func.body {
            match stmt {
                Statement::Return { value: Some(expr) } => {
                    return_expr = Some(expr);
                    break;
                }
                other => {
                    emit_statement(ctx, fn_val, other)?;
                }
            }
        }

        // The return expression must be a Dict for JSON functions
        let dict_expr = return_expr.ok_or_else(|| {
            CodegenError::UnsupportedExpression(
                "JSON function must have a return statement with a dict".to_string(),
            )
        })?;

        match dict_expr {
            Expression::Dict { keys, values, .. } => {
                emit_json_return(ctx, fn_val, user_param_count, keys, values)?;
            }
            _ => {
                return Err(CodegenError::UnsupportedExpression(
                    "JSON function must return a dict literal".to_string(),
                ));
            }
        }
    } else {
        // Scalar function: bind params, emit body
        for (i, param) in func.params.iter().enumerate() {
            let llvm_param = fn_val
                .get_nth_param(i as u32)
                .ok_or_else(|| CodegenError::LlvmError(format!("missing param {i}")))?;
            let alloca = ctx
                .builder
                .build_alloca(llvm_param.get_type(), &param.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_store(alloca, llvm_param)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.set_local(&param.name, alloca);
        }

        for stmt in &func.body {
            emit_statement(ctx, fn_val, stmt)?;
        }

        // If no terminator yet, add a default return or unreachable
        let current_bb = ctx.builder.get_insert_block().unwrap();
        if current_bb.get_terminator().is_none() {
            let ret_type = resolve_type(ctx.context, &func.return_type)?;
            match ret_type {
                Some(ty) => {
                    use inkwell::values::BasicValueEnum;
                    let default_val: BasicValueEnum<'ctx> = match ty {
                        inkwell::types::BasicTypeEnum::IntType(t) => t.const_zero().into(),
                        inkwell::types::BasicTypeEnum::FloatType(t) => t.const_zero().into(),
                        _ => {
                            ctx.builder
                                .build_unreachable()
                                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                            return Ok(fn_val);
                        }
                    };
                    ctx.builder
                        .build_return(Some(&default_val))
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                }
                None => {
                    ctx.builder
                        .build_return(None)
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                }
            }
        }
    }

    Ok(fn_val)
}

fn emit_scalar_function_decl<'ctx>(
    ctx: &CodegenContext<'ctx>,
    func: &Function,
) -> Result<FunctionValue<'ctx>, CodegenError> {
    let ret_type = resolve_type(ctx.context, &func.return_type)?;
    let mut param_types: Vec<BasicMetadataTypeEnum<'ctx>> = Vec::new();
    for p in &func.params {
        let ty = resolve_type(ctx.context, &p.annotation)?.ok_or_else(|| {
            CodegenError::UnsupportedType(format!("void param type for {}", p.name))
        })?;
        param_types.push(ty.into());
    }

    let fn_type = match ret_type {
        Some(inkwell::types::BasicTypeEnum::IntType(t)) => t.fn_type(&param_types, false),
        Some(inkwell::types::BasicTypeEnum::FloatType(t)) => t.fn_type(&param_types, false),
        None => ctx.context.void_type().fn_type(&param_types, false),
        _ => {
            return Err(CodegenError::UnsupportedType(format!(
                "{:?}",
                func.return_type
            )));
        }
    };

    let fn_val = ctx.module.add_function(&func.name, fn_type, None);
    Ok(fn_val)
}

fn emit_json_function_decl<'ctx>(
    ctx: &CodegenContext<'ctx>,
    func: &Function,
) -> Result<FunctionValue<'ctx>, CodegenError> {
    let ptr_ty = ctx.context.ptr_type(AddressSpace::default());
    let mut param_types: Vec<BasicMetadataTypeEnum<'ctx>> = Vec::new();

    // User-defined params first
    for p in &func.params {
        let ty = resolve_type(ctx.context, &p.annotation)?.ok_or_else(|| {
            CodegenError::UnsupportedType(format!("void param type for {}", p.name))
        })?;
        param_types.push(ty.into());
    }

    // Output params: out_ptr (*mut *const u8) and out_len (*mut u64)
    param_types.push(ptr_ty.into()); // out_ptr
    param_types.push(ptr_ty.into()); // out_len

    let fn_type = ctx.context.void_type().fn_type(&param_types, false);
    let fn_val = ctx.module.add_function(&func.name, fn_type, None);
    Ok(fn_val)
}

fn emit_json_return<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    fn_val: FunctionValue<'ctx>,
    user_param_count: usize,
    keys: &[Expression],
    values: &[Expression],
) -> Result<(), CodegenError> {
    let plan = plan_dict(keys, values)?;
    let out_ptr_param = fn_val
        .get_nth_param(user_param_count as u32)
        .ok_or_else(|| CodegenError::LlvmError("missing out_ptr param".into()))?
        .into_pointer_value();
    let out_len_param = fn_val
        .get_nth_param((user_param_count + 1) as u32)
        .ok_or_else(|| CodegenError::LlvmError("missing out_len param".into()))?
        .into_pointer_value();

    match plan {
        JsonPlan::Static(json_str) => {
            let json_len = json_str.len() as u64;
            let global = ctx.add_string_constant(
                &json_str,
                &format!("json_{}", fn_val.get_name().to_str().unwrap_or("anon")),
            );

            ctx.builder
                .build_store(out_ptr_param, global)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_store(
                    out_len_param,
                    ctx.context.i64_type().const_int(json_len, false),
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            ctx.builder
                .build_return(None)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        }
        JsonPlan::Dynamic { format, args } => {
            emit_snprintf_json(ctx, fn_val, user_param_count, &format, &args)?;
        }
    }

    Ok(())
}

fn emit_snprintf_json<'ctx>(
    ctx: &mut CodegenContext<'ctx>,
    fn_val: FunctionValue<'ctx>,
    user_param_count: usize,
    format: &str,
    args: &[crate::emit_json::DynArg],
) -> Result<(), CodegenError> {
    let ptr_ty = ctx.context.ptr_type(AddressSpace::default());
    let i32_ty = ctx.context.i32_type();
    let i64_ty = ctx.context.i64_type();

    // Declare snprintf if not already declared
    let snprintf_fn = match ctx.module.get_function("snprintf") {
        Some(f) => f,
        None => {
            let snprintf_ty = i32_ty.fn_type(
                &[ptr_ty.into(), i64_ty.into(), ptr_ty.into()],
                true, // variadic
            );
            ctx.module.add_function("snprintf", snprintf_ty, None)
        }
    };

    // Global buffer for output (non-reentrant, but avoids returning stack pointer)
    let buf_ty = ctx.context.i8_type().array_type(SNPRINTF_BUF_SIZE as u32);
    let buf_name = format!("buf_{}", fn_val.get_name().to_str().unwrap_or("anon"));
    let buf_global = ctx.module.add_global(buf_ty, None, &buf_name);
    buf_global.set_linkage(inkwell::module::Linkage::Private);
    buf_global.set_initializer(&buf_ty.const_zero());
    let buf = buf_global.as_pointer_value();

    // Format string global
    let fmt_name = format!("fmt_{}", fn_val.get_name().to_str().unwrap_or("anon"));
    let fmt_global = ctx.add_string_constant(format, &fmt_name);

    // Build snprintf call arguments
    let buf_size = i64_ty.const_int(SNPRINTF_BUF_SIZE, false);
    let mut call_args: Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> =
        vec![buf.into(), buf_size.into(), fmt_global.into()];

    // Add dynamic arguments
    for arg in args {
        let local_ptr = ctx
            .get_local(&arg.name)
            .ok_or_else(|| CodegenError::UndefinedVariable(arg.name.clone()))?;
        match arg.fmt {
            ArgFormat::I64 => {
                let val = ctx
                    .builder
                    .build_load(i64_ty, local_ptr, &arg.name)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                call_args.push(val.into());
            }
            ArgFormat::F64 => {
                let val = ctx
                    .builder
                    .build_load(ctx.context.f64_type(), local_ptr, &arg.name)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                call_args.push(val.into());
            }
            ArgFormat::Str => {
                let val = ctx
                    .builder
                    .build_load(ptr_ty, local_ptr, &arg.name)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                call_args.push(val.into());
            }
            ArgFormat::Bool => {
                // Convert bool to "true"/"false" string
                let val = ctx
                    .builder
                    .build_load(ctx.context.bool_type(), local_ptr, &arg.name)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                call_args.push(val.into());
            }
        }
    }

    let len_i32 = ctx
        .builder
        .build_call(snprintf_fn, &call_args, "len")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        .try_as_basic_value()
        .unwrap_basic();

    let len_i64 = ctx
        .builder
        .build_int_s_extend(len_i32.into_int_value(), i64_ty, "len64")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Store results to output params
    let out_ptr_param = fn_val
        .get_nth_param(user_param_count as u32)
        .unwrap()
        .into_pointer_value();
    let out_len_param = fn_val
        .get_nth_param((user_param_count + 1) as u32)
        .unwrap()
        .into_pointer_value();

    ctx.builder
        .build_store(out_ptr_param, buf)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    ctx.builder
        .build_store(out_len_param, len_i64)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    ctx.builder
        .build_return(None)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    Ok(())
}


use inkwell::types::BasicType;
use inkwell::values::BasicValueEnum;
use inkwell::IntPredicate;

use celer_hir::{Expression, TypeAnnotation};

use crate::context::CodegenContext;
use crate::emit_expr::emit_expression;
use crate::error::CodegenError;
use crate::types::resolve_type;

/// Emit a list literal as a stack-allocated struct: { i64 length, [N x elem_type] data }.
/// Returns a pointer to the struct.
pub fn emit_list<'ctx>(
    ctx: &CodegenContext<'ctx>,
    elements: &[Expression],
    ty: &TypeAnnotation,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let inner_ty = match ty {
        TypeAnnotation::List(inner) => inner.as_ref(),
        _ => {
            return Err(CodegenError::UnsupportedType(
                "emit_list called with non-list type".into(),
            ))
        }
    };

    let elem_llvm_ty = resolve_type(ctx.context, inner_ty)?.ok_or_else(|| {
        CodegenError::UnsupportedType(format!("cannot resolve list element type: {inner_ty:?}"))
    })?;

    let i64_ty = ctx.context.i64_type();
    let count = elements.len() as u32;
    let array_ty = elem_llvm_ty.array_type(count);

    // Struct: { i64 length, [N x element_type] data }
    let struct_ty = ctx
        .context
        .struct_type(&[i64_ty.into(), array_ty.into()], false);

    let alloca = ctx
        .builder
        .build_alloca(struct_ty, "list")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Store length field at index 0
    let len_ptr = ctx
        .builder
        .build_struct_gep(struct_ty, alloca, 0, "list.len.ptr")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    ctx.builder
        .build_store(len_ptr, i64_ty.const_int(count as u64, false))
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Store each element into the data array at index 1
    let data_ptr = ctx
        .builder
        .build_struct_gep(struct_ty, alloca, 1, "list.data.ptr")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    for (i, elem) in elements.iter().enumerate() {
        let val = emit_expression(ctx, elem)?;
        let elem_ptr = unsafe {
            ctx.builder
                .build_in_bounds_gep(
                    array_ty,
                    data_ptr,
                    &[
                        i64_ty.const_int(0, false),
                        i64_ty.const_int(i as u64, false),
                    ],
                    &format!("list.elem.{i}"),
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        ctx.builder
            .build_store(elem_ptr, val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    }

    Ok(alloca.into())
}

/// Emit a tuple literal as a stack-allocated struct with heterogeneous fields.
/// Returns a pointer to the struct.
pub fn emit_tuple<'ctx>(
    ctx: &CodegenContext<'ctx>,
    elements: &[Expression],
    ty: &TypeAnnotation,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let elem_types = match ty {
        TypeAnnotation::Tuple(types) => types,
        _ => {
            return Err(CodegenError::UnsupportedType(
                "emit_tuple called with non-tuple type".into(),
            ))
        }
    };

    let mut llvm_field_types = Vec::with_capacity(elem_types.len());
    for et in elem_types {
        let llvm_ty = resolve_type(ctx.context, et)?.ok_or_else(|| {
            CodegenError::UnsupportedType(format!("cannot resolve tuple element type: {et:?}"))
        })?;
        llvm_field_types.push(llvm_ty);
    }

    let field_refs: Vec<_> = llvm_field_types.iter().map(|t| (*t).into()).collect();
    let struct_ty = ctx.context.struct_type(&field_refs, false);

    let alloca = ctx
        .builder
        .build_alloca(struct_ty, "tuple")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    for (i, elem) in elements.iter().enumerate() {
        let val = emit_expression(ctx, elem)?;
        let field_ptr = ctx
            .builder
            .build_struct_gep(struct_ty, alloca, i as u32, &format!("tuple.{i}"))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        ctx.builder
            .build_store(field_ptr, val)
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    }

    Ok(alloca.into())
}

/// Emit a subscript (index) operation on a list: list[i].
/// Performs bounds checking and returns the element value.
pub fn emit_list_subscript<'ctx>(
    ctx: &CodegenContext<'ctx>,
    value: &Expression,
    index: &Expression,
    list_inner_ty: &TypeAnnotation,
    fn_val: inkwell::values::FunctionValue<'ctx>,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let list_ptr = emit_expression(ctx, value)?;
    let idx_val = emit_expression(ctx, index)?;

    let elem_llvm_ty = resolve_type(ctx.context, list_inner_ty)?.ok_or_else(|| {
        CodegenError::UnsupportedType(format!(
            "cannot resolve list element type: {list_inner_ty:?}"
        ))
    })?;

    let i64_ty = ctx.context.i64_type();
    let list_ptr_val = list_ptr.into_pointer_value();

    // We need to reconstruct the struct type to compute GEP.
    // The list type annotation tells us the element count isn't known statically
    // from the type alone, but for list literals the struct was built with a known size.
    // We load the length from field 0 for bounds checking.

    // Load the length (field 0 of the struct is always i64)
    let len_ptr = ctx
        .builder
        .build_struct_gep(
            ctx.context
                .struct_type(&[i64_ty.into(), i64_ty.into()], false),
            list_ptr_val,
            0,
            "list.len.ptr",
        )
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
    let length = ctx
        .builder
        .build_load(i64_ty, len_ptr, "list.len")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Bounds check: index < length
    let idx_int = idx_val.into_int_value();
    let in_bounds = ctx
        .builder
        .build_int_compare(IntPredicate::SLT, idx_int, length.into_int_value(), "bounds")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    let ok_bb = ctx
        .context
        .append_basic_block(fn_val, "subscript.ok");
    let fail_bb = ctx
        .context
        .append_basic_block(fn_val, "subscript.oob");

    ctx.builder
        .build_conditional_branch(in_bounds, ok_bb, fail_bb)
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // Out-of-bounds: trap (unreachable instruction)
    ctx.builder.position_at_end(fail_bb);
    ctx.builder
        .build_unreachable()
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    // In-bounds: compute element address
    ctx.builder.position_at_end(ok_bb);

    // The data starts right after the i64 length field.
    // Compute a byte-level GEP past the length field, then index into the data.
    // Field 0 is i64 (length), field 1 is the array. We use a minimal struct
    // with just the length to get the data pointer offset.
    let i8_ty = ctx.context.i8_type();
    let data_byte_offset = i64_ty.size_of();
    let base_i8_ptr = unsafe {
        ctx.builder
            .build_in_bounds_gep(i8_ty, list_ptr_val, &[data_byte_offset], "data.base")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
    };

    let elem_ptr = unsafe {
        ctx.builder
            .build_in_bounds_gep(
                elem_llvm_ty,
                base_i8_ptr,
                &[idx_int],
                "list.elem",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
    };

    let val = ctx
        .builder
        .build_load(elem_llvm_ty, elem_ptr, "list.val")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    Ok(val)
}

/// Emit a subscript operation on a tuple: tuple[i].
/// The index must be a compile-time integer literal for tuples.
pub fn emit_tuple_subscript<'ctx>(
    ctx: &CodegenContext<'ctx>,
    value: &Expression,
    index: &Expression,
    elem_types: &[TypeAnnotation],
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let tuple_ptr = emit_expression(ctx, value)?;

    let idx = match index {
        Expression::IntLiteral(n) => *n as usize,
        _ => {
            return Err(CodegenError::UnsupportedExpression(
                "tuple subscript requires a constant integer index".into(),
            ))
        }
    };

    if idx >= elem_types.len() {
        return Err(CodegenError::LlvmError(format!(
            "tuple index {idx} out of range for tuple with {} elements",
            elem_types.len()
        )));
    }

    let mut llvm_field_types = Vec::with_capacity(elem_types.len());
    for et in elem_types {
        let llvm_ty = resolve_type(ctx.context, et)?.ok_or_else(|| {
            CodegenError::UnsupportedType(format!("cannot resolve tuple element type: {et:?}"))
        })?;
        llvm_field_types.push(llvm_ty);
    }

    let field_refs: Vec<_> = llvm_field_types.iter().map(|t| (*t).into()).collect();
    let struct_ty = ctx.context.struct_type(&field_refs, false);

    let field_ptr = ctx
        .builder
        .build_struct_gep(
            struct_ty,
            tuple_ptr.into_pointer_value(),
            idx as u32,
            &format!("tuple.{idx}"),
        )
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    let val = ctx
        .builder
        .build_load(llvm_field_types[idx], field_ptr, "tuple.val")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    Ok(val)
}

/// Extract the length of a list struct. Used by builtin len() on lists.
pub fn emit_list_len<'ctx>(
    ctx: &CodegenContext<'ctx>,
    list_expr: &Expression,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let list_ptr = emit_expression(ctx, list_expr)?;
    let i64_ty = ctx.context.i64_type();

    // Field 0 of any list struct is the i64 length
    let min_struct_ty = ctx
        .context
        .struct_type(&[i64_ty.into(), i64_ty.into()], false);
    let len_ptr = ctx
        .builder
        .build_struct_gep(
            min_struct_ty,
            list_ptr.into_pointer_value(),
            0,
            "list.len.ptr",
        )
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    let length = ctx
        .builder
        .build_load(i64_ty, len_ptr, "list.len")
        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

    Ok(length)
}

/// Return the length of a tuple as a compile-time constant.
pub fn emit_tuple_len<'ctx>(
    ctx: &CodegenContext<'ctx>,
    elem_count: usize,
) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    let i64_ty = ctx.context.i64_type();
    Ok(i64_ty.const_int(elem_count as u64, false).into())
}

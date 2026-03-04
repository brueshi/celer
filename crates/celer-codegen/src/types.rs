use celer_hir::TypeAnnotation;
use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;

use crate::error::CodegenError;

/// Resolve a HIR TypeAnnotation to an LLVM basic type.
/// Returns `None` for void/None types.
pub fn resolve_type<'ctx>(
    context: &'ctx Context,
    ty: &TypeAnnotation,
) -> Result<Option<BasicTypeEnum<'ctx>>, CodegenError> {
    match ty {
        TypeAnnotation::Int => Ok(Some(context.i64_type().into())),
        TypeAnnotation::Float => Ok(Some(context.f64_type().into())),
        TypeAnnotation::Bool => Ok(Some(context.bool_type().into())),
        TypeAnnotation::Str => Ok(Some(context.ptr_type(AddressSpace::default()).into())),
        TypeAnnotation::None => Ok(None),
        TypeAnnotation::Unknown => Err(CodegenError::UnresolvedType),
        // Dict returns are handled via output parameters, not as a return type
        TypeAnnotation::Dict(_, _) => Ok(None),
        // Lists and tuples are represented as stack-allocated structs passed by pointer
        TypeAnnotation::List(_) => Ok(Some(context.ptr_type(AddressSpace::default()).into())),
        TypeAnnotation::Tuple(_) => Ok(Some(context.ptr_type(AddressSpace::default()).into())),
        _ => Err(CodegenError::UnsupportedType(format!("{ty:?}"))),
    }
}

/// Check whether a return type indicates JSON-style output parameter convention.
pub fn is_json_return_type(ty: &TypeAnnotation) -> bool {
    matches!(ty, TypeAnnotation::Dict(_, _))
}

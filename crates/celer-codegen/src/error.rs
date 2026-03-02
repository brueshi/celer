use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("unsupported type for codegen: {0}")]
    UnsupportedType(String),

    #[error("undefined function: {0}")]
    UndefinedFunction(String),

    #[error("LLVM error: {0}")]
    LlvmError(String),

    #[error("unresolved type in codegen (run type inference first)")]
    UnresolvedType,
}

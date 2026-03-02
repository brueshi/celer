use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("unsupported type for codegen: {0}")]
    UnsupportedType(String),

    #[error("unsupported expression for codegen: {0}")]
    UnsupportedExpression(String),

    #[error("undefined function: {0}")]
    UndefinedFunction(String),

    #[error("undefined variable: {0}")]
    UndefinedVariable(String),

    #[error("LLVM error: {0}")]
    LlvmError(String),

    #[error("target machine error: {0}")]
    TargetMachineError(String),

    #[error("unresolved type in codegen (run type inference first)")]
    UnresolvedType,
}

use anyhow::{Context, Result};
use inkwell::context::Context as LlvmContext;

/// Run the full compilation pipeline: parse -> type inference -> codegen.
/// Returns the LLVM IR as a string.
pub fn compile(name: &str, path: &str, source: &str) -> Result<String> {
    // Stage 1: Parse Python source to HIR
    let mut module = celer_parser::parse_module(name, path, source).context("parsing failed")?;

    // Stage 2: Type inference
    let mut engine = celer_typeinfer::InferenceEngine::new();
    engine
        .infer_module(&mut module)
        .context("type inference failed")?;

    // Stage 3: LLVM codegen
    let llvm_ctx = LlvmContext::create();
    let mut compiler = celer_codegen::Compiler::new(&llvm_ctx, name);
    compiler
        .compile_module(&module)
        .context("code generation failed")?;

    Ok(compiler.dump_ir())
}

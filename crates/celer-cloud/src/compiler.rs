use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use inkwell::context::Context as LlvmContext;

/// Compile Python source to a shared library. Returns (artifact_path, compile_time_ms).
///
/// This function is blocking and should be called from `tokio::task::spawn_blocking`.
pub fn compile_to_shared_lib(
    module_name: &str,
    source: &str,
    output_dir: &Path,
) -> Result<(PathBuf, u64)> {
    let start = Instant::now();

    // Stage 1: Parse
    let mut module = celer_parser::parse_module(module_name, "<cloud>", source)
        .context("parsing failed")?;

    // Stage 2: Type inference
    let mut engine = celer_typeinfer::InferenceEngine::new();
    engine
        .infer_module(&mut module)
        .context("type inference failed")?;

    // Stage 3: LLVM codegen
    let llvm_ctx = LlvmContext::create();
    let mut compiler = celer_codegen::Compiler::new(&llvm_ctx, module_name);
    compiler
        .compile_module(&module)
        .context("code generation failed")?;
    compiler.verify().context("LLVM verification failed")?;

    // Stage 4: Write object file
    let obj_path = output_dir.join(format!("{module_name}.o"));
    compiler
        .write_object(&obj_path)
        .context("failed to write object file")?;

    // Stage 5: Link to shared library
    let ext = celer_runtime::shared_lib_extension();
    let lib_path = output_dir.join(format!("{module_name}.{ext}"));
    celer_runtime::link_shared(&obj_path, &lib_path).context("linking failed")?;

    // Cleanup object file
    let _ = std::fs::remove_file(&obj_path);

    let compile_time_ms = start.elapsed().as_millis() as u64;
    Ok((lib_path, compile_time_ms))
}

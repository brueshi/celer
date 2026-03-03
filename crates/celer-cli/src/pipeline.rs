use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use inkwell::context::Context as LlvmContext;

/// Result of the full compilation pipeline, including compilability info.
pub struct CompileResult {
    pub ir: String,
    pub compiled_functions: HashSet<String>,
    /// Functions that use JSON output-param calling convention (return dict).
    pub json_functions: HashSet<String>,
    pub skipped_functions: Vec<(String, String)>,
}

/// Extract function names that return dict (JSON calling convention) from HIR.
fn extract_json_functions(module: &celer_hir::Module) -> HashSet<String> {
    let mut json_fns = HashSet::new();
    for stmt in &module.body {
        if let celer_hir::Statement::FunctionDef(func) = stmt
            && celer_codegen::is_json_return(&func.return_type)
        {
            json_fns.insert(func.name.clone());
        }
    }
    json_fns
}

/// Run the full compilation pipeline: parse -> type inference -> compilability -> codegen.
/// Returns the LLVM IR as a string.
pub fn compile(name: &str, path: &str, source: &str) -> Result<String> {
    let result = compile_with_report(name, path, source)?;
    Ok(result.ir)
}

/// Run the full pipeline with compilability analysis report.
pub fn compile_with_report(name: &str, path: &str, source: &str) -> Result<CompileResult> {
    // Stage 1: Parse Python source to HIR
    let mut module = celer_parser::parse_module(name, path, source).context("parsing failed")?;

    // Stage 2: Type inference
    let mut engine = celer_typeinfer::InferenceEngine::new();
    engine
        .infer_module(&mut module)
        .context("type inference failed")?;

    // Stage 3: Compilability analysis
    let type_ctx = celer_typeinfer::TypeContext::new();
    let analyzer = celer_typeinfer::CompilabilityAnalyzer::new(&type_ctx);
    let report = analyzer.analyze_module(&module);

    let compiled_functions: HashSet<String> = report
        .compilable_functions()
        .into_iter()
        .map(String::from)
        .collect();

    let skipped_functions: Vec<(String, String)> = report
        .skipped_functions()
        .into_iter()
        .map(|(n, r)| (n.to_string(), r.to_string()))
        .collect();

    let json_functions = extract_json_functions(&module);

    // Stage 4: LLVM codegen
    let llvm_ctx = LlvmContext::create();
    let mut compiler = celer_codegen::Compiler::new(&llvm_ctx, name);
    compiler
        .compile_module(&module)
        .context("code generation failed")?;

    Ok(CompileResult {
        ir: compiler.dump_ir(),
        compiled_functions,
        json_functions,
        skipped_functions,
    })
}

/// Compile to a native object file.
pub fn compile_to_object(name: &str, path: &str, source: &str, output: &Path) -> Result<()> {
    // Stage 1: Parse
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

    // Verify before writing
    compiler.verify().context("LLVM verification failed")?;

    // Stage 4: Write native object file
    compiler
        .write_object(output)
        .context("failed to write object file")?;

    Ok(())
}

/// Compile to object and return compilability report alongside.
pub fn compile_to_object_with_report(
    name: &str,
    path: &str,
    source: &str,
    output: &Path,
) -> Result<CompileResult> {
    // Stage 1: Parse
    let mut module = celer_parser::parse_module(name, path, source).context("parsing failed")?;

    // Stage 2: Type inference
    let mut engine = celer_typeinfer::InferenceEngine::new();
    engine
        .infer_module(&mut module)
        .context("type inference failed")?;

    // Stage 3: Compilability analysis
    let type_ctx = celer_typeinfer::TypeContext::new();
    let analyzer = celer_typeinfer::CompilabilityAnalyzer::new(&type_ctx);
    let report = analyzer.analyze_module(&module);

    let compiled_functions: HashSet<String> = report
        .compilable_functions()
        .into_iter()
        .map(String::from)
        .collect();

    let skipped_functions: Vec<(String, String)> = report
        .skipped_functions()
        .into_iter()
        .map(|(n, r)| (n.to_string(), r.to_string()))
        .collect();

    let json_functions = extract_json_functions(&module);

    // Stage 4: LLVM codegen
    let llvm_ctx = LlvmContext::create();
    let mut compiler = celer_codegen::Compiler::new(&llvm_ctx, name);
    compiler
        .compile_module(&module)
        .context("code generation failed")?;

    compiler.verify().context("LLVM verification failed")?;

    // Stage 5: Write native object file
    compiler
        .write_object(output)
        .context("failed to write object file")?;

    Ok(CompileResult {
        ir: compiler.dump_ir(),
        compiled_functions,
        json_functions,
        skipped_functions,
    })
}

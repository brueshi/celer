use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::pipeline;

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Path to the Python source file
    pub input: PathBuf,

    /// Function to execute (defaults to "main")
    #[arg(short, long, default_value = "main")]
    pub function: String,

    /// Arguments to pass (as integers, space-separated)
    #[arg(short, long, num_args = 0..)]
    pub args: Vec<i64>,
}

pub fn execute(args: &RunArgs) -> Result<()> {
    let source = std::fs::read_to_string(&args.input)
        .with_context(|| format!("failed to read {}", args.input.display()))?;

    let name = args
        .input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");

    let path = args.input.to_string_lossy();

    // Set up temp directory for compilation artifacts
    let temp_dir = std::env::temp_dir().join("celer-run");
    std::fs::create_dir_all(&temp_dir)?;

    let obj_path = temp_dir.join(format!("{name}.o"));
    let ext = celer_runtime::shared_lib_extension();
    let lib_path = temp_dir.join(format!("{name}.{ext}"));

    // Full pipeline: parse -> infer -> analyze -> compile -> link
    let report = pipeline::compile_to_object_with_report(name, &path, &source, &obj_path)
        .context("compilation failed")?;

    // Print compilability summary
    println!("Compilability report:");
    if !report.compiled_functions.is_empty() {
        let mut funcs: Vec<&String> = report.compiled_functions.iter().collect();
        funcs.sort();
        for f in funcs {
            println!("  [native] {f}");
        }
    }
    for (name, reason) in &report.skipped_functions {
        println!("  [skip]   {name}: {reason}");
    }
    println!();

    // Link to shared library
    celer_runtime::link_shared(&obj_path, &lib_path).context("linking failed")?;

    // Build dispatcher
    let dispatcher = celer_runtime::FallbackDispatcher::with_library(
        &lib_path,
        source.clone(),
        report.compiled_functions,
        report.json_functions,
    )
    .context("failed to load native module")?;

    // Build arguments
    let call_args: Vec<celer_runtime::Value> =
        args.args.iter().map(|v| celer_runtime::Value::I64(*v)).collect();

    // Execute
    let is_native = dispatcher.is_compiled(&args.function);
    let result = dispatcher
        .call(&args.function, &call_args)
        .with_context(|| format!("failed to execute function '{}'", args.function))?;

    let mode = if is_native { "native" } else { "cpython" };
    println!("[{mode}] {}({}) = {result}", args.function, format_args_display(&call_args));

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(())
}

fn format_args_display(args: &[celer_runtime::Value]) -> String {
    args.iter()
        .map(|a| a.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

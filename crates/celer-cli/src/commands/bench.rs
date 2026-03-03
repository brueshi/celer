use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::pipeline;

#[derive(Debug, Args)]
pub struct BenchArgs {
    /// Number of warmup iterations
    #[arg(long, default_value = "1000")]
    pub warmup: u64,

    /// Number of benchmark iterations
    #[arg(long, default_value = "100000")]
    pub iterations: u64,

    /// Output format: table, json
    #[arg(long, default_value = "table")]
    pub format: String,
}

pub fn execute(args: &BenchArgs) -> Result<()> {
    let workloads = celer_bench::Workload::all_workloads();
    let runner = celer_bench::BenchRunner::new(args.warmup, args.iterations);

    let mut results = Vec::new();
    let temp_dir = std::env::temp_dir().join("celer-bench");
    std::fs::create_dir_all(&temp_dir)?;

    for workload in &workloads {
        // Run CPython benchmark
        println!("Running {} (cpython)...", workload.name);
        match runner.run_cpython(workload) {
            Ok(result) => results.push(result),
            Err(e) => {
                eprintln!("  CPython benchmark failed: {e}");
            }
        }

        // Compile to native
        println!("Compiling {} (celer-aot)...", workload.name);
        let obj_path = temp_dir.join(format!("{}.o", workload.name));
        let ext = celer_runtime::shared_lib_extension();
        let lib_path = temp_dir.join(format!("{}.{}", workload.name, ext));

        match compile_workload(workload, &obj_path, &lib_path) {
            Ok(()) => {
                println!("Running {} (celer-aot)...", workload.name);
                match runner.run_native(workload, &lib_path) {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        eprintln!("  Native benchmark failed: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("  Compilation failed: {e}");
            }
        }
    }

    // Print results
    if results.is_empty() {
        println!("No benchmark results collected.");
        return Ok(());
    }

    match args.format.as_str() {
        "json" => println!("{}", celer_bench::Reporter::format_json(&results)),
        _ => println!("\n{}", celer_bench::Reporter::format_table(&results)),
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(())
}

fn compile_workload(
    workload: &celer_bench::Workload,
    obj_path: &Path,
    lib_path: &Path,
) -> Result<()> {
    pipeline::compile_to_object(
        &workload.name,
        "<bench>",
        &workload.python_source,
        obj_path,
    )
    .context("compilation failed")?;

    celer_runtime::link_shared(obj_path, lib_path).context("linking failed")?;

    Ok(())
}

use std::path::PathBuf;
use std::process::Command;

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

    /// Runners to compare: cpython,celer,go,rust
    #[arg(long, default_value = "cpython,celer")]
    pub compare: String,
}

pub fn execute(args: &BenchArgs) -> Result<()> {
    let workloads = celer_bench::Workload::all_workloads();
    let runner = celer_bench::BenchRunner::new(args.warmup, args.iterations);
    let runners: Vec<&str> = args.compare.split(',').map(str::trim).collect();

    // Auto-build external runners if requested
    if runners.contains(&"go") {
        build_go_benchmarks();
    }
    if runners.contains(&"rust") {
        build_rust_benchmarks();
    }

    let mut results = Vec::new();
    let temp_dir = std::env::temp_dir().join("celer-bench");
    std::fs::create_dir_all(&temp_dir)?;

    for workload in &workloads {
        // Run CPython benchmark
        if runners.contains(&"cpython") {
            println!("Running {} (cpython)...", workload.name);
            match runner.run_cpython(workload) {
                Ok(result) => results.push(result),
                Err(e) => eprintln!("  CPython benchmark failed: {e}"),
            }
        }

        // Compile and run celer-aot benchmark
        if runners.contains(&"celer") {
            println!("Compiling {} (celer-aot)...", workload.name);
            let obj_path = temp_dir.join(format!("{}.o", workload.name));
            let ext = celer_runtime::shared_lib_extension();
            let lib_path = temp_dir.join(format!("{}.{}", workload.name, ext));

            match compile_workload(workload, &obj_path, &lib_path) {
                Ok(()) => {
                    println!("Running {} (celer-aot)...", workload.name);
                    match runner.run_native(workload, &lib_path) {
                        Ok(result) => results.push(result),
                        Err(e) => eprintln!("  Native benchmark failed: {e}"),
                    }
                }
                Err(e) => eprintln!("  Compilation failed: {e}"),
            }
        }

        let binary_name = workload.name.replace('-', "_");

        // Run Go benchmark
        if runners.contains(&"go") {
            let go_bin = PathBuf::from(format!("benches/go/cmd/{binary_name}/{binary_name}"));
            if go_bin.exists() {
                println!("Running {} (go)...", workload.name);
                match runner.run_external(&workload.name, "go", &go_bin) {
                    Ok(result) => results.push(result),
                    Err(e) => eprintln!("  Go benchmark failed: {e}"),
                }
            }
        }

        // Run Rust benchmark
        if runners.contains(&"rust") {
            let rust_bin = PathBuf::from(format!("benches/rust/target/release/{binary_name}"));
            if rust_bin.exists() {
                println!("Running {} (rust)...", workload.name);
                match runner.run_external(&workload.name, "rust", &rust_bin) {
                    Ok(result) => results.push(result),
                    Err(e) => eprintln!("  Rust benchmark failed: {e}"),
                }
            }
        }
    }

    if results.is_empty() {
        println!("No benchmark results collected.");
        return Ok(());
    }

    match args.format.as_str() {
        "json" => println!("{}", celer_bench::Reporter::format_json(&results)),
        _ => println!("\n{}", celer_bench::Reporter::format_table(&results)),
    }

    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(())
}

fn build_go_benchmarks() {
    let go_dir = PathBuf::from("benches/go");
    if !go_dir.exists() {
        eprintln!("Go bench directory not found at benches/go/");
        return;
    }

    println!("Building Go benchmarks...");

    // Build each binary into its own cmd directory
    let workload_bins = [
        "json_serialize_static",
        "json_serialize_dynamic",
        "fibonacci",
        "for_loop_sum",
        "business_logic",
        "http_path_param",
        "http_compute_endpoint",
    ];

    for name in &workload_bins {
        let pkg = format!("./cmd/{name}");
        let out = format!("./cmd/{name}/{name}");
        let status = Command::new("go")
            .args(["build", "-o", &out, &pkg])
            .current_dir(&go_dir)
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => eprintln!("Go build of {name} exited with status: {s}"),
            Err(e) => eprintln!("Failed to build Go {name}: {e}"),
        }
    }

    println!("Go benchmarks built.");
}

fn build_rust_benchmarks() {
    let rust_dir = PathBuf::from("benches/rust");
    if !rust_dir.exists() {
        eprintln!("Rust bench directory not found at benches/rust/");
        return;
    }

    println!("Building Rust benchmarks...");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&rust_dir)
        .status();

    match status {
        Ok(s) if s.success() => println!("Rust benchmarks built successfully."),
        Ok(s) => eprintln!("Rust build exited with status: {s}"),
        Err(e) => eprintln!("Failed to run cargo build: {e}"),
    }
}

fn compile_workload(
    workload: &celer_bench::Workload,
    obj_path: &std::path::Path,
    lib_path: &std::path::Path,
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

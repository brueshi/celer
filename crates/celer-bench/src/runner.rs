use std::ffi::CString;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use pyo3::prelude::*;

use crate::workloads::Workload;

/// Result from a single benchmark run.
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub workload_name: String,
    pub runner_name: String,
    pub iterations: u64,
    pub total_duration: Duration,
}

impl BenchResult {
    pub fn ops_per_sec(&self) -> f64 {
        self.iterations as f64 / self.total_duration.as_secs_f64()
    }

    pub fn avg_ns(&self) -> f64 {
        self.total_duration.as_nanos() as f64 / self.iterations as f64
    }
}

pub struct BenchRunner {
    warmup_iterations: u64,
    bench_iterations: u64,
}

impl BenchRunner {
    pub fn new(warmup_iterations: u64, bench_iterations: u64) -> Self {
        Self {
            warmup_iterations,
            bench_iterations,
        }
    }

    /// Run CPython benchmark for a workload.
    pub fn run_cpython(&self, workload: &Workload) -> Result<BenchResult> {
        let py_code = Self::build_cpython_bench_code(workload);
        let py_code_c = CString::new(py_code)?;
        let call_code = Self::build_cpython_call_code(workload);
        let call_code_c = CString::new(call_code)?;

        // Warmup
        Python::with_gil(|py| -> Result<()> {
            py.run(&py_code_c, None, None)?;
            for _ in 0..self.warmup_iterations {
                py.run(&call_code_c, None, None)?;
            }
            Ok(())
        })?;

        // Benchmark
        let start = Instant::now();
        Python::with_gil(|py| -> Result<()> {
            for _ in 0..self.bench_iterations {
                py.run(&call_code_c, None, None)?;
            }
            Ok(())
        })?;
        let duration = start.elapsed();

        Ok(BenchResult {
            workload_name: workload.name.clone(),
            runner_name: "cpython".into(),
            iterations: self.bench_iterations,
            total_duration: duration,
        })
    }

    /// Run native (Celer AOT) benchmark for a workload.
    pub fn run_native(&self, workload: &Workload, lib_path: &Path) -> Result<BenchResult> {
        use crate::workloads::ReturnKind;
        use celer_runtime::{NativeModule, Value};

        let native = unsafe { NativeModule::load(lib_path)? };

        let args: Vec<Value> = match workload.arg {
            None => vec![],
            Some(val) => vec![Value::I64(val)],
        };

        // Select dispatch strategy based on return kind to avoid
        // the overhead of try-json-then-scalar fallback per iteration.
        match workload.return_kind {
            ReturnKind::Json => {
                // Warmup
                for _ in 0..self.warmup_iterations {
                    match workload.arg {
                        None => { native.call_no_args(&workload.function_name)?; }
                        Some(val) => { native.call_one_int(&workload.function_name, val)?; }
                    }
                }
                // Benchmark
                let start = Instant::now();
                for _ in 0..self.bench_iterations {
                    match workload.arg {
                        None => { native.call_no_args(&workload.function_name)?; }
                        Some(val) => { native.call_one_int(&workload.function_name, val)?; }
                    }
                }
                let duration = start.elapsed();
                Ok(BenchResult {
                    workload_name: workload.name.clone(),
                    runner_name: "celer-aot".into(),
                    iterations: self.bench_iterations,
                    total_duration: duration,
                })
            }
            ReturnKind::ScalarI64 => {
                // Warmup
                for _ in 0..self.warmup_iterations {
                    native.call(&workload.function_name, &args)?;
                }
                // Benchmark
                let start = Instant::now();
                for _ in 0..self.bench_iterations {
                    native.call(&workload.function_name, &args)?;
                }
                let duration = start.elapsed();
                Ok(BenchResult {
                    workload_name: workload.name.clone(),
                    runner_name: "celer-aot".into(),
                    iterations: self.bench_iterations,
                    total_duration: duration,
                })
            }
        }
    }

    fn build_cpython_bench_code(workload: &Workload) -> String {
        format!("import json\n{}", workload.python_source.trim())
    }

    fn build_cpython_call_code(workload: &Workload) -> String {
        use crate::workloads::ReturnKind;
        match (&workload.return_kind, workload.arg) {
            (ReturnKind::Json, None) => format!("json.dumps({}())", workload.function_name),
            (ReturnKind::Json, Some(val)) => {
                format!("json.dumps({}({}))", workload.function_name, val)
            }
            (ReturnKind::ScalarI64, None) => format!("{}()", workload.function_name),
            (ReturnKind::ScalarI64, Some(val)) => {
                format!("{}({})", workload.function_name, val)
            }
        }
    }

    /// Run an external benchmark binary (Go, Rust, etc).
    /// The binary must accept --iterations N --warmup N and print JSON:
    /// {"iterations": N, "total_ns": N}
    pub fn run_external(
        &self,
        workload_name: &str,
        runner_name: &str,
        binary_path: &Path,
    ) -> Result<BenchResult> {
        let output = Command::new(binary_path)
            .arg("--iterations")
            .arg(self.bench_iterations.to_string())
            .arg("--warmup")
            .arg(self.warmup_iterations.to_string())
            .output()
            .with_context(|| format!("failed to run {}", binary_path.display()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{runner_name} benchmark failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim();
        let parsed: serde_json::Value = serde_json::from_str(line)
            .with_context(|| format!("failed to parse {runner_name} output: {line}"))?;

        let iterations = parsed["iterations"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("missing iterations field"))?;
        let total_ns = parsed["total_ns"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("missing total_ns field"))?;

        Ok(BenchResult {
            workload_name: workload_name.to_string(),
            runner_name: runner_name.to_string(),
            iterations,
            total_duration: Duration::from_nanos(total_ns),
        })
    }
}

impl Default for BenchRunner {
    fn default() -> Self {
        Self::new(1000, 100_000)
    }
}

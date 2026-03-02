use crate::runner::BenchResult;

pub struct Reporter;

impl Reporter {
    /// Format results as a table string.
    pub fn format_table(results: &[BenchResult]) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "{:<26} {:<13} {:>12} {:>12} {:>10}\n",
            "Workload", "Runner", "Ops/sec", "Avg (ns)", "Speedup"
        ));
        output.push_str(&"-".repeat(75));
        output.push('\n');

        // Group by workload to compute speedup
        let workload_names: Vec<String> = results
            .iter()
            .map(|r| r.workload_name.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for wl_name in &workload_names {
            let wl_results: Vec<&BenchResult> = results
                .iter()
                .filter(|r| &r.workload_name == wl_name)
                .collect();

            let cpython_ops = wl_results
                .iter()
                .find(|r| r.runner_name == "cpython")
                .map(|r| r.ops_per_sec())
                .unwrap_or(1.0);

            for result in &wl_results {
                let ops = result.ops_per_sec();
                let avg_ns = result.avg_ns();
                let speedup = ops / cpython_ops;

                output.push_str(&format!(
                    "{:<26} {:<13} {:>12.0} {:>12.0} {:>9.1}x\n",
                    result.workload_name, result.runner_name, ops, avg_ns, speedup
                ));
            }
        }

        output
    }

    /// Format results as JSON.
    pub fn format_json(results: &[BenchResult]) -> String {
        let entries: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "workload": r.workload_name,
                    "runner": r.runner_name,
                    "iterations": r.iterations,
                    "total_ms": r.total_duration.as_millis(),
                    "ops_per_sec": r.ops_per_sec(),
                    "avg_ns": r.avg_ns(),
                })
            })
            .collect();

        serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".into())
    }
}

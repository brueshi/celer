use crate::runner::BenchResult;

pub struct Reporter;

impl Reporter {
    /// Format results as a table string with multi-runner support.
    pub fn format_table(results: &[BenchResult]) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "{:<28} {:<14} {:>14} {:>12} {:>10}\n",
            "Workload", "Runner", "Ops/sec", "Avg (ns)", "Speedup"
        ));
        output.push_str(&"-".repeat(80));
        output.push('\n');

        // Preserve workload order from input
        let mut workload_order = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for r in results {
            if seen.insert(r.workload_name.clone()) {
                workload_order.push(r.workload_name.clone());
            }
        }

        let mut all_speedups: Vec<f64> = Vec::new();

        for wl_name in &workload_order {
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

                if result.runner_name != "cpython" {
                    all_speedups.push(speedup);
                }

                output.push_str(&format!(
                    "{:<28} {:<14} {:>14.0} {:>12.0} {:>9.1}x\n",
                    result.workload_name, result.runner_name, ops, avg_ns, speedup
                ));
            }
            output.push('\n');
        }

        // Geometric mean of all non-cpython speedups
        if !all_speedups.is_empty() {
            let geo_mean =
                all_speedups.iter().product::<f64>().powf(1.0 / all_speedups.len() as f64);
            output.push_str(&"-".repeat(80));
            output.push('\n');
            output.push_str(&format!(
                "{:<28} {:<14} {:>14} {:>12} {:>9.1}x\n",
                "geometric-mean", "all", "", "", geo_mean
            ));
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

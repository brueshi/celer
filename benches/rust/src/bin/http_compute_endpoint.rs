use std::time::Instant;

use clap::Parser;
use serde::Serialize;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "100000")]
    iterations: u64,
    #[arg(long, default_value = "1000")]
    warmup: u64,
}

#[derive(Serialize)]
struct ComputeResponse {
    result: i64,
    input: i64,
}

#[inline]
fn compute(n: i64) -> ComputeResponse {
    let mut result = 0i64;
    for i in 0..n {
        result += i;
    }
    ComputeResponse { result, input: n }
}

fn main() {
    let args = Args::parse();

    for _ in 0..args.warmup {
        let _ = serde_json::to_string(&compute(100));
    }

    let start = Instant::now();
    for _ in 0..args.iterations {
        let data = serde_json::to_string(&compute(100)).unwrap();
        std::hint::black_box(&data);
    }
    let elapsed = start.elapsed();

    println!(
        r#"{{"iterations":{},"total_ns":{}}}"#,
        args.iterations,
        elapsed.as_nanos()
    );
}

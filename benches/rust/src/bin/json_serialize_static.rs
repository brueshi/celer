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
struct Response {
    message: &'static str,
}

#[inline]
fn root() -> Response {
    Response { message: "hello" }
}

fn main() {
    let args = Args::parse();

    // Warmup
    for _ in 0..args.warmup {
        let _ = serde_json::to_string(&root());
    }

    // Benchmark
    let start = Instant::now();
    for _ in 0..args.iterations {
        let data = serde_json::to_string(&root()).unwrap();
        std::hint::black_box(&data);
    }
    let elapsed = start.elapsed();

    println!(
        r#"{{"iterations":{},"total_ns":{}}}"#,
        args.iterations,
        elapsed.as_nanos()
    );
}

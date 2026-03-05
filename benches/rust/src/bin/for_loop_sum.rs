use std::time::Instant;

use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "100000")]
    iterations: u64,
    #[arg(long, default_value = "1000")]
    warmup: u64,
}

#[inline]
fn range_sum(n: i64) -> i64 {
    let mut total = 0i64;
    for i in 0..n {
        total += i;
    }
    total
}

fn main() {
    let args = Args::parse();

    for _ in 0..args.warmup {
        std::hint::black_box(range_sum(1000));
    }

    let start = Instant::now();
    for _ in 0..args.iterations {
        std::hint::black_box(range_sum(1000));
    }
    let elapsed = start.elapsed();

    println!(
        r#"{{"iterations":{},"total_ns":{}}}"#,
        args.iterations,
        elapsed.as_nanos()
    );
}

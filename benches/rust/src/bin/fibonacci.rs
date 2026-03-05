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
fn fib(n: i64) -> i64 {
    let (mut a, mut b) = (0i64, 1i64);
    for _ in 0..n {
        let t = a + b;
        a = b;
        b = t;
    }
    a
}

fn main() {
    let args = Args::parse();

    for _ in 0..args.warmup {
        std::hint::black_box(fib(30));
    }

    let start = Instant::now();
    for _ in 0..args.iterations {
        std::hint::black_box(fib(30));
    }
    let elapsed = start.elapsed();

    println!(
        r#"{{"iterations":{},"total_ns":{}}}"#,
        args.iterations,
        elapsed.as_nanos()
    );
}

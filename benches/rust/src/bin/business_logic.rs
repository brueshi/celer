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
struct PriceResponse {
    price: i64,
    currency: &'static str,
}

#[inline]
fn apply_discount(price: i64, threshold: i64) -> i64 {
    if price > threshold {
        price * 90 / 100
    } else {
        price
    }
}

#[inline]
fn calculate_price(base_price: i64) -> PriceResponse {
    let final_price = apply_discount(base_price, 50);
    PriceResponse {
        price: final_price,
        currency: "USD",
    }
}

fn main() {
    let args = Args::parse();

    for _ in 0..args.warmup {
        let _ = serde_json::to_string(&calculate_price(100));
    }

    let start = Instant::now();
    for _ in 0..args.iterations {
        let data = serde_json::to_string(&calculate_price(100)).unwrap();
        std::hint::black_box(&data);
    }
    let elapsed = start.elapsed();

    println!(
        r#"{{"iterations":{},"total_ns":{}}}"#,
        args.iterations,
        elapsed.as_nanos()
    );
}

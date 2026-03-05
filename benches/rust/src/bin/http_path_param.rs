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
struct ItemResponse {
    item_id: i64,
    name: &'static str,
    in_stock: bool,
}

#[inline]
fn get_item(item_id: i64) -> ItemResponse {
    ItemResponse {
        item_id,
        name: "widget",
        in_stock: true,
    }
}

fn main() {
    let args = Args::parse();

    for _ in 0..args.warmup {
        let _ = serde_json::to_string(&get_item(42));
    }

    let start = Instant::now();
    for _ in 0..args.iterations {
        let data = serde_json::to_string(&get_item(42)).unwrap();
        std::hint::black_box(&data);
    }
    let elapsed = start.elapsed();

    println!(
        r#"{{"iterations":{},"total_ns":{}}}"#,
        args.iterations,
        elapsed.as_nanos()
    );
}

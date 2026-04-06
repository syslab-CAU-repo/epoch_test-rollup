use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SendTimeLine {
    raw_transaction: String,
    send_epoch_ms: u128,
}

fn normalize_raw(s: &str) -> String {
    let s = s.trim().trim_matches('"');
    let s = s.strip_prefix("0X").or_else(|| s.strip_prefix("0x")).unwrap_or(s);
    format!("0x{}", s.to_ascii_lowercase())
}

fn csv_row_fields(line: &str) -> Option<(u128, String)> {
    let mut parts = line.splitn(3, ',');
    let _index = parts.next()?;
    let arrived_at_epoch_ms: u128 = parts.next()?.parse().ok()?;
    let rest = parts.next()?;
    let raw = rest.trim().trim_matches('"').to_string();
    Some((arrived_at_epoch_ms, raw))
}

fn read_json_field_u128(path: &str, field: &str) -> u128 {
    let f = File::open(path).unwrap_or_else(|e| panic!("open {}: {}", path, e));
    let v: serde_json::Value =
        serde_json::from_reader(f).unwrap_or_else(|e| panic!("parse {}: {}", path, e));
    v[field]
        .as_u64()
        .map(|n| n as u128)
        .or_else(|| v[field].as_f64().map(|n| n as u128))
        .unwrap_or_else(|| panic!("{}: field '{}' is not a number", path, field))
}

fn main() {
    let mut args = env::args().skip(1);
    let csv_path = args.next().expect(
        "usage: latency_stats <arrival.csv> <send_time_map.jsonl> <client_meta.json> <rollup_meta.json> [denominator]",
    );
    let jsonl_path = args.next().expect(
        "usage: latency_stats <arrival.csv> <send_time_map.jsonl> <client_meta.json> <rollup_meta.json> [denominator]",
    );
    let client_meta_path = args.next().expect(
        "usage: latency_stats <arrival.csv> <send_time_map.jsonl> <client_meta.json> <rollup_meta.json> [denominator]",
    );
    let rollup_meta_path = args.next().expect(
        "usage: latency_stats <arrival.csv> <send_time_map.jsonl> <client_meta.json> <rollup_meta.json> [denominator]",
    );
    let denominator: Option<usize> = args
        .next()
        .map(|s| s.parse().expect("denominator must be usize"));

    let experiment_start_epoch_ms =
        read_json_field_u128(&client_meta_path, "experiment_start_epoch_ms");
    let experiment_end_epoch_ms =
        read_json_field_u128(&rollup_meta_path, "experiment_end_epoch_ms");
    let experiment_duration_ms = experiment_end_epoch_ms.saturating_sub(experiment_start_epoch_ms);

    let send_map: HashMap<String, u128> = {
        let f = File::open(&jsonl_path).unwrap_or_else(|e| panic!("open {}: {}", jsonl_path, e));
        let mut m = HashMap::new();
        for (lineno, line) in BufReader::new(f).lines().enumerate() {
            let line =
                line.unwrap_or_else(|e| panic!("read {} line {}: {}", jsonl_path, lineno + 1, e));
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let rec: SendTimeLine = serde_json::from_str(line).unwrap_or_else(|e| {
                panic!("invalid JSON at {}:{}: {}\n{}", jsonl_path, lineno + 1, e, line)
            });
            m.insert(normalize_raw(&rec.raw_transaction), rec.send_epoch_ms);
        }
        m
    };

    let csv_file = File::open(&csv_path).unwrap_or_else(|e| panic!("open {}: {}", csv_path, e));
    let mut lines = BufReader::new(csv_file).lines();
    let header = lines.next().transpose().expect("csv header").expect("empty csv");
    if !header.starts_with("index,") {
        eprintln!("warning: unexpected header: {}", header);
    }

    let mut latencies_ms: Vec<i64> = Vec::new();
    let mut missing = 0usize;
    let mut total_csv_rows = 0usize;

    for (lineno, line) in lines.enumerate() {
        let line = line.unwrap_or_else(|e| panic!("read {}: {}", csv_path, e));
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        total_csv_rows += 1;
        let Some((arrived_at_epoch_ms, raw)) = csv_row_fields(line) else {
            panic!("bad csv row {}:{}: {}", csv_path, lineno + 2, line);
        };
        let arrival_ms = arrived_at_epoch_ms as i64;
        let key = normalize_raw(&raw);
        let Some(&send_ms) = send_map.get(&key) else {
            missing += 1;
            let pfx: String = key.chars().take(20).collect();
            eprintln!("missing send_time for key prefix {}...", pfx);
            continue;
        };
        latencies_ms.push(arrival_ms - send_ms as i64);
    }

    let n = latencies_ms.len();
    if n == 0 {
        println!("no matched rows");
        return;
    }

    latencies_ms.sort_unstable();
    let sum: i128 = latencies_ms.iter().map(|&x| x as i128).sum();
    let denom = denominator.unwrap_or(n);
    let mean_over_denom = sum as f64 / denom as f64;
    let average_matched = sum as f64 / n as f64;

    let median = if n % 2 == 1 {
        latencies_ms[n / 2] as f64
    } else {
        (latencies_ms[n / 2 - 1] as f64 + latencies_ms[n / 2] as f64) / 2.0
    };

    println!("=== Latency ===");
    println!("matched rows: {}", n);
    if missing > 0 {
        println!("missing send_time keys: {}", missing);
    }
    println!("sum(latency_ms): {}", sum);
    println!("average_ms (sum / matched rows = {}): {:.6}", n, average_matched);
    println!("mean_ms (sum / {}): {:.6}", denom, mean_over_denom);
    println!("median_ms: {:.6}", median);

    println!("\n=== TPS ===");
    println!(
        "experiment_start_epoch_ms (client): {}",
        experiment_start_epoch_ms
    );
    println!(
        "experiment_end_epoch_ms   (rollup): {}",
        experiment_end_epoch_ms
    );
    println!("experiment_duration_ms: {}", experiment_duration_ms);
    let duration_secs = experiment_duration_ms as f64 / 1000.0;
    println!("experiment_duration_s: {:.3}", duration_secs);
    println!("total_received_tx (csv rows): {}", total_csv_rows);
    if duration_secs > 0.0 {
        let tps = total_csv_rows as f64 / duration_secs;
        println!("TPS (total_received_tx / duration_s): {:.6}", tps);
    } else {
        println!("TPS: N/A (duration is zero)");
    }
}

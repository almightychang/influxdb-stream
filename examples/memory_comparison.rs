//! Memory usage comparison: influxdb-stream vs influxdb2
//!
//! Run with: `cargo run --release --example memory_comparison`

use futures::StreamExt;
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};
use tokio::runtime::Runtime;

// Test configuration
const INFLUXDB_URL: &str = "http://localhost:8086";
const INFLUXDB_ORG: &str = "test-org";
const INFLUXDB_TOKEN: &str = "test-token-for-development-only";
const INFLUXDB_BUCKET: &str = "test-bucket";

fn get_process_memory_mb() -> f64 {
    let mut sys = System::new();
    let pid = sysinfo::get_current_pid().unwrap();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

    if let Some(process) = sys.process(pid) {
        process.memory() as f64 / 1024.0 / 1024.0
    } else {
        0.0
    }
}

fn influxdb_available() -> bool {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();
        client
            .get(format!("{}/health", INFLUXDB_URL))
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    })
}

fn write_benchmark_data(measurement: &str, count: usize) {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // Delete existing data
        let delete_url = format!(
            "{}/api/v2/delete?org={}&bucket={}",
            INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_BUCKET
        );
        let _ = client
            .post(&delete_url)
            .header("Authorization", format!("Token {}", INFLUXDB_TOKEN))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "start": "1970-01-01T00:00:00Z",
                "stop": "2100-01-01T00:00:00Z",
                "predicate": format!("_measurement=\"{}\"", measurement)
            }))
            .send()
            .await;

        // Write data in batches
        let batch_size = 10000;
        let base_ts = 1700000000000i64;
        let num_batches = count.div_ceil(batch_size);

        for batch in 0..num_batches {
            let start_idx = batch * batch_size;
            let end_idx = std::cmp::min(start_idx + batch_size, count);
            let mut lines = Vec::with_capacity(end_idx - start_idx);

            for idx in start_idx..end_idx {
                let ts = base_ts + (idx as i64 * 1000);
                lines.push(format!(
                    "{},host=server{},region=us-east value={}.{} {}",
                    measurement,
                    idx % 100,
                    idx % 1000,
                    idx % 10000,
                    ts
                ));
            }

            let url = format!(
                "{}/api/v2/write?org={}&bucket={}&precision=ms",
                INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_BUCKET
            );

            client
                .post(&url)
                .header("Authorization", format!("Token {}", INFLUXDB_TOKEN))
                .header("Content-Type", "text/plain")
                .body(lines.join("\n"))
                .send()
                .await
                .unwrap();

            if (batch + 1) % 10 == 0 {
                print!(".");
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    });
}

fn main() {
    if !influxdb_available() {
        eprintln!("Error: InfluxDB not available at {}", INFLUXDB_URL);
        eprintln!("Start it with: docker-compose up -d");
        std::process::exit(1);
    }

    let rt = Runtime::new().unwrap();
    let sizes = [10_000, 50_000, 100_000, 200_000];

    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║                         MEMORY USAGE COMPARISON                              ║");
    println!("║                    (System RSS - Resident Set Size)                          ║");
    println!("╠══════════════════════════════════════════════════════════════════════════════╣");
    println!("║  Records   │ influxdb-stream (MB) │ influxdb2 (MB) │ Ratio │ Memory Saved   ║");
    println!("╠══════════════════════════════════════════════════════════════════════════════╣");

    for size in sizes {
        let measurement = format!("mem_test_{}", size);
        print!("║  Setting up {:>7} records", size);
        use std::io::Write;
        std::io::stdout().flush().ok();

        write_benchmark_data(&measurement, size);
        println!(" ... done");

        // Measure influxdb-stream (streaming - O(1) memory)
        let stream_result = rt.block_on(async {
            // Force GC and stabilize
            tokio::time::sleep(Duration::from_millis(500)).await;
            let baseline = get_process_memory_mb();

            let client = influxdb_stream::Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
            let query = format!(
                r#"from(bucket: "{}")
                   |> range(start: 2023-01-01T00:00:00Z)
                   |> filter(fn: (r) => r._measurement == "{}")"#,
                INFLUXDB_BUCKET, measurement
            );

            let mut stream = client.query_stream(&query).await.unwrap();
            let mut count = 0;
            let mut peak_memory = baseline;

            while let Some(result) = stream.next().await {
                let _ = result.unwrap();
                count += 1;

                // Sample memory periodically
                if count % 1000 == 0 {
                    let current = get_process_memory_mb();
                    if current > peak_memory {
                        peak_memory = current;
                    }
                }
            }

            let final_memory = get_process_memory_mb();
            if final_memory > peak_memory {
                peak_memory = final_memory;
            }

            (count, peak_memory - baseline)
        });

        // Measure influxdb2 (collects all into Vec - O(n) memory)
        let influx2_result = rt.block_on(async {
            // Force GC and stabilize
            tokio::time::sleep(Duration::from_millis(500)).await;
            let baseline = get_process_memory_mb();

            let client = influxdb2::Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
            let flux_query = format!(
                r#"from(bucket: "{}")
                   |> range(start: 2023-01-01T00:00:00Z)
                   |> filter(fn: (r) => r._measurement == "{}")"#,
                INFLUXDB_BUCKET, measurement
            );
            let query = influxdb2::models::Query::new(flux_query);

            let records = client.query_raw(Some(query)).await.unwrap_or_default();
            let count = records.len();

            // Measure memory while holding all records
            let peak_memory = get_process_memory_mb();

            // Keep records alive until measurement
            drop(records);

            (count, peak_memory - baseline)
        });

        let ratio = if stream_result.1 > 0.0 {
            influx2_result.1 / stream_result.1
        } else {
            f64::INFINITY
        };

        let saved = if influx2_result.1 > 0.0 {
            ((influx2_result.1 - stream_result.1) / influx2_result.1 * 100.0).max(0.0)
        } else {
            0.0
        };

        println!(
            "║  {:>8} │ {:>20.2} │ {:>14.2} │ {:>5.1}x │ {:>12.1}%  ║",
            size, stream_result.1, influx2_result.1, ratio, saved
        );
    }

    println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!(
        "Note: influxdb-stream uses O(1) memory (streaming), influxdb2 uses O(n) memory (Vec)."
    );
    println!();
}

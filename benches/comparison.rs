//! Comparison benchmarks: influxdb-stream vs influxdb2
//!
//! This benchmark compares the performance and memory usage of:
//! - `influxdb-stream`: Our streaming client
//! - `influxdb2`: The existing non-streaming client
//!
//! Run with: `cargo bench --bench comparison`

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use futures::StreamExt;
use std::time::Duration;
use tokio::runtime::Runtime;

// Test configuration
const INFLUXDB_URL: &str = "http://localhost:8086";
const INFLUXDB_ORG: &str = "test-org";
const INFLUXDB_TOKEN: &str = "test-token-for-development-only";
const INFLUXDB_BUCKET: &str = "test-bucket";

// ============================================================================
// Helpers
// ============================================================================

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
        let num_batches = (count + batch_size - 1) / batch_size;

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

// ============================================================================
// Throughput Comparison
// ============================================================================

fn bench_throughput_comparison(c: &mut Criterion) {
    if !influxdb_available() {
        eprintln!("Skipping benchmarks: InfluxDB not available");
        return;
    }

    let rt = Runtime::new().unwrap();
    let sizes = [1_000, 5_000, 10_000];

    let mut group = c.benchmark_group("comparison");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    for size in sizes {
        let measurement = format!("cmp_{}", size);
        println!("\nSetting up {} records...", size);
        write_benchmark_data(&measurement, size);

        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("influxdb-stream", size),
            &size,
            |b, &_size| {
                b.to_async(&rt).iter(|| async {
                    let client = influxdb_stream::Client::new(
                        INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN,
                    );
                    let query = format!(
                        r#"from(bucket: "{}")
                           |> range(start: 2023-01-01T00:00:00Z)
                           |> filter(fn: (r) => r._measurement == "{}")"#,
                        INFLUXDB_BUCKET, measurement
                    );

                    let mut stream = client.query_stream(&query).await.unwrap();
                    let mut count = 0;
                    while let Some(result) = stream.next().await {
                        result.unwrap();
                        count += 1;
                    }
                    count
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("influxdb2", size),
            &size,
            |b, &_size| {
                b.to_async(&rt).iter(|| async {
                    let client = influxdb2::Client::new(
                        INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN,
                    );
                    let flux_query = format!(
                        r#"from(bucket: "{}")
                           |> range(start: 2023-01-01T00:00:00Z)
                           |> filter(fn: (r) => r._measurement == "{}")"#,
                        INFLUXDB_BUCKET, measurement
                    );
                    let query = influxdb2::models::Query::new(flux_query);

                    let records = client.query_raw(Some(query)).await.unwrap_or_default();
                    records.len()
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Criterion Benchmarks
// ============================================================================

criterion_group!(
    benches,
    bench_throughput_comparison,
);

criterion_main!(benches);

// Memory comparison is available as a separate example:
// cargo run --release --example memory_comparison

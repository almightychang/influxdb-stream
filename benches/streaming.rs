//! Benchmarks for influxdb-stream.
//!
//! These benchmarks require a running InfluxDB instance with test data.
//!
//! Setup:
//! 1. Start InfluxDB: `docker-compose up -d`
//! 2. Run integration tests to populate data: `cargo test --test integration`
//! 3. Run benchmarks: `cargo bench`
//!
//! Or use the setup script:
//! ```bash
//! ./scripts/bench-setup.sh
//! cargo bench
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use futures::StreamExt;
use influxdb_stream::Client;
use std::time::Duration;
use tokio::runtime::Runtime;

// Test configuration - matches docker-compose.yml
const INFLUXDB_URL: &str = "http://localhost:8086";
const INFLUXDB_ORG: &str = "test-org";
const INFLUXDB_TOKEN: &str = "test-token-for-development-only";
const INFLUXDB_BUCKET: &str = "test-bucket";

/// Check if InfluxDB is available
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

/// Write test data for benchmarking
fn write_benchmark_data(measurement: &str, count: usize) {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // First, try to delete existing data
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
        let batch_size = 5000;
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
                    idx % 10,
                    idx % 100,
                    idx % 1000,
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
        }

        // Wait for data to be indexed
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
}

/// Benchmark streaming query performance
fn bench_streaming_query(c: &mut Criterion) {
    if !influxdb_available() {
        eprintln!("Skipping benchmarks: InfluxDB not available");
        eprintln!("Start InfluxDB with: docker-compose up -d");
        return;
    }

    let rt = Runtime::new().unwrap();

    // Test with different data sizes
    let sizes = [1_000, 10_000, 50_000];

    let mut group = c.benchmark_group("streaming_query");
    group.sample_size(10); // Reduce sample size for large datasets
    group.measurement_time(Duration::from_secs(30));

    for size in sizes {
        let measurement = format!("bench_{}", size);

        // Setup: write test data
        println!("Setting up {} records for benchmark...", size);
        write_benchmark_data(&measurement, size);

        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("records", size),
            &size,
            |b, &_size| {
                b.to_async(&rt).iter(|| async {
                    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
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
    }

    group.finish();
}

/// Benchmark memory efficiency by measuring peak allocation
fn bench_memory_efficiency(c: &mut Criterion) {
    if !influxdb_available() {
        return;
    }

    let rt = Runtime::new().unwrap();

    // Prepare large dataset
    let measurement = "bench_memory";
    let size = 50_000;

    println!("Setting up {} records for memory benchmark...", size);
    write_benchmark_data(measurement, size);

    let mut group = c.benchmark_group("memory_efficiency");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    group.bench_function("50k_records_streaming", |b| {
        b.to_async(&rt).iter(|| async {
            let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
            let query = format!(
                r#"from(bucket: "{}")
                   |> range(start: 2023-01-01T00:00:00Z)
                   |> filter(fn: (r) => r._measurement == "{}")"#,
                INFLUXDB_BUCKET, measurement
            );

            let mut stream = client.query_stream(&query).await.unwrap();

            // Process all records without storing them
            let mut count = 0;
            let mut sum: f64 = 0.0;
            while let Some(result) = stream.next().await {
                let record = result.unwrap();
                if let Some(v) = record.get_double("_value") {
                    sum += v;
                }
                count += 1;
            }
            (count, sum)
        });
    });

    group.finish();
}

/// Benchmark first-byte latency (time to first record)
fn bench_first_byte_latency(c: &mut Criterion) {
    if !influxdb_available() {
        return;
    }

    let rt = Runtime::new().unwrap();

    // Prepare dataset - use unique name to avoid conflicts
    let measurement = "bench_latency_test";
    println!("Setting up {} records for latency benchmark...", 1000);
    write_benchmark_data(measurement, 1000);

    // Extra wait for data to be indexed
    rt.block_on(async {
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    let mut group = c.benchmark_group("latency");
    group.sample_size(20);

    group.bench_function("time_to_first_record", |b| {
        b.to_async(&rt).iter(|| async {
            let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
            let query = format!(
                r#"from(bucket: "{}")
                   |> range(start: 2023-01-01T00:00:00Z)
                   |> filter(fn: (r) => r._measurement == "{}")"#,
                INFLUXDB_BUCKET, measurement
            );

            let mut stream = client.query_stream(&query).await.unwrap();

            // Just get the first record
            match stream.next().await {
                Some(Ok(record)) => record,
                Some(Err(e)) => panic!("Error reading record: {}", e),
                None => panic!("No records found for measurement {}", measurement),
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_streaming_query,
    bench_memory_efficiency,
    bench_first_byte_latency,
);

criterion_main!(benches);

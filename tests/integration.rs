//! Integration tests for influxdb-stream.
//!
//! These tests require a running InfluxDB instance.
//! Start one with: `docker-compose up -d`
//!
//! Run tests with: `cargo test --test integration`

use futures::StreamExt;
use influxdb_stream::Client;
use std::time::Duration;

// Test configuration - matches docker-compose.yml
const INFLUXDB_URL: &str = "http://localhost:8086";
const INFLUXDB_ORG: &str = "test-org";
const INFLUXDB_TOKEN: &str = "test-token-for-development-only";
const INFLUXDB_BUCKET: &str = "test-bucket";

/// Helper to check if InfluxDB is available
async fn influxdb_available() -> bool {
    let client = reqwest::Client::new();
    client
        .get(format!("{}/health", INFLUXDB_URL))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Helper to write test data using Line Protocol
async fn write_test_data(lines: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/v2/write?org={}&bucket={}&precision=ms",
        INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_BUCKET
    );

    let response = client
        .post(&url)
        .header("Authorization", format!("Token {}", INFLUXDB_TOKEN))
        .header("Content-Type", "text/plain")
        .body(lines.to_string())
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(format!("Write failed: {} - {}", status, body).into());
    }

    Ok(())
}

/// Helper to delete all data in bucket
async fn clear_bucket() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/v2/delete?org={}&bucket={}",
        INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_BUCKET
    );

    let body = serde_json::json!({
        "start": "1970-01-01T00:00:00Z",
        "stop": "2100-01-01T00:00:00Z"
    });

    client
        .post(&url)
        .header("Authorization", format!("Token {}", INFLUXDB_TOKEN))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    Ok(())
}

/// Generate Line Protocol for N data points
fn generate_line_protocol(measurement: &str, count: usize) -> String {
    let base_ts = 1700000000000i64; // 2023-11-14
    let mut lines = Vec::with_capacity(count);

    for i in 0..count {
        let ts = base_ts + (i as i64 * 1000); // 1 second apart
        lines.push(format!(
            "{},host=server{},region=us-east value={}.{} {}",
            measurement,
            i % 10,       // 10 different hosts
            i % 100,      // value integer part
            i % 1000,     // value decimal part
            ts
        ));
    }

    lines.join("\n")
}

// ============================================================================
// Basic Integration Tests
// ============================================================================

#[tokio::test]
async fn test_basic_query_stream() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    // Clear and write test data
    clear_bucket().await.unwrap();
    let lines = generate_line_protocol("temperature", 100);
    write_test_data(&lines).await.unwrap();

    // Wait for data to be queryable
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query with streaming
    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
    let query = format!(
        r#"from(bucket: "{}")
           |> range(start: 2023-01-01T00:00:00Z)
           |> filter(fn: (r) => r._measurement == "temperature")"#,
        INFLUXDB_BUCKET
    );

    let mut stream = client.query_stream(&query).await.unwrap();

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let record = result.expect("Failed to parse record");
        assert!(record.measurement().is_some());
        assert!(record.time().is_some());
        count += 1;
    }

    assert_eq!(count, 100, "Expected 100 records, got {}", count);
}

#[tokio::test]
async fn test_empty_result() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    clear_bucket().await.unwrap();

    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
    let query = format!(
        r#"from(bucket: "{}")
           |> range(start: 2023-01-01T00:00:00Z)
           |> filter(fn: (r) => r._measurement == "nonexistent")"#,
        INFLUXDB_BUCKET
    );

    let mut stream = client.query_stream(&query).await.unwrap();

    let mut count = 0;
    while let Some(result) = stream.next().await {
        result.expect("Failed to parse record");
        count += 1;
    }

    assert_eq!(count, 0, "Expected 0 records for nonexistent measurement");
}

#[tokio::test]
async fn test_multiple_tables() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    // Use unique measurement names to avoid conflicts with other tests
    let lines1 = generate_line_protocol("multi_cpu", 50);
    let lines2 = generate_line_protocol("multi_memory", 50);
    write_test_data(&lines1).await.unwrap();
    write_test_data(&lines2).await.unwrap();

    // Wait for data to be indexed
    tokio::time::sleep(Duration::from_secs(1)).await;

    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
    let query = format!(
        r#"from(bucket: "{}")
           |> range(start: 2023-01-01T00:00:00Z)
           |> filter(fn: (r) => r._measurement == "multi_cpu" or r._measurement == "multi_memory")"#,
        INFLUXDB_BUCKET
    );

    let mut stream = client.query_stream(&query).await.unwrap();

    let mut cpu_count = 0;
    let mut memory_count = 0;

    while let Some(result) = stream.next().await {
        let record = result.expect("Failed to parse record");
        match record.measurement().as_deref() {
            Some("multi_cpu") => cpu_count += 1,
            Some("multi_memory") => memory_count += 1,
            other => panic!("Unexpected measurement: {:?}", other),
        }
    }

    assert_eq!(cpu_count, 50, "Expected 50 cpu records");
    assert_eq!(memory_count, 50, "Expected 50 memory records");
}

#[tokio::test]
async fn test_query_collect() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    clear_bucket().await.unwrap();
    let lines = generate_line_protocol("test_collect", 25);
    write_test_data(&lines).await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
    let query = format!(
        r#"from(bucket: "{}")
           |> range(start: 2023-01-01T00:00:00Z)
           |> filter(fn: (r) => r._measurement == "test_collect")"#,
        INFLUXDB_BUCKET
    );

    // Use the non-streaming query method
    let records = client.query(&query).await.unwrap();

    assert_eq!(records.len(), 25, "Expected 25 records");
}

// ============================================================================
// Large Dataset Tests
// ============================================================================

#[tokio::test]
async fn test_large_dataset_10k() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    clear_bucket().await.unwrap();

    // Write 10,000 data points
    let lines = generate_line_protocol("large_test", 10_000);
    write_test_data(&lines).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
    let query = format!(
        r#"from(bucket: "{}")
           |> range(start: 2023-01-01T00:00:00Z)
           |> filter(fn: (r) => r._measurement == "large_test")"#,
        INFLUXDB_BUCKET
    );

    let start = std::time::Instant::now();
    let mut stream = client.query_stream(&query).await.unwrap();

    let mut count = 0;
    while let Some(result) = stream.next().await {
        result.expect("Failed to parse record");
        count += 1;
    }
    let elapsed = start.elapsed();

    println!(
        "Processed {} records in {:?} ({:.0} records/sec)",
        count,
        elapsed,
        count as f64 / elapsed.as_secs_f64()
    );

    assert_eq!(count, 10_000, "Expected 10,000 records, got {}", count);
}

#[tokio::test]
#[ignore] // Run with: cargo test --test integration test_large_dataset_100k -- --ignored
async fn test_large_dataset_100k() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    clear_bucket().await.unwrap();

    // Write 100,000 data points in batches
    println!("Writing 100,000 data points...");
    for batch in 0..10 {
        let lines = generate_line_protocol(&format!("large_test_{}", batch), 10_000);
        write_test_data(&lines).await.unwrap();
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
    let query = format!(
        r#"from(bucket: "{}")
           |> range(start: 2023-01-01T00:00:00Z)"#,
        INFLUXDB_BUCKET
    );

    println!("Querying...");
    let start = std::time::Instant::now();
    let mut stream = client.query_stream(&query).await.unwrap();

    let mut count = 0;
    while let Some(result) = stream.next().await {
        result.expect("Failed to parse record");
        count += 1;
        if count % 10_000 == 0 {
            println!("  Processed {} records...", count);
        }
    }
    let elapsed = start.elapsed();

    println!(
        "Processed {} records in {:?} ({:.0} records/sec)",
        count,
        elapsed,
        count as f64 / elapsed.as_secs_f64()
    );

    assert_eq!(count, 100_000, "Expected 100,000 records, got {}", count);
}

// ============================================================================
// Data Type Tests
// ============================================================================

#[tokio::test]
async fn test_various_data_types() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    clear_bucket().await.unwrap();

    // Write data with various field types
    let lines = r#"types,tag=test int_field=42i,float_field=2.72,bool_field=true,string_field="hello" 1700000000000"#;
    write_test_data(lines).await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);
    let query = format!(
        r#"from(bucket: "{}")
           |> range(start: 2023-01-01T00:00:00Z)
           |> filter(fn: (r) => r._measurement == "types")"#,
        INFLUXDB_BUCKET
    );

    let mut stream = client.query_stream(&query).await.unwrap();

    let mut found_int = false;
    let mut found_float = false;
    let mut found_bool = false;
    let mut found_string = false;

    while let Some(result) = stream.next().await {
        let record = result.expect("Failed to parse record");
        match record.field().as_deref() {
            Some("int_field") => {
                assert_eq!(record.get_long("_value"), Some(42));
                found_int = true;
            }
            Some("float_field") => {
                let val = record.get_double("_value").unwrap();
                assert!((val - 2.72).abs() < 0.001);
                found_float = true;
            }
            Some("bool_field") => {
                assert_eq!(record.get_bool("_value"), Some(true));
                found_bool = true;
            }
            Some("string_field") => {
                assert_eq!(record.get_string("_value"), Some("hello".to_string()));
                found_string = true;
            }
            _ => {}
        }
    }

    assert!(found_int, "int_field not found");
    assert!(found_float, "float_field not found");
    assert!(found_bool, "bool_field not found");
    assert!(found_string, "string_field not found");
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_invalid_query() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);

    // Invalid Flux syntax
    let result = client.query_stream("this is not valid flux").await;

    assert!(result.is_err(), "Expected error for invalid query");
}

#[tokio::test]
async fn test_nonexistent_bucket() {
    if !influxdb_available().await {
        eprintln!("Skipping test: InfluxDB not available");
        return;
    }

    let client = Client::new(INFLUXDB_URL, INFLUXDB_ORG, INFLUXDB_TOKEN);

    let query = r#"from(bucket: "nonexistent-bucket-12345")
                   |> range(start: -1h)"#;

    let result = client.query_stream(query).await;

    // InfluxDB returns an error for nonexistent buckets
    assert!(result.is_err(), "Expected error for nonexistent bucket");
}

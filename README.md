# influxdb-stream

[![Crates.io](https://img.shields.io/crates/v/influxdb-stream.svg)](https://crates.io/crates/influxdb-stream)
[![Documentation](https://docs.rs/influxdb-stream/badge.svg)](https://docs.rs/influxdb-stream)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Async streaming client for InfluxDB 2.x** — Query millions of time-series rows without running out of memory.

> 💡 Other Rust clients are async but load all results into `Vec<T>`. This crate is the first to offer true record-by-record streaming.

## The Problem

Existing Rust InfluxDB clients load entire query results into memory:

```rust
// Using existing crates - this will OOM with large datasets!
let results: Vec<MyData> = client.query(query).await?;
```

When you're dealing with time-series data spanning months or years, with millions of data points, this approach simply doesn't work.

## The Solution

`influxdb-stream` streams results one record at a time:

```rust
// Process millions of rows with constant memory usage
let mut stream = client.query_stream(query).await?;
while let Some(record) = stream.next().await {
    process(record?);
}
```

## Comparison with Existing Crates

| Crate | InfluxDB Version | Async | Streaming | Memory Efficient |
|-------|------------------|-------|-----------|------------------|
| `influxdb` | 1.x | ✅ | ❌ | ❌ |
| `influxdb2` | 2.x | ✅ | ❌ | ❌ |
| `influx_db_client` | 1.x/2.x | ✅ | ❌ | ❌ |
| **`influxdb-stream`** | **2.x** | **✅** | **✅** | **✅** |

> All crates are async, but return `Vec<T>`. Only `influxdb-stream` streams record-by-record.

## Benchmark Results

Measured on MacBook Pro (M-series), InfluxDB 2.7 running locally.

### Throughput Comparison: influxdb-stream vs influxdb2

| Records | influxdb-stream | influxdb2 | Improvement |
|---------|-----------------|-----------|-------------|
| 1,000 | 166K rec/s | 132K rec/s | **+26%** |
| 5,000 | 387K rec/s | 276K rec/s | **+40%** |
| 10,000 | 456K rec/s | 334K rec/s | **+37%** |

### Memory Usage Comparison (System RSS)

| Records | influxdb-stream | influxdb2 | Ratio | Memory Saved |
|---------|-----------------|-----------|-------|--------------|
| 10,000 | 0.95 MB | 13.03 MB | **13.7x** | 92.7% |
| 50,000 | 0.23 MB | 46.88 MB | **200x** | 99.5% |
| 100,000 | 0.34 MB | 71.69 MB | **208x** | 99.5% |
| 200,000 | 0.06 MB | 137.84 MB | **2205x** | ~100% |

**Key advantages:**
- **26-40% faster** than influxdb2 crate
- **O(1) memory**: Constant memory regardless of result set size
- **200-2000x less memory** for large datasets
- **Streaming mode**: Process records without storing them (influxdb2 can't do this)

Run benchmarks yourself:
```bash
cargo bench --bench comparison              # Throughput comparison
cargo run --release --example memory_comparison  # Memory comparison
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
influxdb-stream = "0.1"
futures = "0.3"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Quick Start

```rust
use influxdb_stream::Client;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let client = Client::new(
        "http://localhost:8086",
        "my-org",
        "my-token"
    );

    // Execute a streaming query
    let mut stream = client.query_stream(r#"
        from(bucket: "sensors")
        |> range(start: -30d)
        |> filter(fn: (r) => r._measurement == "temperature")
    "#).await?;

    // Process records one at a time
    let mut count = 0;
    while let Some(record) = stream.next().await {
        let record = record?;
        println!(
            "{}: {} = {}",
            record.time().map(|t| t.to_rfc3339()).unwrap_or_default(),
            record.field().unwrap_or_default(),
            record.value().map(|v| v.to_string()).unwrap_or_default()
        );
        count += 1;
    }
    println!("Processed {} records", count);

    Ok(())
}
```

## API Overview

### Client

```rust
// Create with default HTTP client
let client = Client::new(url, org, token);

// Create with custom reqwest client (for timeouts, proxies, etc.)
let http = reqwest::Client::builder()
    .timeout(Duration::from_secs(300))
    .build()?;
let client = Client::with_http_client(http, url, org, token);
```

### Streaming Query

```rust
// Returns a Stream of Result<FluxRecord>
let mut stream = client.query_stream("from(bucket: \"test\") |> range(start: -1h)").await?;

while let Some(result) = stream.next().await {
    let record: FluxRecord = result?;
    // Process record...
}
```

### Collect All (for small datasets)

```rust
// Warning: loads all results into memory
let records: Vec<FluxRecord> = client.query("...").await?;
```

### FluxRecord

```rust
let record: FluxRecord = ...;

// Common accessors
record.time()         // Option<&DateTime<FixedOffset>>
record.measurement()  // Option<String>
record.field()        // Option<String>
record.value()        // Option<&Value>

// Generic accessors
record.get("column_name")        // Option<&Value>
record.get_string("tag_name")    // Option<String>
record.get_double("_value")      // Option<f64>
record.get_long("_value")        // Option<i64>
record.get_bool("flag")          // Option<bool>
```

### Value Types

All InfluxDB data types are supported:

| InfluxDB Type | Rust Type |
|---------------|-----------|
| `string` | `Value::String(String)` |
| `double` | `Value::Double(OrderedFloat<f64>)` |
| `boolean` | `Value::Bool(bool)` |
| `long` | `Value::Long(i64)` |
| `unsignedLong` | `Value::UnsignedLong(u64)` |
| `duration` | `Value::Duration(chrono::Duration)` |
| `base64Binary` | `Value::Base64Binary(Vec<u8>)` |
| `dateTime:RFC3339` | `Value::TimeRFC(DateTime<FixedOffset>)` |

## Use Cases

### Data Migration

```rust
let mut stream = source_client.query_stream(r#"
    from(bucket: "old-bucket")
    |> range(start: 2020-01-01T00:00:00Z, stop: 2024-01-01T00:00:00Z)
"#).await?;

while let Some(record) = stream.next().await {
    let record = record?;
    let point = transform_to_line_protocol(&record);
    dest_client.write(&point).await?;
}
```

### ETL Pipeline

```rust
let mut stream = client.query_stream("...").await?;

while let Some(record) = stream.next().await {
    let record = record?;
    let transformed = transform(record);
    kafka_producer.send(transformed).await?;
}
```

### Real-time Analytics

```rust
let mut stream = client.query_stream(r#"
    from(bucket: "metrics")
    |> range(start: -5m)
    |> aggregateWindow(every: 1s, fn: mean)
"#).await?;

let mut stats = RollingStats::new();
while let Some(record) = stream.next().await {
    stats.update(record?.get_double("_value").unwrap_or(0.0));
}
println!("Mean: {}, Std: {}", stats.mean(), stats.std());
```

## How It Works

```
InfluxDB HTTP API (/api/v2/query)
        ↓ (Accept: application/csv)
    reqwest bytes_stream()
        ↓
    tokio_util::StreamReader
        ↓
    Annotated CSV State Machine Parser
        ↓
    Stream<Item = Result<FluxRecord>>
```

The parser processes InfluxDB's [annotated CSV format](https://docs.influxdata.com/influxdb/cloud/reference/syntax/annotated-csv/) line by line, never buffering more than a single row at a time.

## Roadmap

- [x] **v0.1.0** - Query Streaming
  - [x] Flux query execution with streaming results
  - [x] Memory-efficient processing
  - [x] All InfluxDB data types
  - [x] Comprehensive error handling

- [ ] **v0.2.0** - Write Streaming
  - [ ] `Stream<Item = DataPoint>` → InfluxDB streaming write
  - [ ] Line Protocol conversion with chunked transfer
  - [ ] Backpressure handling

- [ ] **v0.3.0** - Arrow Flight (InfluxDB 3.x)
  - [ ] Arrow Flight SQL protocol support
  - [ ] `RecordBatch` streaming
  - [ ] Feature flags for v2/v3

## Requirements

- Rust 1.85+ (Edition 2024)
- InfluxDB 2.x server

## Development

### Running Tests

Tests require a running InfluxDB instance. Use Docker Compose to start one:

```bash
# Start InfluxDB
docker-compose up -d

# Run all tests (unit + integration)
./scripts/test-local.sh

# Or run manually:
cargo test --lib                    # Unit tests only
cargo test --test integration       # Integration tests only
```

### Running Benchmarks

Benchmarks measure streaming performance with various dataset sizes:

```bash
# Run benchmarks
./scripts/bench-local.sh

# Or run manually:
cargo bench

# Results are saved to target/criterion/
# Open target/criterion/report/index.html for graphs
```

### Test Coverage

| Category | Tests | Description |
|----------|-------|-------------|
| Unit | 5 | Value parsing for each data type |
| Integration | 8 | End-to-end with real InfluxDB |
| Large Dataset | 2 | 10K and 100K record streaming |
| Benchmark | 3 | Throughput, memory, latency |

## License

This project is licensed under the [MIT License](LICENSE).

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Acknowledgments

This project has been battle-tested in production, processing billions of time-series data points for industrial IoT applications.

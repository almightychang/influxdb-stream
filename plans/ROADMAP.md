# influxdb-stream Roadmap

> Last updated: 2025-12-22

---

## Overview

Rust 생태계의 InfluxDB 클라이언트들은 쿼리 결과를 전부 메모리에 로드한다. `influxdb-stream`은 진정한 async streaming을 지원하는 최초의 Rust 크레이트다.

---

## v0.1.0 - Query Streaming ✅ Released

- [x] Flux 쿼리 실행 후 `Stream<Item = Result<Record>>`로 결과 반환
- [x] 메모리 효율: 전체 결과를 메모리에 올리지 않음
- [x] 모든 InfluxDB 데이터 타입 지원
- [x] `influxdb2` 크레이트 의존 없이 자체 타입 정의
- [x] panic 없이 모든 에러를 Result로 전파
- [x] 벤치마크: influxdb2 대비 26-40% 빠름, 200-2000x 적은 메모리

---

## v0.2.0 - Write Streaming

- [ ] `Stream<Item = DataPoint>` → InfluxDB로 streaming write
- [ ] Line Protocol로 변환하면서 청크 단위 전송
- [ ] Backpressure 처리

### Technical Approach

```
Stream<Item = DataPoint>
        ↓
    Line Protocol 변환 (Iterator)
        ↓
    reqwest Body::wrap_stream()
        ↓
    InfluxDB HTTP API (/api/v2/write)
```

### Target API

```rust
let source_stream = client.query_stream("...").await?;

client.write_stream(
    "target-bucket",
    source_stream.map(|r| transform(r))
).await?;
```

---

## v0.3.0 - Arrow Flight (InfluxDB 3.x)

- [ ] Arrow Flight SQL 프로토콜 지원
- [ ] `RecordBatch` 스트리밍
- [ ] Feature flag로 분리 (`v2`, `v3`)

### Technical Approach

```
arrow-flight 크레이트
        ↓
    FlightSqlServiceClient
        ↓
    Stream<Item = RecordBatch>
```

### Feature Flags

```toml
[features]
default = ["v2"]
v2 = ["reqwest", "csv-async"]
v3 = ["arrow-flight", "tonic"]
```

### Target API

```rust
use influxdb_stream::v3::FlightClient;

let client = FlightClient::new("http://localhost:8086", "my-token");
let mut stream = client.query_stream("SELECT * FROM cpu").await?;

while let Some(batch) = stream.next().await {
    let batch: RecordBatch = batch?;
    // Arrow 네이티브로 polars/datafusion과 바로 연동
}
```

---

## Won't Have

- 동기(blocking) API - async 전용
- InfluxDB 1.x 전용 기능

---

## References

- [InfluxDB Annotated CSV Spec](https://docs.influxdata.com/influxdb/cloud/reference/syntax/annotated-csv/)
- [arrow-flight crate](https://crates.io/crates/arrow-flight)
- [influxdb2 crate](https://crates.io/crates/influxdb2)

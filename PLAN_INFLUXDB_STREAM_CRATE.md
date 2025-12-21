# influxdb-stream 개발 계획

> 작성일: 2025-12-21

---

## 1. 왜 만들어야 하는가?

### 문제 상황

현재 Rust 생태계의 InfluxDB 클라이언트들은 **쿼리 결과를 전부 메모리에 로드**한다.

```rust
// influxdb2 크레이트의 현재 방식
let results: Vec<MyData> = client.query::<MyData>(query).await?;
```

수백만 건의 시계열 데이터를 쿼리하면 OOM이 발생한다. 대용량 데이터 마이그레이션, ETL 파이프라인, 실시간 분석 등에서 치명적인 한계다.

### 기존 크레이트 조사 결과 (2025-12-21)

| 크레이트 | 버전 | Async | Streaming |
|---------|------|-------|-----------|
| `influxdb` | 1.x | ✅ | ❌ |
| `influxdb2` | 2.x | ✅ | ❌ |
| `influx_db_client` | 1.x/2.x | ✅ | ❌ |
| InfluxDB 3 공식 | 3.x | - | **Rust 클라이언트 없음** |

**결론: 진정한 async streaming을 지원하는 Rust 크레이트가 없다.**

### 우리가 이미 가진 것

`data-agent-rs` 프로젝트에서 대용량 마이그레이션을 위해 직접 구현한 streaming query 클라이언트:

- HTTP response를 `bytes_stream()`으로 받아서
- `tokio_util::StreamReader`로 감싸고
- InfluxDB annotated CSV를 한 줄씩 async하게 파싱
- `Stream<Item = FluxRecord>`로 yield

**이 코드를 정리해서 오픈소스로 공개하면 Rust 생태계의 gap을 메울 수 있다.**

---

## 2. 로드맵

### v0.1.0 - Query Streaming

- [ ] Flux 쿼리 실행 후 `Stream<Item = Result<Record>>`로 결과 반환
- [ ] 메모리 효율: 전체 결과를 메모리에 올리지 않음
- [ ] 모든 InfluxDB 데이터 타입 지원
- [ ] `influxdb2` 크레이트 의존 없이 자체 타입 정의
- [ ] panic 없이 모든 에러를 Result로 전파

### v0.2.0 - Write Streaming

- [ ] `Stream<Item = DataPoint>` → InfluxDB로 streaming write
- [ ] Line Protocol로 변환하면서 청크 단위 전송
- [ ] Backpressure 처리

### v0.3.0 - Arrow Flight (InfluxDB 3)

- [ ] Arrow Flight SQL 프로토콜 지원
- [ ] `RecordBatch` 스트리밍
- [ ] Feature flag로 분리 (`v2`, `v3`)

### Won't Have

- 동기(blocking) API - async 전용
- InfluxDB 1.x 전용 기능

---

## 3. 기술적 접근

### Query Streaming (v0.1)

```
InfluxDB HTTP API (/api/v2/query)
        ↓ (Accept: application/csv)
    reqwest bytes_stream()
        ↓
    tokio_util::StreamReader
        ↓
    Annotated CSV State Machine Parser
        ↓
    async_stream! { yield FluxRecord }
```

### Write Streaming (v0.2)

```
Stream<Item = DataPoint>
        ↓
    Line Protocol 변환 (Iterator)
        ↓
    reqwest Body::wrap_stream()
        ↓
    InfluxDB HTTP API (/api/v2/write)
```

- InfluxDB Write API는 Line Protocol을 HTTP body로 받음
- `reqwest`는 `Body::wrap_stream()`으로 streaming body 지원
- Record → Line Protocol 변환하면서 청크 단위로 전송 가능

### Arrow Flight (v0.3)

```
arrow-flight 크레이트
        ↓
    FlightSqlServiceClient
        ↓
    Stream<Item = RecordBatch>
```

Feature flags로 분리:
```toml
[features]
default = ["v2"]
v2 = ["reqwest", "csv-async"]
v3 = ["arrow-flight", "tonic"]
```

---

## 4. 오픈소스 성공 요소

### 기술적 완성도

- [ ] MSRV (Minimum Supported Rust Version) 정책 명시
- [ ] 기존 크레이트 대비 메모리 사용량 벤치마크
- [ ] 네트워크 끊김, 빈 결과 등 엣지 케이스 처리

### 문서화

- [ ] README에서 "왜 필요한지" 명확히 전달
- [ ] 기존 크레이트와의 비교표
- [ ] 시나리오별 예제 (마이그레이션, ETL, 실시간 처리)

### 유지보수

- [ ] SemVer 준수, CHANGELOG 유지
- [ ] CI에서 InfluxDB docker로 integration test
- [ ] MIT + Apache 2.0 듀얼 라이선스

---

## 5. 목표 사용 경험

### Query Streaming (v0.1)

```rust
use influxdb_stream::Client;
use futures::StreamExt;

let client = Client::new("http://localhost:8086", "my-org", "my-token");

let mut stream = client.query_stream(r#"
    from(bucket: "sensors")
    |> range(start: -30d)
    |> filter(fn: (r) => r._measurement == "temperature")
"#).await?;

while let Some(record) = stream.next().await {
    let record = record?;
    process(record);
}
```

### Write Streaming (v0.2)

```rust
let source_stream = client.query_stream("...").await?;

client.write_stream(
    "target-bucket",
    source_stream.map(|r| transform(r))
).await?;
```

### Arrow Flight (v0.3)

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

## 6. 참고

### 현재 코드 위치

- Query streaming 구현: `src/agents/migrate-influxdb-to-influxdb/src/query.rs`
- 사용 예시: `src/agents/migrate-influxdb-to-influxdb/src/main.rs`

### 추출 대상

- `QueryClient` - HTTP 클라이언트
- `AsyncQueryTableResult` - Annotated CSV 파서 (state machine)
- `DataType`, `FluxColumn` - 타입 정의

### 외부 참고

- [InfluxDB Annotated CSV 스펙](https://docs.influxdata.com/influxdb/cloud/reference/syntax/annotated-csv/)
- [arrow-flight 크레이트](https://crates.io/crates/arrow-flight)
- [influxdb2 크레이트](https://crates.io/crates/influxdb2)

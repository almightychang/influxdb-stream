# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-12-23

Initial release.

### Added

- `Client` for connecting to InfluxDB 2.x
  - `query_stream()` - streaming query returning `Stream<Item = Result<FluxRecord>>`
  - `query()` - convenience method that collects all results into `Vec<FluxRecord>`
  - `with_http_client()` - custom `reqwest::Client` support
- `AnnotatedCsvParser` for parsing InfluxDB's annotated CSV format
  - State machine parser processing one row at a time
  - O(1) memory usage regardless of result set size
- `FluxRecord` with typed accessors
  - `time()`, `measurement()`, `field()`, `value()`
  - `get()`, `get_string()`, `get_double()`, `get_long()`, `get_bool()`
- `Value` enum supporting all InfluxDB data types
  - `String`, `Double`, `Bool`, `Long`, `UnsignedLong`
  - `Duration`, `Base64Binary`, `TimeRFC`
- Comprehensive error handling via `Error` enum

[0.1.0]: https://github.com/aniai-dev/influxdb-stream/releases/tag/v0.1.0

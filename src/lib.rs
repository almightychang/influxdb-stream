//! # influxdb-stream
//!
//! Async streaming client for InfluxDB 2.x that lets you query millions of rows
//! without running out of memory.
//!
//! ## Why?
//!
//! Existing Rust InfluxDB clients load entire query results into memory:
//!
//! ```ignore
//! // This will OOM with millions of rows!
//! let results: Vec<MyData> = client.query(query).await?;
//! ```
//!
//! `influxdb-stream` streams results one record at a time:
//!
//! ```ignore
//! // Process millions of rows with constant memory usage
//! let mut stream = client.query_stream(query).await?;
//! while let Some(record) = stream.next().await {
//!     process(record?);
//! }
//! ```
//!
//! ## Quick Start
//!
//! ```ignore
//! use influxdb_stream::Client;
//! use futures::StreamExt;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = Client::new("http://localhost:8086", "my-org", "my-token");
//!
//!     let mut stream = client.query_stream(r#"
//!         from(bucket: "sensors")
//!         |> range(start: -30d)
//!         |> filter(fn: (r) => r._measurement == "temperature")
//!     "#).await?;
//!
//!     while let Some(record) = stream.next().await {
//!         let record = record?;
//!         println!(
//!             "{}: {} = {:?}",
//!             record.measurement().unwrap_or_default(),
//!             record.field().unwrap_or_default(),
//!             record.value()
//!         );
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Features
//!
//! - **Memory efficient**: Streams results without loading everything into memory
//! - **Async native**: Built on tokio and futures
//! - **All data types**: Supports all InfluxDB data types (string, double, bool,
//!   long, unsignedLong, duration, base64Binary, dateTime:RFC3339)
//! - **Error handling**: All errors are returned as Results, no panics
//! - **Zero copy parsing**: Parses InfluxDB's annotated CSV format on the fly

pub mod client;
pub mod error;
pub mod parser;
pub mod types;
pub mod value;

// Re-export main types at crate root
pub use client::Client;
pub use error::{Error, Result};
pub use types::{DataType, FluxColumn, FluxRecord, FluxTableMetadata};
pub use value::Value;

// Re-export parser for advanced use cases
pub use parser::AnnotatedCsvParser;

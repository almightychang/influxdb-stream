//! Error types for influxdb-stream.

use thiserror::Error;

/// Error type for influxdb-stream operations.
#[derive(Error, Debug)]
pub enum Error {
    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// Failed to serialize query to JSON.
    #[error("Failed to serialize query: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Failed to parse CSV data.
    #[error("CSV parse error: {0}")]
    Csv(String),

    /// Failed to parse a value from the response.
    #[error("Failed to parse value: {message}")]
    Parse {
        /// Description of what failed to parse.
        message: String,
    },

    /// Unknown data type in annotated CSV.
    #[error("Unknown data type: {0}")]
    UnknownDataType(String),

    /// Missing required annotation in CSV.
    #[error("Missing annotation: {0}")]
    MissingAnnotation(String),

    /// Row has different number of columns than expected.
    #[error("Column count mismatch: expected {expected}, got {actual}")]
    ColumnMismatch {
        /// Expected number of columns.
        expected: usize,
        /// Actual number of columns found.
        actual: usize,
    },

    /// Query returned an error from InfluxDB.
    #[error("Query error from InfluxDB: {message}")]
    QueryError {
        /// Error message returned by InfluxDB.
        message: String,
        /// Optional reference link for debugging.
        reference: Option<String>,
    },

    /// I/O error during streaming.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for influxdb-stream operations.
pub type Result<T> = std::result::Result<T, Error>;

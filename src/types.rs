//! Core types for InfluxDB Flux query results.

use std::collections::BTreeMap;
use std::str::FromStr;

use crate::error::Error;
use crate::value::Value;

/// Data types supported in InfluxDB annotated CSV.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    /// String data type.
    String,
    /// 64-bit floating point.
    Double,
    /// Boolean value.
    Bool,
    /// Signed 64-bit integer.
    Long,
    /// Unsigned 64-bit integer.
    UnsignedLong,
    /// Duration (Go-style, e.g., "1h30m").
    Duration,
    /// Base64-encoded binary data.
    Base64Binary,
    /// RFC3339 timestamp (with optional nanosecond precision).
    TimeRFC,
}

impl FromStr for DataType {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "string" => Ok(Self::String),
            "double" => Ok(Self::Double),
            "boolean" => Ok(Self::Bool),
            "long" => Ok(Self::Long),
            "unsignedLong" => Ok(Self::UnsignedLong),
            "duration" => Ok(Self::Duration),
            "base64Binary" => Ok(Self::Base64Binary),
            "dateTime:RFC3339" | "dateTime:RFC3339Nano" => Ok(Self::TimeRFC),
            _ => Err(Error::UnknownDataType(input.to_string())),
        }
    }
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DataType::String => "string",
            DataType::Double => "double",
            DataType::Bool => "boolean",
            DataType::Long => "long",
            DataType::UnsignedLong => "unsignedLong",
            DataType::Duration => "duration",
            DataType::Base64Binary => "base64Binary",
            DataType::TimeRFC => "dateTime:RFC3339",
        };
        write!(f, "{}", s)
    }
}

/// Metadata for a column in a Flux table.
#[derive(Clone, Debug)]
pub struct FluxColumn {
    /// Column name.
    pub name: String,
    /// Data type of the column.
    pub data_type: DataType,
    /// Whether this column is part of the group key.
    pub group: bool,
    /// Default value for missing entries.
    pub default_value: String,
}

impl FluxColumn {
    /// Create a new FluxColumn with default values.
    pub fn new() -> Self {
        Self {
            name: String::new(),
            data_type: DataType::String,
            group: false,
            default_value: String::new(),
        }
    }
}

impl Default for FluxColumn {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata for a Flux table (one result set from a query).
#[derive(Clone, Debug)]
pub struct FluxTableMetadata {
    /// Table position/index in the query results.
    pub position: i32,
    /// Column definitions for this table.
    pub columns: Vec<FluxColumn>,
}

impl FluxTableMetadata {
    /// Create a new FluxTableMetadata with the given position and column count.
    pub fn new(position: i32, column_count: usize) -> Self {
        let columns = (0..column_count).map(|_| FluxColumn::new()).collect();
        Self { position, columns }
    }

    /// Get a column by name.
    pub fn column(&self, name: &str) -> Option<&FluxColumn> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// A single record (row) from a Flux query result.
#[derive(Clone, Debug)]
pub struct FluxRecord {
    /// Table index this record belongs to.
    pub table: i32,
    /// Column name to value mapping.
    pub values: BTreeMap<String, Value>,
}

impl FluxRecord {
    /// Create a new empty FluxRecord.
    pub fn new(table: i32) -> Self {
        Self {
            table,
            values: BTreeMap::new(),
        }
    }

    /// Get a value by column name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    /// Get value as string.
    pub fn get_string(&self, name: &str) -> Option<String> {
        self.values.get(name).and_then(|v| v.string())
    }

    /// Get value as f64.
    pub fn get_double(&self, name: &str) -> Option<f64> {
        self.values.get(name).and_then(|v| v.as_double())
    }

    /// Get value as i64.
    pub fn get_long(&self, name: &str) -> Option<i64> {
        self.values.get(name).and_then(|v| v.as_long())
    }

    /// Get value as bool.
    pub fn get_bool(&self, name: &str) -> Option<bool> {
        self.values.get(name).and_then(|v| v.as_bool())
    }

    /// Get the timestamp (_time field).
    pub fn time(&self) -> Option<&chrono::DateTime<chrono::FixedOffset>> {
        self.values.get("_time").and_then(|v| v.as_time())
    }

    /// Get the measurement name (_measurement field).
    pub fn measurement(&self) -> Option<String> {
        self.get_string("_measurement")
    }

    /// Get the field name (_field).
    pub fn field(&self) -> Option<String> {
        self.get_string("_field")
    }

    /// Get the field value (_value).
    pub fn value(&self) -> Option<&Value> {
        self.values.get("_value")
    }
}

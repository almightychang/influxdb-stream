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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Datelike};
    use ordered_float::OrderedFloat;

    // =========================================================================
    // DataType tests
    // =========================================================================

    #[test]
    fn test_datatype_from_str() {
        assert_eq!(DataType::from_str("string").unwrap(), DataType::String);
        assert_eq!(DataType::from_str("double").unwrap(), DataType::Double);
        assert_eq!(DataType::from_str("boolean").unwrap(), DataType::Bool);
        assert_eq!(DataType::from_str("long").unwrap(), DataType::Long);
        assert_eq!(
            DataType::from_str("unsignedLong").unwrap(),
            DataType::UnsignedLong
        );
        assert_eq!(DataType::from_str("duration").unwrap(), DataType::Duration);
        assert_eq!(
            DataType::from_str("base64Binary").unwrap(),
            DataType::Base64Binary
        );
        assert_eq!(
            DataType::from_str("dateTime:RFC3339").unwrap(),
            DataType::TimeRFC
        );
        assert_eq!(
            DataType::from_str("dateTime:RFC3339Nano").unwrap(),
            DataType::TimeRFC
        );
    }

    #[test]
    fn test_datatype_from_str_unknown() {
        let result = DataType::from_str("unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_datatype_display() {
        assert_eq!(DataType::String.to_string(), "string");
        assert_eq!(DataType::Double.to_string(), "double");
        assert_eq!(DataType::Bool.to_string(), "boolean");
        assert_eq!(DataType::Long.to_string(), "long");
        assert_eq!(DataType::UnsignedLong.to_string(), "unsignedLong");
        assert_eq!(DataType::Duration.to_string(), "duration");
        assert_eq!(DataType::Base64Binary.to_string(), "base64Binary");
        assert_eq!(DataType::TimeRFC.to_string(), "dateTime:RFC3339");
    }

    #[test]
    fn test_datatype_roundtrip() {
        // Parse and display should be consistent
        for type_str in [
            "string",
            "double",
            "boolean",
            "long",
            "unsignedLong",
            "duration",
            "base64Binary",
            "dateTime:RFC3339",
        ] {
            let dt = DataType::from_str(type_str).unwrap();
            assert_eq!(dt.to_string(), type_str);
        }
    }

    // =========================================================================
    // FluxColumn tests
    // =========================================================================

    #[test]
    fn test_flux_column_new() {
        let col = FluxColumn::new();
        assert_eq!(col.name, "");
        assert_eq!(col.data_type, DataType::String);
        assert!(!col.group);
        assert_eq!(col.default_value, "");
    }

    #[test]
    fn test_flux_column_default() {
        let col = FluxColumn::default();
        assert_eq!(col.name, "");
        assert_eq!(col.data_type, DataType::String);
        assert!(!col.group);
        assert_eq!(col.default_value, "");
    }

    // =========================================================================
    // FluxTableMetadata tests
    // =========================================================================

    #[test]
    fn test_flux_table_metadata_new() {
        let table = FluxTableMetadata::new(0, 3);
        assert_eq!(table.position, 0);
        assert_eq!(table.columns.len(), 3);
    }

    #[test]
    fn test_flux_table_metadata_column() {
        let mut table = FluxTableMetadata::new(0, 2);
        table.columns[0].name = "col1".to_string();
        table.columns[1].name = "col2".to_string();

        assert!(table.column("col1").is_some());
        assert_eq!(table.column("col1").unwrap().name, "col1");
        assert!(table.column("col2").is_some());
        assert!(table.column("nonexistent").is_none());
    }

    // =========================================================================
    // FluxRecord tests
    // =========================================================================

    #[test]
    fn test_flux_record_new() {
        let record = FluxRecord::new(5);
        assert_eq!(record.table, 5);
        assert!(record.values.is_empty());
    }

    #[test]
    fn test_flux_record_get() {
        let mut record = FluxRecord::new(0);
        record
            .values
            .insert("key".to_string(), Value::String("value".to_string()));

        assert!(record.get("key").is_some());
        assert_eq!(record.get("key"), Some(&Value::String("value".to_string())));
        assert!(record.get("nonexistent").is_none());
    }

    #[test]
    fn test_flux_record_get_string() {
        let mut record = FluxRecord::new(0);
        record
            .values
            .insert("name".to_string(), Value::String("alice".to_string()));
        record.values.insert("count".to_string(), Value::Long(42));

        assert_eq!(record.get_string("name"), Some("alice".to_string()));
        assert_eq!(record.get_string("count"), None); // Not a string
        assert_eq!(record.get_string("nonexistent"), None);
    }

    #[test]
    fn test_flux_record_get_double() {
        let mut record = FluxRecord::new(0);
        record
            .values
            .insert("value".to_string(), Value::Double(OrderedFloat::from(2.72)));
        record
            .values
            .insert("name".to_string(), Value::String("test".to_string()));

        assert_eq!(record.get_double("value"), Some(2.72));
        assert_eq!(record.get_double("name"), None); // Not a double
        assert_eq!(record.get_double("nonexistent"), None);
    }

    #[test]
    fn test_flux_record_get_long() {
        let mut record = FluxRecord::new(0);
        record.values.insert("count".to_string(), Value::Long(-42));
        record
            .values
            .insert("name".to_string(), Value::String("test".to_string()));

        assert_eq!(record.get_long("count"), Some(-42));
        assert_eq!(record.get_long("name"), None); // Not a long
        assert_eq!(record.get_long("nonexistent"), None);
    }

    #[test]
    fn test_flux_record_get_bool() {
        let mut record = FluxRecord::new(0);
        record.values.insert("flag".to_string(), Value::Bool(true));
        record
            .values
            .insert("name".to_string(), Value::String("test".to_string()));

        assert_eq!(record.get_bool("flag"), Some(true));
        assert_eq!(record.get_bool("name"), None); // Not a bool
        assert_eq!(record.get_bool("nonexistent"), None);
    }

    #[test]
    fn test_flux_record_time() {
        let mut record = FluxRecord::new(0);
        let dt = DateTime::parse_from_rfc3339("2023-11-14T12:00:00Z").unwrap();
        record
            .values
            .insert("_time".to_string(), Value::TimeRFC(dt));

        assert!(record.time().is_some());
        assert_eq!(record.time().unwrap().year(), 2023);
    }

    #[test]
    fn test_flux_record_time_missing() {
        let record = FluxRecord::new(0);
        assert!(record.time().is_none());
    }

    #[test]
    fn test_flux_record_measurement() {
        let mut record = FluxRecord::new(0);
        record
            .values
            .insert("_measurement".to_string(), Value::String("cpu".to_string()));

        assert_eq!(record.measurement(), Some("cpu".to_string()));
    }

    #[test]
    fn test_flux_record_measurement_missing() {
        let record = FluxRecord::new(0);
        assert!(record.measurement().is_none());
    }

    #[test]
    fn test_flux_record_field() {
        let mut record = FluxRecord::new(0);
        record.values.insert(
            "_field".to_string(),
            Value::String("temperature".to_string()),
        );

        assert_eq!(record.field(), Some("temperature".to_string()));
    }

    #[test]
    fn test_flux_record_field_missing() {
        let record = FluxRecord::new(0);
        assert!(record.field().is_none());
    }

    #[test]
    fn test_flux_record_value() {
        let mut record = FluxRecord::new(0);
        record.values.insert(
            "_value".to_string(),
            Value::Double(OrderedFloat::from(25.5)),
        );

        assert!(record.value().is_some());
        assert_eq!(
            record.value(),
            Some(&Value::Double(OrderedFloat::from(25.5)))
        );
    }

    #[test]
    fn test_flux_record_value_missing() {
        let record = FluxRecord::new(0);
        assert!(record.value().is_none());
    }
}

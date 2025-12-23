//! Async parser for InfluxDB annotated CSV format.
//!
//! This module provides a streaming parser for InfluxDB's annotated CSV format,
//! which is the format returned by the `/api/v2/query` endpoint.

use std::collections::BTreeMap;
use std::str::FromStr;

use base64::Engine;
use chrono::DateTime;
use csv_async::{AsyncReaderBuilder, StringRecord, Trim};
use futures::StreamExt;
use go_parse_duration::parse_duration;
use ordered_float::OrderedFloat;
use tokio::io::AsyncRead;

use crate::error::{Error, Result};
use crate::types::{DataType, FluxRecord, FluxTableMetadata};
use crate::value::Value;

/// Internal state of the CSV parser.
///
/// State transitions:
/// ```text
/// Normal -> Annotation (when # row encountered)
/// Annotation -> Normal (after header row)
/// Annotation -> Error (when error table detected)
/// Error -> (terminates with error)
/// ```
#[derive(PartialEq, Clone, Copy)]
enum ParsingState {
    /// Normal data rows.
    Normal,
    /// Processing annotation rows.
    Annotation,
    /// Error state (InfluxDB returned an error in the CSV).
    Error,
}

/// Result of processing a single row.
enum RowAction {
    /// Continue to next row (annotation or header processed).
    Continue,
    /// Return a parsed record.
    Record(FluxRecord),
    /// Return an error.
    Error(Error),
}

/// Async streaming parser for InfluxDB annotated CSV.
///
/// This parser reads an async byte stream and yields `FluxRecord`s one at a time,
/// without loading the entire response into memory.
///
/// # Example
///
/// ```ignore
/// use influxdb_stream::parser::AnnotatedCsvParser;
/// use tokio::io::AsyncRead;
///
/// async fn parse<R: AsyncRead + Unpin + Send>(reader: R) {
///     let mut parser = AnnotatedCsvParser::new(reader);
///     while let Some(record) = parser.next().await.transpose() {
///         match record {
///             Ok(rec) => println!("Got record: {:?}", rec),
///             Err(e) => eprintln!("Parse error: {}", e),
///         }
///     }
/// }
/// ```
pub struct AnnotatedCsvParser<R: AsyncRead + Unpin> {
    csv: csv_async::AsyncReader<R>,
    table_position: i32,
    table: Option<FluxTableMetadata>,
    parsing_state: ParsingState,
    data_type_annotation_found: bool,
}

impl<R: AsyncRead + Unpin + Send> AnnotatedCsvParser<R> {
    /// Create a new parser from an async reader.
    pub fn new(reader: R) -> Self {
        let csv = AsyncReaderBuilder::new()
            .has_headers(false) // We handle headers/annotations ourselves
            .trim(Trim::Fields)
            .flexible(true)
            .create_reader(reader);

        Self {
            csv,
            table_position: 0,
            table: None,
            parsing_state: ParsingState::Normal,
            data_type_annotation_found: false,
        }
    }

    /// Parse and return the next record.
    ///
    /// Returns:
    /// - `Ok(Some(record))` - Successfully parsed a record
    /// - `Ok(None)` - End of stream (EOF)
    /// - `Err(e)` - Parse error
    pub async fn next(&mut self) -> Result<Option<FluxRecord>> {
        let mut records = self.csv.records();

        loop {
            let row = match records.next().await {
                Some(Ok(r)) => r,
                Some(Err(e)) => return Err(Error::Csv(format!("CSV read error: {}", e))),
                None => return Ok(None), // EOF
            };

            // Skip empty rows or rows with only 1 column
            if row.len() <= 1 {
                continue;
            }

            // Detect start of new annotation block
            if detect_annotation_start(
                &row,
                self.parsing_state,
                &mut self.table,
                &mut self.table_position,
                &mut self.parsing_state,
                &mut self.data_type_annotation_found,
            ) {
                // New table started, parsing_state is now Annotation
            }

            // Get table reference or return error if missing
            let table = match &mut self.table {
                Some(t) => t,
                None => {
                    return Err(Error::MissingAnnotation(
                        "No annotations found before data".to_string(),
                    ));
                }
            };

            // Validate column count
            if row.len() - 1 != table.columns.len() {
                return Err(Error::ColumnMismatch {
                    expected: table.columns.len(),
                    actual: row.len() - 1,
                });
            }

            // Process the row based on its first cell
            let action = process_row(
                &row,
                table,
                self.parsing_state,
                self.data_type_annotation_found,
                &mut self.parsing_state,
                &mut self.data_type_annotation_found,
            )?;

            match action {
                RowAction::Continue => continue,
                RowAction::Record(record) => return Ok(Some(record)),
                RowAction::Error(e) => return Err(e),
            }
        }
    }
}

/// Detect if a row starts a new annotation block.
/// Returns true if a new annotation block was started.
fn detect_annotation_start(
    row: &StringRecord,
    current_state: ParsingState,
    table: &mut Option<FluxTableMetadata>,
    table_position: &mut i32,
    parsing_state: &mut ParsingState,
    data_type_annotation_found: &mut bool,
) -> bool {
    if let Some(first) = row.get(0) {
        if !first.is_empty() && first.starts_with('#') && current_state == ParsingState::Normal {
            // Start of a new table
            *table = Some(FluxTableMetadata::new(*table_position, row.len() - 1));
            *table_position += 1;
            *parsing_state = ParsingState::Annotation;
            *data_type_annotation_found = false;
            return true;
        }
    }
    false
}

/// Process a single row and return the appropriate action.
fn process_row(
    row: &StringRecord,
    table: &mut FluxTableMetadata,
    current_state: ParsingState,
    current_datatype_found: bool,
    parsing_state: &mut ParsingState,
    data_type_annotation_found: &mut bool,
) -> Result<RowAction> {
    let first_cell = row.get(0).unwrap_or_default();

    match first_cell {
        "" => process_empty_first_cell(
            row,
            table,
            current_state,
            current_datatype_found,
            parsing_state,
        ),
        "#datatype" => {
            process_datatype_annotation(row, table, data_type_annotation_found)?;
            Ok(RowAction::Continue)
        }
        "#group" => {
            process_group_annotation(row, table);
            Ok(RowAction::Continue)
        }
        "#default" => {
            process_default_annotation(row, table);
            Ok(RowAction::Continue)
        }
        other => Err(Error::Parse {
            message: format!("Invalid first cell: {}", other),
        }),
    }
}

/// Process a row with empty first cell (header, data, or error row).
fn process_empty_first_cell(
    row: &StringRecord,
    table: &mut FluxTableMetadata,
    current_state: ParsingState,
    data_type_annotation_found: bool,
    parsing_state: &mut ParsingState,
) -> Result<RowAction> {
    match current_state {
        ParsingState::Annotation => {
            process_header_row(row, table, data_type_annotation_found, parsing_state)
        }
        ParsingState::Error => Ok(RowAction::Error(parse_error_response(row))),
        ParsingState::Normal => parse_data_row(row, table),
    }
}

/// Process the header row (first row after annotations with empty first cell).
fn process_header_row(
    row: &StringRecord,
    table: &mut FluxTableMetadata,
    data_type_annotation_found: bool,
    parsing_state: &mut ParsingState,
) -> Result<RowAction> {
    if !data_type_annotation_found {
        return Err(Error::MissingAnnotation(
            "#datatype annotation not found".to_string(),
        ));
    }

    // Check for error table
    if row.get(1).unwrap_or_default() == "error" {
        *parsing_state = ParsingState::Error;
        return Ok(RowAction::Continue);
    }

    // Fill column names from header row
    for i in 1..row.len() {
        if let Some(name) = row.get(i) {
            table.columns[i - 1].name = name.to_string();
        }
    }
    *parsing_state = ParsingState::Normal;

    Ok(RowAction::Continue)
}

/// Parse an error response from InfluxDB.
fn parse_error_response(row: &StringRecord) -> Error {
    let message = row
        .get(1)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown query error".to_string());

    let reference = row.get(2).filter(|s| !s.is_empty()).map(|s| s.to_string());

    Error::QueryError { message, reference }
}

/// Parse a data row into a FluxRecord.
fn parse_data_row(row: &StringRecord, table: &FluxTableMetadata) -> Result<RowAction> {
    let mut values = BTreeMap::new();

    for i in 1..row.len() {
        let col = &table.columns[i - 1];
        let raw_value = row.get(i).unwrap_or_default();
        let value = if raw_value.is_empty() {
            &col.default_value
        } else {
            raw_value
        };

        let parsed = parse_value(value, col.data_type, &col.name)?;
        values.insert(col.name.clone(), parsed);
    }

    Ok(RowAction::Record(FluxRecord {
        table: table.position,
        values,
    }))
}

/// Process #datatype annotation row.
fn process_datatype_annotation(
    row: &StringRecord,
    table: &mut FluxTableMetadata,
    data_type_annotation_found: &mut bool,
) -> Result<()> {
    *data_type_annotation_found = true;

    for i in 1..row.len() {
        if let Some(type_str) = row.get(i) {
            let dt = DataType::from_str(type_str)?;
            table.columns[i - 1].data_type = dt;
        }
    }

    Ok(())
}

/// Process #group annotation row.
fn process_group_annotation(row: &StringRecord, table: &mut FluxTableMetadata) {
    for i in 1..row.len() {
        if let Some(value) = row.get(i) {
            table.columns[i - 1].group = value == "true";
        }
    }
}

/// Process #default annotation row.
fn process_default_annotation(row: &StringRecord, table: &mut FluxTableMetadata) {
    for i in 1..row.len() {
        if let Some(value) = row.get(i) {
            table.columns[i - 1].default_value = value.to_string();
        }
    }
}

/// Parse a string value into a Value based on the data type.
fn parse_value(s: &str, data_type: DataType, column_name: &str) -> Result<Value> {
    // Handle empty strings as null for non-string types
    if s.is_empty() && data_type != DataType::String {
        return Ok(Value::Null);
    }

    match data_type {
        DataType::String => Ok(Value::String(s.to_string())),
        DataType::Double => {
            let v = s.parse::<f64>().map_err(|e| Error::Parse {
                message: format!("Invalid double '{}' for column '{}': {}", s, column_name, e),
            })?;
            Ok(Value::Double(OrderedFloat::from(v)))
        }
        DataType::Bool => {
            let v = s.to_lowercase() != "false";
            Ok(Value::Bool(v))
        }
        DataType::Long => {
            let v = s.parse::<i64>().map_err(|e| Error::Parse {
                message: format!("Invalid long '{}' for column '{}': {}", s, column_name, e),
            })?;
            Ok(Value::Long(v))
        }
        DataType::UnsignedLong => {
            let v = s.parse::<u64>().map_err(|e| Error::Parse {
                message: format!(
                    "Invalid unsignedLong '{}' for column '{}': {}",
                    s, column_name, e
                ),
            })?;
            Ok(Value::UnsignedLong(v))
        }
        DataType::Duration => {
            let nanos = parse_duration(s).map_err(|_| Error::Parse {
                message: format!("Invalid duration '{}' for column '{}'", s, column_name),
            })?;
            Ok(Value::Duration(chrono::Duration::nanoseconds(nanos)))
        }
        DataType::Base64Binary => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|e| Error::Parse {
                    message: format!("Invalid base64 '{}' for column '{}': {}", s, column_name, e),
                })?;
            Ok(Value::Base64Binary(bytes))
        }
        DataType::TimeRFC => {
            let t = DateTime::parse_from_rfc3339(s).map_err(|e| Error::Parse {
                message: format!(
                    "Invalid RFC3339 timestamp '{}' for column '{}': {}",
                    s, column_name, e
                ),
            })?;
            Ok(Value::TimeRFC(t))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};
    use std::io::Cursor;

    // =========================================================================
    // parse_value tests - Basic types
    // =========================================================================

    #[test]
    fn test_parse_value_string() {
        let v = parse_value("hello", DataType::String, "test").unwrap();
        assert_eq!(v, Value::String("hello".to_string()));
    }

    #[test]
    fn test_parse_value_string_empty() {
        // Empty string should remain as empty string, not null
        let v = parse_value("", DataType::String, "test").unwrap();
        assert_eq!(v, Value::String("".to_string()));
    }

    #[test]
    fn test_parse_value_double() {
        let v = parse_value("2.72", DataType::Double, "test").unwrap();
        assert_eq!(v, Value::Double(OrderedFloat::from(2.72)));
    }

    #[test]
    fn test_parse_value_double_negative() {
        let v = parse_value("-123.456", DataType::Double, "test").unwrap();
        assert_eq!(v, Value::Double(OrderedFloat::from(-123.456)));
    }

    #[test]
    fn test_parse_value_double_scientific() {
        let v = parse_value("1.5e10", DataType::Double, "test").unwrap();
        assert_eq!(v, Value::Double(OrderedFloat::from(1.5e10)));
    }

    #[test]
    fn test_parse_value_bool() {
        assert_eq!(
            parse_value("true", DataType::Bool, "test").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            parse_value("false", DataType::Bool, "test").unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_parse_value_bool_case_insensitive() {
        assert_eq!(
            parse_value("TRUE", DataType::Bool, "test").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            parse_value("FALSE", DataType::Bool, "test").unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            parse_value("False", DataType::Bool, "test").unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_parse_value_long() {
        let v = parse_value("-42", DataType::Long, "test").unwrap();
        assert_eq!(v, Value::Long(-42));
    }

    #[test]
    fn test_parse_value_long_max() {
        let v = parse_value("9223372036854775807", DataType::Long, "test").unwrap();
        assert_eq!(v, Value::Long(i64::MAX));
    }

    #[test]
    fn test_parse_value_long_min() {
        let v = parse_value("-9223372036854775808", DataType::Long, "test").unwrap();
        assert_eq!(v, Value::Long(i64::MIN));
    }

    #[test]
    fn test_parse_value_unsigned_long() {
        let v = parse_value("42", DataType::UnsignedLong, "test").unwrap();
        assert_eq!(v, Value::UnsignedLong(42));
    }

    #[test]
    fn test_parse_value_unsigned_long_max() {
        let v = parse_value("18446744073709551615", DataType::UnsignedLong, "test").unwrap();
        assert_eq!(v, Value::UnsignedLong(u64::MAX));
    }

    #[test]
    fn test_parse_value_duration() {
        let v = parse_value("1h30m", DataType::Duration, "test").unwrap();
        let expected = chrono::Duration::nanoseconds(5_400_000_000_000); // 1.5 hours in nanos
        assert_eq!(v, Value::Duration(expected));
    }

    #[test]
    fn test_parse_value_duration_nanoseconds() {
        let v = parse_value("100ns", DataType::Duration, "test").unwrap();
        let expected = chrono::Duration::nanoseconds(100);
        assert_eq!(v, Value::Duration(expected));
    }

    #[test]
    fn test_parse_value_duration_complex() {
        let v = parse_value("2h45m30s", DataType::Duration, "test").unwrap();
        // 2*3600 + 45*60 + 30 = 9930 seconds = 9_930_000_000_000 ns
        let expected = chrono::Duration::nanoseconds(9_930_000_000_000);
        assert_eq!(v, Value::Duration(expected));
    }

    #[test]
    fn test_parse_value_base64() {
        let v = parse_value("SGVsbG8gV29ybGQ=", DataType::Base64Binary, "test").unwrap();
        assert_eq!(v, Value::Base64Binary(b"Hello World".to_vec()));
    }

    #[test]
    fn test_parse_value_base64_empty() {
        let v = parse_value("", DataType::Base64Binary, "test").unwrap();
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn test_parse_value_time_rfc3339() {
        let v = parse_value("2023-11-14T12:30:45Z", DataType::TimeRFC, "test").unwrap();
        if let Value::TimeRFC(dt) = v {
            assert_eq!(dt.year(), 2023);
            assert_eq!(dt.month(), 11);
            assert_eq!(dt.day(), 14);
            assert_eq!(dt.hour(), 12);
            assert_eq!(dt.minute(), 30);
            assert_eq!(dt.second(), 45);
        } else {
            panic!("Expected TimeRFC value");
        }
    }

    #[test]
    fn test_parse_value_time_rfc3339_with_timezone() {
        let v = parse_value("2023-11-14T12:30:45+09:00", DataType::TimeRFC, "test").unwrap();
        if let Value::TimeRFC(dt) = v {
            assert_eq!(dt.year(), 2023);
            assert_eq!(dt.offset().local_minus_utc(), 9 * 3600);
        } else {
            panic!("Expected TimeRFC value");
        }
    }

    #[test]
    fn test_parse_value_time_rfc3339_nano() {
        let v = parse_value("2023-11-14T12:30:45.123456789Z", DataType::TimeRFC, "test").unwrap();
        if let Value::TimeRFC(dt) = v {
            assert_eq!(dt.nanosecond(), 123456789);
        } else {
            panic!("Expected TimeRFC value");
        }
    }

    #[test]
    fn test_parse_value_empty_is_null() {
        let v = parse_value("", DataType::Long, "test").unwrap();
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn test_parse_value_empty_is_null_for_all_non_string_types() {
        assert_eq!(
            parse_value("", DataType::Double, "test").unwrap(),
            Value::Null
        );
        assert_eq!(
            parse_value("", DataType::Long, "test").unwrap(),
            Value::Null
        );
        assert_eq!(
            parse_value("", DataType::UnsignedLong, "test").unwrap(),
            Value::Null
        );
        assert_eq!(
            parse_value("", DataType::Bool, "test").unwrap(),
            Value::Null
        );
        assert_eq!(
            parse_value("", DataType::Duration, "test").unwrap(),
            Value::Null
        );
        assert_eq!(
            parse_value("", DataType::Base64Binary, "test").unwrap(),
            Value::Null
        );
        assert_eq!(
            parse_value("", DataType::TimeRFC, "test").unwrap(),
            Value::Null
        );
    }

    // =========================================================================
    // parse_value tests - Error cases
    // =========================================================================

    #[test]
    fn test_parse_value_invalid_double() {
        let result = parse_value("not_a_number", DataType::Double, "test");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::Parse { .. }));
    }

    #[test]
    fn test_parse_value_invalid_long() {
        let result = parse_value("12.5", DataType::Long, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_value_invalid_long_overflow() {
        let result = parse_value("9999999999999999999999", DataType::Long, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_value_invalid_unsigned_long_negative() {
        let result = parse_value("-1", DataType::UnsignedLong, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_value_invalid_duration() {
        let result = parse_value("not_a_duration", DataType::Duration, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_value_invalid_base64() {
        let result = parse_value("!!invalid!!", DataType::Base64Binary, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_value_invalid_time() {
        let result = parse_value("not-a-timestamp", DataType::TimeRFC, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_value_invalid_time_format() {
        // Valid date but wrong format
        let result = parse_value("2023/11/14 12:30:45", DataType::TimeRFC, "test");
        assert!(result.is_err());
    }

    // =========================================================================
    // AnnotatedCsvParser tests - Full flow
    // =========================================================================

    /// Helper to create a parser from a string
    fn parser_from_str(s: &str) -> AnnotatedCsvParser<Cursor<Vec<u8>>> {
        AnnotatedCsvParser::new(Cursor::new(s.as_bytes().to_vec()))
    }

    #[tokio::test]
    async fn test_parser_basic_csv() {
        let csv = r#"#datatype,string,long,double
#group,false,false,false
#default,,0,0.0
,name,count,value
,alice,10,1.5
,bob,20,2.5
"#;
        let mut parser = parser_from_str(csv);

        let record1 = parser.next().await.unwrap().unwrap();
        assert_eq!(record1.get_string("name"), Some("alice".to_string()));
        assert_eq!(record1.get_long("count"), Some(10));
        assert_eq!(record1.get_double("value"), Some(1.5));

        let record2 = parser.next().await.unwrap().unwrap();
        assert_eq!(record2.get_string("name"), Some("bob".to_string()));
        assert_eq!(record2.get_long("count"), Some(20));
        assert_eq!(record2.get_double("value"), Some(2.5));

        // EOF
        assert!(parser.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_parser_empty_input() {
        let csv = "";
        let mut parser = parser_from_str(csv);

        // Should return None (EOF) immediately
        assert!(parser.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_parser_empty_result_set() {
        // Only annotations, no data rows
        let csv = r#"#datatype,string,long
#group,false,false
#default,,
,name,value
"#;
        let mut parser = parser_from_str(csv);

        // No data rows, should return None
        assert!(parser.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_parser_missing_datatype_annotation() {
        let csv = r#"#group,false,false
#default,,
,name,value
,alice,10
"#;
        let mut parser = parser_from_str(csv);

        let result = parser.next().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::MissingAnnotation(_)));
    }

    #[tokio::test]
    async fn test_parser_column_count_mismatch() {
        let csv = r#"#datatype,string,long
#group,false,false
#default,,
,name,value
,alice,10,extra_column
"#;
        let mut parser = parser_from_str(csv);

        let result = parser.next().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::ColumnMismatch { .. }));
    }

    #[tokio::test]
    async fn test_parser_influxdb_error_response() {
        let csv = r#"#datatype,string,string
#group,true,true
#default,,
,error,reference
,bucket not found,some-reference-id
"#;
        let mut parser = parser_from_str(csv);

        let result = parser.next().await;
        assert!(result.is_err());
        if let Error::QueryError { message, reference } = result.unwrap_err() {
            assert_eq!(message, "bucket not found");
            assert_eq!(reference, Some("some-reference-id".to_string()));
        } else {
            panic!("Expected QueryError");
        }
    }

    #[tokio::test]
    async fn test_parser_influxdb_error_response_no_reference() {
        let csv = r#"#datatype,string,string
#group,true,true
#default,,
,error,reference
,query syntax error,
"#;
        let mut parser = parser_from_str(csv);

        let result = parser.next().await;
        assert!(result.is_err());
        if let Error::QueryError { message, reference } = result.unwrap_err() {
            assert_eq!(message, "query syntax error");
            assert!(reference.is_none());
        } else {
            panic!("Expected QueryError");
        }
    }

    #[tokio::test]
    async fn test_parser_multiple_tables() {
        let csv = r#"#datatype,string,long
#group,false,false
#default,,
,name,value
,alice,10

#datatype,string,double
#group,false,false
#default,,
,name,score
,bob,95.5
"#;
        let mut parser = parser_from_str(csv);

        let record1 = parser.next().await.unwrap().unwrap();
        assert_eq!(record1.table, 0);
        assert_eq!(record1.get_string("name"), Some("alice".to_string()));
        assert_eq!(record1.get_long("value"), Some(10));

        let record2 = parser.next().await.unwrap().unwrap();
        assert_eq!(record2.table, 1);
        assert_eq!(record2.get_string("name"), Some("bob".to_string()));
        assert_eq!(record2.get_double("score"), Some(95.5));

        assert!(parser.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_parser_default_values() {
        let csv = r#"#datatype,string,long,double
#group,false,false,false
#default,unknown,0,1.0
,name,count,value
,alice,,
"#;
        let mut parser = parser_from_str(csv);

        let record = parser.next().await.unwrap().unwrap();
        assert_eq!(record.get_string("name"), Some("alice".to_string()));
        // Empty values should use defaults
        assert_eq!(record.get_long("count"), Some(0));
        assert_eq!(record.get_double("value"), Some(1.0));
    }

    #[tokio::test]
    async fn test_parser_group_annotation() {
        let csv = r#"#datatype,string,string,long
#group,true,false,false
#default,,,
,_measurement,host,value
,cpu,server1,100
"#;
        let mut parser = parser_from_str(csv);

        let record = parser.next().await.unwrap().unwrap();
        assert_eq!(record.get_string("_measurement"), Some("cpu".to_string()));
        assert_eq!(record.get_string("host"), Some("server1".to_string()));
        assert_eq!(record.get_long("value"), Some(100));
    }

    #[tokio::test]
    async fn test_parser_all_data_types() {
        let csv = r#"#datatype,string,long,unsignedLong,double,boolean,dateTime:RFC3339
#group,false,false,false,false,false,false
#default,,,,,,
,str,lng,ulng,dbl,bl,ts
,hello,-42,18446744073709551615,2.72,true,2023-11-14T12:00:00Z
"#;
        let mut parser = parser_from_str(csv);

        let record = parser.next().await.unwrap().unwrap();
        assert_eq!(record.get_string("str"), Some("hello".to_string()));
        assert_eq!(record.get_long("lng"), Some(-42));
        assert_eq!(
            record.values.get("ulng").and_then(|v| v.as_unsigned_long()),
            Some(u64::MAX)
        );
        assert_eq!(record.get_double("dbl"), Some(2.72));
        assert_eq!(record.get_bool("bl"), Some(true));
        assert!(record.values.get("ts").and_then(|v| v.as_time()).is_some());
    }

    #[tokio::test]
    async fn test_parser_skips_empty_rows() {
        let csv = r#"#datatype,string,long
#group,false,false
#default,,
,name,value

,alice,10

,bob,20

"#;
        let mut parser = parser_from_str(csv);

        let record1 = parser.next().await.unwrap().unwrap();
        assert_eq!(record1.get_string("name"), Some("alice".to_string()));

        let record2 = parser.next().await.unwrap().unwrap();
        assert_eq!(record2.get_string("name"), Some("bob".to_string()));

        assert!(parser.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_parser_invalid_first_cell() {
        let csv = r#"#datatype,string,long
#group,false,false
#default,,
,name,value
invalid,alice,10
"#;
        let mut parser = parser_from_str(csv);

        let result = parser.next().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Parse { .. }));
    }

    #[tokio::test]
    async fn test_parser_unknown_datatype() {
        let csv = r#"#datatype,string,unknown_type
#group,false,false
#default,,
,name,value
,alice,10
"#;
        let mut parser = parser_from_str(csv);

        let result = parser.next().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::UnknownDataType(_)));
    }
}

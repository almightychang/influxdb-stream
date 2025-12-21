//! Async parser for InfluxDB annotated CSV format.
//!
//! This module provides a streaming parser for InfluxDB's annotated CSV format,
//! which is the format returned by the `/api/v2/query` endpoint.

use std::collections::BTreeMap;
use std::str::FromStr;

use base64::Engine;
use chrono::DateTime;
use csv_async::{AsyncReaderBuilder, Trim};
use futures::StreamExt;
use go_parse_duration::parse_duration;
use ordered_float::OrderedFloat;
use tokio::io::AsyncRead;

use crate::error::{Error, Result};
use crate::types::{DataType, FluxRecord, FluxTableMetadata};
use crate::value::Value;

/// Internal state of the CSV parser.
#[derive(PartialEq)]
enum ParsingState {
    /// Normal data rows.
    Normal,
    /// Processing annotation rows.
    Annotation,
    /// Error state (InfluxDB returned an error in the CSV).
    Error,
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
            let row_opt = records.next().await;
            let row = match row_opt {
                Some(Ok(r)) => r,
                Some(Err(e)) => {
                    return Err(Error::Csv(format!("CSV read error: {}", e)));
                }
                None => return Ok(None), // EOF
            };

            // Skip empty rows or rows with only 1 column
            if row.len() <= 1 {
                continue;
            }

            // Check for annotation block start (rows starting with '#')
            if let Some(first) = row.get(0) {
                if !first.is_empty() && first.starts_with('#') {
                    if self.parsing_state == ParsingState::Normal {
                        // Start of a new table
                        self.table = Some(FluxTableMetadata::new(
                            self.table_position,
                            row.len() - 1,
                        ));
                        self.table_position += 1;
                        self.parsing_state = ParsingState::Annotation;
                        self.data_type_annotation_found = false;
                    }
                }
            }

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

            match row.get(0).unwrap_or_default() {
                // Data row (first cell is empty)
                "" => {
                    match self.parsing_state {
                        ParsingState::Annotation => {
                            if !self.data_type_annotation_found {
                                return Err(Error::MissingAnnotation(
                                    "#datatype annotation not found".to_string(),
                                ));
                            }
                            // Check for error table
                            if row.get(1).unwrap_or_default() == "error" {
                                self.parsing_state = ParsingState::Error;
                            } else {
                                // This is the header row - fill column names
                                for i in 1..row.len() {
                                    table.columns[i - 1].name = row.get(i).unwrap().to_string();
                                }
                                self.parsing_state = ParsingState::Normal;
                            }
                            continue;
                        }
                        ParsingState::Error => {
                            // Parse error message from InfluxDB
                            let message = if row.len() > 1 && !row.get(1).unwrap().is_empty() {
                                row.get(1).unwrap().to_string()
                            } else {
                                "Unknown query error".to_string()
                            };
                            let reference = if row.len() > 2 && !row.get(2).unwrap().is_empty() {
                                Some(row.get(2).unwrap().to_string())
                            } else {
                                None
                            };
                            return Err(Error::QueryError { message, reference });
                        }
                        ParsingState::Normal => {
                            // Parse data row into FluxRecord
                            let mut values = BTreeMap::new();
                            for i in 1..row.len() {
                                let col = &table.columns[i - 1];
                                let mut v = row.get(i).unwrap();
                                if v.is_empty() {
                                    v = &col.default_value;
                                }
                                let parsed = parse_value(v, col.data_type, &col.name)?;
                                values.insert(col.name.clone(), parsed);
                            }
                            return Ok(Some(FluxRecord {
                                table: table.position,
                                values,
                            }));
                        }
                    }
                }
                // Annotation rows
                "#datatype" => {
                    self.data_type_annotation_found = true;
                    for i in 1..row.len() {
                        let dt = DataType::from_str(row.get(i).unwrap())?;
                        table.columns[i - 1].data_type = dt;
                    }
                }
                "#group" => {
                    for i in 1..row.len() {
                        table.columns[i - 1].group = row.get(i).unwrap() == "true";
                    }
                }
                "#default" => {
                    for i in 1..row.len() {
                        table.columns[i - 1].default_value = row.get(i).unwrap().to_string();
                    }
                }
                other => {
                    return Err(Error::Parse {
                        message: format!("Invalid first cell: {}", other),
                    });
                }
            }
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
                    message: format!(
                        "Invalid base64 '{}' for column '{}': {}",
                        s, column_name, e
                    ),
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

    #[test]
    fn test_parse_value_string() {
        let v = parse_value("hello", DataType::String, "test").unwrap();
        assert_eq!(v, Value::String("hello".to_string()));
    }

    #[test]
    fn test_parse_value_double() {
        let v = parse_value("3.14", DataType::Double, "test").unwrap();
        assert_eq!(v, Value::Double(OrderedFloat::from(3.14)));
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
    fn test_parse_value_long() {
        let v = parse_value("-42", DataType::Long, "test").unwrap();
        assert_eq!(v, Value::Long(-42));
    }

    #[test]
    fn test_parse_value_empty_is_null() {
        let v = parse_value("", DataType::Long, "test").unwrap();
        assert_eq!(v, Value::Null);
    }
}

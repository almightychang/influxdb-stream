//! Value types for InfluxDB Flux query results.

use chrono::{DateTime, FixedOffset};
use ordered_float::OrderedFloat;

/// Represents a value in an InfluxDB Flux query result.
///
/// This enum covers all data types that can appear in InfluxDB annotated CSV responses.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// String value.
    String(String),

    /// 64-bit floating point value.
    Double(OrderedFloat<f64>),

    /// Boolean value.
    Bool(bool),

    /// Signed 64-bit integer.
    Long(i64),

    /// Unsigned 64-bit integer.
    UnsignedLong(u64),

    /// Duration value (in nanoseconds, stored as chrono::Duration).
    Duration(chrono::Duration),

    /// Base64-encoded binary data.
    Base64Binary(Vec<u8>),

    /// RFC3339 timestamp with timezone.
    TimeRFC(DateTime<FixedOffset>),

    /// Null value.
    Null,
}

impl Value {
    /// Returns the value as a string reference if it is a `String` variant.
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the value as an owned string if it is a `String` variant.
    pub fn string(&self) -> Option<String> {
        match self {
            Value::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Returns the value as a f64 if it is a `Double` variant.
    pub fn as_double(&self) -> Option<f64> {
        match self {
            Value::Double(f) => Some(f.into_inner()),
            _ => None,
        }
    }

    /// Returns the value as a bool if it is a `Bool` variant.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns the value as an i64 if it is a `Long` variant.
    pub fn as_long(&self) -> Option<i64> {
        match self {
            Value::Long(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the value as a u64 if it is an `UnsignedLong` variant.
    pub fn as_unsigned_long(&self) -> Option<u64> {
        match self {
            Value::UnsignedLong(u) => Some(*u),
            _ => None,
        }
    }

    /// Returns the value as a chrono::Duration if it is a `Duration` variant.
    pub fn as_duration(&self) -> Option<&chrono::Duration> {
        match self {
            Value::Duration(d) => Some(d),
            _ => None,
        }
    }

    /// Returns the value as a byte slice if it is a `Base64Binary` variant.
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            Value::Base64Binary(b) => Some(b),
            _ => None,
        }
    }

    /// Returns the value as a DateTime if it is a `TimeRFC` variant.
    pub fn as_time(&self) -> Option<&DateTime<FixedOffset>> {
        match self {
            Value::TimeRFC(t) => Some(t),
            _ => None,
        }
    }

    /// Returns true if this value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Double(d) => write!(f, "{}", d),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Long(i) => write!(f, "{}", i),
            Value::UnsignedLong(u) => write!(f, "{}", u),
            Value::Duration(d) => write!(f, "{}ns", d.num_nanoseconds().unwrap_or(0)),
            Value::Base64Binary(b) => write!(f, "<binary {} bytes>", b.len()),
            Value::TimeRFC(t) => write!(f, "{}", t.to_rfc3339()),
            Value::Null => write!(f, "null"),
        }
    }
}

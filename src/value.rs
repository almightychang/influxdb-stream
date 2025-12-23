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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Value accessor tests
    // =========================================================================

    #[test]
    fn test_as_string() {
        let v = Value::String("hello".to_string());
        assert_eq!(v.as_string(), Some("hello"));

        // Wrong type returns None
        assert_eq!(Value::Long(42).as_string(), None);
        assert_eq!(Value::Null.as_string(), None);
    }

    #[test]
    fn test_string() {
        let v = Value::String("hello".to_string());
        assert_eq!(v.string(), Some("hello".to_string()));

        // Wrong type returns None
        assert_eq!(Value::Long(42).string(), None);
        assert_eq!(Value::Null.string(), None);
    }

    #[test]
    fn test_as_double() {
        let v = Value::Double(OrderedFloat::from(2.72));
        assert_eq!(v.as_double(), Some(2.72));

        // Wrong type returns None
        assert_eq!(Value::Long(42).as_double(), None);
        assert_eq!(Value::String("2.72".to_string()).as_double(), None);
        assert_eq!(Value::Null.as_double(), None);
    }

    #[test]
    fn test_as_bool() {
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Bool(false).as_bool(), Some(false));

        // Wrong type returns None
        assert_eq!(Value::Long(1).as_bool(), None);
        assert_eq!(Value::String("true".to_string()).as_bool(), None);
        assert_eq!(Value::Null.as_bool(), None);
    }

    #[test]
    fn test_as_long() {
        assert_eq!(Value::Long(42).as_long(), Some(42));
        assert_eq!(Value::Long(-100).as_long(), Some(-100));
        assert_eq!(Value::Long(i64::MAX).as_long(), Some(i64::MAX));

        // Wrong type returns None
        assert_eq!(Value::UnsignedLong(42).as_long(), None);
        assert_eq!(Value::Double(OrderedFloat::from(42.0)).as_long(), None);
        assert_eq!(Value::Null.as_long(), None);
    }

    #[test]
    fn test_as_unsigned_long() {
        assert_eq!(Value::UnsignedLong(42).as_unsigned_long(), Some(42));
        assert_eq!(Value::UnsignedLong(u64::MAX).as_unsigned_long(), Some(u64::MAX));

        // Wrong type returns None
        assert_eq!(Value::Long(42).as_unsigned_long(), None);
        assert_eq!(Value::Double(OrderedFloat::from(42.0)).as_unsigned_long(), None);
        assert_eq!(Value::Null.as_unsigned_long(), None);
    }

    #[test]
    fn test_as_duration() {
        let dur = chrono::Duration::nanoseconds(1_000_000_000);
        let v = Value::Duration(dur);
        assert!(v.as_duration().is_some());
        assert_eq!(v.as_duration().unwrap().num_seconds(), 1);

        // Wrong type returns None
        assert!(Value::Long(1000).as_duration().is_none());
        assert!(Value::Null.as_duration().is_none());
    }

    #[test]
    fn test_as_binary() {
        let v = Value::Base64Binary(vec![1, 2, 3, 4]);
        assert_eq!(v.as_binary(), Some(&[1u8, 2, 3, 4][..]));

        // Wrong type returns None
        assert!(Value::String("data".to_string()).as_binary().is_none());
        assert!(Value::Null.as_binary().is_none());
    }

    #[test]
    fn test_as_time() {
        let dt = DateTime::parse_from_rfc3339("2023-11-14T12:00:00Z").unwrap();
        let v = Value::TimeRFC(dt);
        assert!(v.as_time().is_some());

        // Wrong type returns None
        assert!(Value::String("2023-11-14".to_string()).as_time().is_none());
        assert!(Value::Long(1699963200).as_time().is_none());
        assert!(Value::Null.as_time().is_none());
    }

    #[test]
    fn test_is_null() {
        assert!(Value::Null.is_null());

        // Non-null values
        assert!(!Value::String("".to_string()).is_null());
        assert!(!Value::Long(0).is_null());
        assert!(!Value::Bool(false).is_null());
        assert!(!Value::Double(OrderedFloat::from(0.0)).is_null());
    }

    // =========================================================================
    // Value Display tests
    // =========================================================================

    #[test]
    fn test_display_string() {
        let v = Value::String("hello world".to_string());
        assert_eq!(v.to_string(), "hello world");
    }

    #[test]
    fn test_display_double() {
        let v = Value::Double(OrderedFloat::from(1.23456));
        assert!(v.to_string().starts_with("1.23"));
    }

    #[test]
    fn test_display_bool() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Bool(false).to_string(), "false");
    }

    #[test]
    fn test_display_long() {
        assert_eq!(Value::Long(42).to_string(), "42");
        assert_eq!(Value::Long(-100).to_string(), "-100");
    }

    #[test]
    fn test_display_unsigned_long() {
        assert_eq!(Value::UnsignedLong(42).to_string(), "42");
        assert_eq!(Value::UnsignedLong(u64::MAX).to_string(), "18446744073709551615");
    }

    #[test]
    fn test_display_duration() {
        let dur = chrono::Duration::nanoseconds(1_500_000_000);
        let v = Value::Duration(dur);
        assert_eq!(v.to_string(), "1500000000ns");
    }

    #[test]
    fn test_display_base64_binary() {
        let v = Value::Base64Binary(vec![1, 2, 3, 4, 5]);
        assert_eq!(v.to_string(), "<binary 5 bytes>");
    }

    #[test]
    fn test_display_time_rfc() {
        let dt = DateTime::parse_from_rfc3339("2023-11-14T12:30:45Z").unwrap();
        let v = Value::TimeRFC(dt);
        assert!(v.to_string().contains("2023-11-14"));
        assert!(v.to_string().contains("12:30:45"));
    }

    #[test]
    fn test_display_null() {
        assert_eq!(Value::Null.to_string(), "null");
    }

    // =========================================================================
    // Value equality tests
    // =========================================================================

    #[test]
    fn test_value_equality() {
        assert_eq!(Value::String("a".to_string()), Value::String("a".to_string()));
        assert_ne!(Value::String("a".to_string()), Value::String("b".to_string()));

        assert_eq!(Value::Long(42), Value::Long(42));
        assert_ne!(Value::Long(42), Value::Long(43));

        assert_eq!(Value::Null, Value::Null);

        // Different types are not equal
        assert_ne!(Value::Long(42), Value::UnsignedLong(42));
        assert_ne!(Value::String("42".to_string()), Value::Long(42));
    }

    #[test]
    fn test_value_clone() {
        let original = Value::String("test".to_string());
        let cloned = original.clone();
        assert_eq!(original, cloned);

        let original = Value::Base64Binary(vec![1, 2, 3]);
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}

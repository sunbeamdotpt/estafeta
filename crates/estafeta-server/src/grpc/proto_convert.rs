//! Helpers for converting between `prost_types::Struct` and `serde_json::Value`.

use prost_types::value::Kind;
use prost_types::{ListValue, Struct, Value as ProtoValue};
use serde_json::{Map, Number, Value};

/// Convert a `prost_types::Struct` to a `serde_json::Value`.
pub fn proto_struct_to_value(s: &Struct) -> Value {
    let map: Map<String, Value> = s
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), proto_value_to_json(v)))
        .collect();
    Value::Object(map)
}

/// Convert a `serde_json::Value` to a `prost_types::Struct`.
/// Returns `None` if the value is not an object.
pub fn value_to_proto_struct(v: &Value) -> Option<Struct> {
    match v {
        Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_proto_value(v)))
                .collect();
            Some(Struct { fields })
        }
        _ => None,
    }
}

fn proto_value_to_json(v: &ProtoValue) -> Value {
    match &v.kind {
        Some(Kind::NullValue(_)) => Value::Null,
        Some(Kind::NumberValue(n)) => {
            Number::from_f64(*n)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
        Some(Kind::StringValue(s)) => Value::String(s.clone()),
        Some(Kind::BoolValue(b)) => Value::Bool(*b),
        Some(Kind::StructValue(s)) => proto_struct_to_value(s),
        Some(Kind::ListValue(list)) => {
            Value::Array(list.values.iter().map(proto_value_to_json).collect())
        }
        None => Value::Null,
    }
}

fn json_to_proto_value(v: &Value) -> ProtoValue {
    let kind = match v {
        Value::Null => Some(Kind::NullValue(0)),
        Value::Bool(b) => Some(Kind::BoolValue(*b)),
        Value::Number(n) => Some(Kind::NumberValue(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => Some(Kind::StringValue(s.clone())),
        Value::Array(arr) => Some(Kind::ListValue(ListValue {
            values: arr.iter().map(json_to_proto_value).collect(),
        })),
        Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_proto_value(v)))
                .collect();
            Some(Kind::StructValue(Struct { fields }))
        }
    };
    ProtoValue { kind }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_roundtrip_simple_object() {
        let original = json!({
            "name": "test",
            "count": 42.0,
            "active": true
        });
        let proto = value_to_proto_struct(&original).unwrap();
        let back = proto_struct_to_value(&proto);
        assert_eq!(original, back);
    }

    #[test]
    fn test_roundtrip_nested_object() {
        let original = json!({
            "user": {
                "name": "alice",
                "tags": ["admin", "active"]
            }
        });
        let proto = value_to_proto_struct(&original).unwrap();
        let back = proto_struct_to_value(&proto);
        assert_eq!(original, back);
    }

    #[test]
    fn test_roundtrip_null_values() {
        let original = json!({
            "present": "yes",
            "absent": null
        });
        let proto = value_to_proto_struct(&original).unwrap();
        let back = proto_struct_to_value(&proto);
        assert_eq!(original, back);
    }

    #[test]
    fn test_roundtrip_array() {
        let original = json!({
            "items": [1.0, 2.0, 3.0],
            "mixed": [true, "hello", null, 99.0]
        });
        let proto = value_to_proto_struct(&original).unwrap();
        let back = proto_struct_to_value(&proto);
        assert_eq!(original, back);
    }

    #[test]
    fn test_roundtrip_empty_object() {
        let original = json!({});
        let proto = value_to_proto_struct(&original).unwrap();
        let back = proto_struct_to_value(&proto);
        assert_eq!(original, back);
    }

    #[test]
    fn test_non_object_returns_none() {
        assert!(value_to_proto_struct(&json!("string")).is_none());
        assert!(value_to_proto_struct(&json!(42)).is_none());
        assert!(value_to_proto_struct(&json!(null)).is_none());
        assert!(value_to_proto_struct(&json!([1, 2])).is_none());
    }

    #[test]
    fn test_deeply_nested() {
        let original = json!({
            "a": { "b": { "c": { "d": "deep" } } }
        });
        let proto = value_to_proto_struct(&original).unwrap();
        let back = proto_struct_to_value(&proto);
        assert_eq!(original, back);
    }
}

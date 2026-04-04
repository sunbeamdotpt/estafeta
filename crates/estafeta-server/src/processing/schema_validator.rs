use serde_json::Value;

/// Validate a JSON payload against a JSON Schema.
/// Returns Ok(()) if valid, or a list of validation errors.
pub fn validate_payload(schema: &Value, payload: &Value) -> Result<(), Vec<String>> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|e| vec![format!("invalid schema: {e}")])?;

    let errors: Vec<String> = validator
        .iter_errors(payload)
        .map(|e| format!("{e}"))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check if a schema is itself a valid JSON Schema.
pub fn is_valid_schema(schema: &Value) -> bool {
    jsonschema::validator_for(schema).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_payload() {
        let schema = json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "body": { "type": "string" }
            },
            "required": ["title"]
        });
        let payload = json!({ "title": "Hello", "body": "World" });
        assert!(validate_payload(&schema, &payload).is_ok());
    }

    #[test]
    fn test_missing_required_field() {
        let schema = json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" }
            },
            "required": ["title"]
        });
        let payload = json!({ "body": "World" });
        let err = validate_payload(&schema, &payload).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn test_wrong_type() {
        let schema = json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            }
        });
        let payload = json!({ "count": "not a number" });
        let err = validate_payload(&schema, &payload).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn test_permissive_schema() {
        let schema = json!({ "type": "object" });
        let payload = json!({ "anything": "goes", "nested": { "ok": true } });
        assert!(validate_payload(&schema, &payload).is_ok());
    }

    #[test]
    fn test_valid_schema_check() {
        assert!(is_valid_schema(&json!({"type": "object"})));
    }
}

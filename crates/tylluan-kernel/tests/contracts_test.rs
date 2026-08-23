//! Executable Contract Tests (Pilar 2 - repo-pillars)
//!
//! Validates that the real production Rust data structures and handlers
//! strictly comply with the formal JSON Schemas located in `schemas/`.
//!
//! Tests real structs (EmbedRequest, EmbedResponse, HealthResponse) from `tylluan_kernel`
//! so that any internal struct drift immediately breaks the contract test.

use serde_json::{Value, json};
use tylluan_kernel::transport::http::HealthResponse;
use tylluan_kernel::transport::http::api_v1::{EmbedRequest, EmbedResponse};

const EMBED_REQ_SCHEMA_STR: &str = include_str!("../../../schemas/api_v1_embed_request.json");
const EMBED_RESP_SCHEMA_STR: &str = include_str!("../../../schemas/api_v1_embed_response.json");
const HEALTH_RESP_SCHEMA_STR: &str = include_str!("../../../schemas/health_response.json");

/// Validates a JSON value against a simplified JSON Schema definition.
/// Returns Ok(()) if valid, or Err with detailed explanation if invalid.
fn validate_json_schema(instance: &Value, schema: &Value) -> Result<(), String> {
    // 1. Type validation
    if let Some(expected_type) = schema.get("type").and_then(|t| t.as_str()) {
        match expected_type {
            "object" => {
                let obj = instance.as_object().ok_or_else(|| format!("Expected object, got {instance:?}"))?;
                
                // Required fields
                if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                    for req_field in required {
                        let field_name = req_field.as_str().unwrap_or_default();
                        if !obj.contains_key(field_name) {
                            return Err(format!("Missing required field '{field_name}' in object"));
                        }
                    }
                }

                // Additional properties check
                if let Some(allow_additional) = schema.get("additionalProperties").and_then(|a| a.as_bool())
                    && !allow_additional
                    && let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
                        for key in obj.keys() {
                            if !properties.contains_key(key) {
                                return Err(format!("Unexpected additional property '{key}' not allowed by schema"));
                            }
                        }
                    }

                // Validate individual property schemas
                if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
                    for (prop_name, prop_schema) in properties {
                        if let Some(prop_val) = obj.get(prop_name) {
                            validate_json_schema(prop_val, prop_schema)
                                .map_err(|e| format!("In property '{prop_name}': {e}"))?;
                        }
                    }
                }
            }
            "array" => {
                let arr = instance.as_array().ok_or_else(|| format!("Expected array, got {instance:?}"))?;
                if let Some(min_items) = schema.get("minItems").and_then(|m| m.as_u64())
                    && (arr.len() as u64) < min_items {
                        return Err(format!("Array length {} is less than minItems {}", arr.len(), min_items));
                    }
                if let Some(item_schema) = schema.get("items") {
                    for (idx, item) in arr.iter().enumerate() {
                        validate_json_schema(item, item_schema)
                            .map_err(|e| format!("In array item [{idx}]: {e}"))?;
                    }
                }
            }
            "string" => {
                let s = instance.as_str().ok_or_else(|| format!("Expected string, got {instance:?}"))?;
                if let Some(min_len) = schema.get("minLength").and_then(|m| m.as_u64())
                    && (s.len() as u64) < min_len {
                        return Err(format!("String length {} is less than minLength {}", s.len(), min_len));
                    }
                if let Some(enum_vals) = schema.get("enum").and_then(|e| e.as_array()) {
                    let is_in_enum = enum_vals.iter().any(|ev| ev.as_str() == Some(s));
                    if !is_in_enum {
                        return Err(format!("String '{s}' is not in allowed enum {enum_vals:?}"));
                    }
                }
            }
            "number" => {
                if !instance.is_number() {
                    return Err(format!("Expected number, got {instance:?}"));
                }
            }
            "integer" => {
                if !instance.is_i64() && !instance.is_u64() {
                    return Err(format!("Expected integer, got {instance:?}"));
                }
                if let Some(min) = schema.get("minimum").and_then(|m| m.as_i64()) {
                    let val = instance.as_i64().unwrap_or(0);
                    if val < min {
                        return Err(format!("Integer value {val} is less than minimum {min}"));
                    }
                }
            }
            "boolean" if !instance.is_boolean() => {
                return Err(format!("Expected boolean, got {instance:?}"));
            }
            _ => {}
        }
    }
    Ok(())
}

#[test]
fn test_schemas_are_valid_json() {
    let req_schema: Value = serde_json::from_str(EMBED_REQ_SCHEMA_STR).expect("Embed request schema must be valid JSON");
    let resp_schema: Value = serde_json::from_str(EMBED_RESP_SCHEMA_STR).expect("Embed response schema must be valid JSON");
    let health_schema: Value = serde_json::from_str(HEALTH_RESP_SCHEMA_STR).expect("Health response schema must be valid JSON");

    assert_eq!(req_schema["title"], "EmbedRequest");
    assert_eq!(resp_schema["title"], "EmbedResponse");
    assert_eq!(health_schema["title"], "HealthResponse");
}

#[test]
fn test_real_embed_request_struct_matches_schema() {
    let schema: Value = serde_json::from_str(EMBED_REQ_SCHEMA_STR).unwrap();

    // 1. Instantiate the real production Rust struct
    let real_req = EmbedRequest {
        text: "benchmark intent for code analysis".to_string(),
    };

    // 2. Serialize real struct instance
    let serialized_req = serde_json::to_value(&real_req).expect("Failed to serialize real EmbedRequest");

    // 3. Validate against formal schema
    assert!(validate_json_schema(&serialized_req, &schema).is_ok());

    // 4. Test roundtrip deserialization into the real struct
    let deserialized: EmbedRequest = serde_json::from_value(serialized_req).expect("Roundtrip deserialize failed");
    assert_eq!(deserialized, real_req);
}

#[test]
fn test_real_embed_response_struct_matches_schema() {
    let schema: Value = serde_json::from_str(EMBED_RESP_SCHEMA_STR).unwrap();

    // 1. Instantiate real production Rust struct (1024-dim vector from BGE-M3)
    let mock_vector: Vec<f32> = vec![0.042; 1024];
    let real_resp = EmbedResponse {
        dimension: mock_vector.len(),
        embedding: mock_vector.clone(),
        model: "bge-m3".to_string(),
    };

    // 2. Serialize real struct instance
    let serialized_resp = serde_json::to_value(&real_resp).expect("Failed to serialize real EmbedResponse");

    // 3. Validate against formal schema
    assert!(validate_json_schema(&serialized_resp, &schema).is_ok());

    // 4. Test roundtrip deserialization into the real struct
    let deserialized: EmbedResponse = serde_json::from_value(serialized_resp).expect("Roundtrip deserialize failed");
    assert_eq!(deserialized, real_resp);
}

#[test]
fn test_real_health_response_struct_matches_schema() {
    let schema: Value = serde_json::from_str(HEALTH_RESP_SCHEMA_STR).unwrap();

    // 1. Instantiate real production Rust struct
    let real_health = HealthResponse {
        status: "ok".to_string(),
        version: "0.16.0".to_string(),
        commit: "be69f11".to_string(),
    };

    // 2. Serialize real struct instance
    let serialized_health = serde_json::to_value(&real_health).expect("Failed to serialize real HealthResponse");

    // 3. Validate against formal schema
    assert!(validate_json_schema(&serialized_health, &schema).is_ok());

    // 4. Test roundtrip deserialization into the real struct
    let deserialized: HealthResponse = serde_json::from_value(serialized_health).expect("Roundtrip deserialize failed");
    assert_eq!(deserialized, real_health);
}

#[test]
fn test_contract_rejects_drifted_payloads() {
    let req_schema: Value = serde_json::from_str(EMBED_REQ_SCHEMA_STR).unwrap();
    let resp_schema: Value = serde_json::from_str(EMBED_RESP_SCHEMA_STR).unwrap();
    let health_schema: Value = serde_json::from_str(HEALTH_RESP_SCHEMA_STR).unwrap();

    // Drift 1: Request missing "text"
    let bad_req = json!({ "wrong_field": "text" });
    assert!(validate_json_schema(&bad_req, &req_schema).is_err());

    // Drift 2: Response with renamed key ("vector" instead of "embedding")
    let bad_resp = json!({
        "vector": [0.1, 0.2],
        "dimension": 2,
        "model": "bge-m3"
    });
    assert!(validate_json_schema(&bad_resp, &resp_schema).is_err());

    // Drift 3: Response with wrong item types in vector
    let bad_items = json!({
        "embedding": ["0.1", "0.2"],
        "dimension": 2,
        "model": "bge-m3"
    });
    assert!(validate_json_schema(&bad_items, &resp_schema).is_err());

    // Drift 4: Health status outside allowed enum
    let bad_health = json!({
        "status": "unhealthy_custom_status",
        "version": "0.16.0",
        "commit": "be69f11"
    });
    assert!(validate_json_schema(&bad_health, &health_schema).is_err());
}

#!/usr/bin/env python3
"""
Pilar 2 - Executable Contract Verification Script

Validates that live kernel endpoints (/health and /api/v1/embed) return payloads
strictly complying with the formal JSON Schemas in `schemas/`.
"""

import json
import sys
import urllib.request
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")

def validate_object(data, schema, path=""):
    schema_type = schema.get("type")
    if schema_type == "object":
        if not isinstance(data, dict):
            raise ValueError(f"{path}: expected object/dict, got {type(data).__name__}")
        
        # Check required fields
        for req in schema.get("required", []):
            if req not in data:
                raise ValueError(f"{path}: missing required property '{req}'")
        
        # Check additional properties
        if schema.get("additionalProperties") is False:
            allowed = set(schema.get("properties", {}).keys())
            for k in data.keys():
                if k not in allowed:
                    raise ValueError(f"{path}: unexpected property '{k}' (not allowed by schema)")
        
        # Validate properties
        for prop, prop_schema in schema.get("properties", {}).items():
            if prop in data:
                validate_object(data[prop], prop_schema, f"{path}.{prop}" if path else prop)
                
    elif schema_type == "array":
        if not isinstance(data, list):
            raise ValueError(f"{path}: expected array/list, got {type(data).__name__}")
        min_items = schema.get("minItems")
        if min_items is not None and len(data) < min_items:
            raise ValueError(f"{path}: array length {len(data)} < minItems {min_items}")
        item_schema = schema.get("items")
        if item_schema:
            for idx, item in enumerate(data):
                validate_object(item, item_schema, f"{path}[{idx}]")
                
    elif schema_type == "string":
        if not isinstance(data, str):
            raise ValueError(f"{path}: expected string, got {type(data).__name__}")
        min_len = schema.get("minLength")
        if min_len is not None and len(data) < min_len:
            raise ValueError(f"{path}: string length {len(data)} < minLength {min_len}")
        enum_vals = schema.get("enum")
        if enum_vals is not None and data not in enum_vals:
            raise ValueError(f"{path}: value '{data}' not in allowed enum {enum_vals}")
            
    elif schema_type == "integer":
        if not isinstance(data, int) or isinstance(data, bool):
            raise ValueError(f"{path}: expected integer, got {type(data).__name__}")
        minimum = schema.get("minimum")
        if minimum is not None and data < minimum:
            raise ValueError(f"{path}: value {data} < minimum {minimum}")
            
    elif schema_type == "number":
        if not isinstance(data, (int, float)) or isinstance(data, bool):
            raise ValueError(f"{path}: expected number, got {type(data).__name__}")

def main():
    root = Path(__file__).resolve().parent.parent
    schemas_dir = root / "schemas"
    
    health_schema = json.loads((schemas_dir / "health_response.json").read_text(encoding="utf-8"))
    embed_resp_schema = json.loads((schemas_dir / "api_v1_embed_response.json").read_text(encoding="utf-8"))
    
    print("=== Pilar 2 Contract Verification (Python Consumer) ===")
    
    # 1. Test /health against schema
    try:
        with urllib.request.urlopen("http://127.0.0.1:4000/health", timeout=5) as r:
            health_data = json.loads(r.read())
            validate_object(health_data, health_schema)
            print(f"✅ GET /health payload complies with schemas/health_response.json: {health_data}")
    except Exception as e:
        print(f"❌ GET /health contract validation failed: {e}")
        sys.exit(1)

    # 2. Test POST /api/v1/embed against schema
    try:
        req_body = json.dumps({"text": "contract verification text"}).encode("utf-8")
        req = urllib.request.Request(
            "http://127.0.0.1:4000/api/v1/embed",
            data=req_body,
            headers={"Content-Type": "application/json"}
        )
        with urllib.request.urlopen(req, timeout=10) as r:
            embed_data = json.loads(r.read())
            validate_object(embed_data, embed_resp_schema)
            dim = embed_data.get("dimension")
            model = embed_data.get("model")
            vec_len = len(embed_data.get("embedding", []))
            print(f"✅ POST /api/v1/embed payload complies with schemas/api_v1_embed_response.json: model={model}, dimension={dim}, vector_len={vec_len}")
    except Exception as e:
        print(f"❌ POST /api/v1/embed contract validation failed: {e}")
        sys.exit(1)

    print("\n🎯 All live contracts verified successfully!")

if __name__ == "__main__":
    main()

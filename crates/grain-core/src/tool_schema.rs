//! Short-lived, offline validators for untrusted MCP tool inputs.
//! The library implements JSON Schema; our preflight only bounds resources and
//! rejects reference/dialect features outside Grain's supported profile.

use serde_json::Value;

pub const MAX_ARGUMENT_BYTES: usize = 64 * 1024;
const MAX_NODES: usize = 4096;
const MAX_DEPTH: usize = 32;

pub fn validate_definition(schema: &Value) -> Result<(), String> {
    compile(schema).map(|_| ())
}

pub fn parse_arguments(raw: &str, schema: &Value) -> Result<Value, String> {
    if raw.len() > MAX_ARGUMENT_BYTES {
        return Err("arguments exceed the 64 KiB limit".into());
    }
    let arguments: Value = serde_json::from_str(raw).map_err(|_| "arguments are not valid JSON")?;
    validate_arguments(schema, &arguments)?;
    Ok(arguments)
}

pub fn validate_arguments(schema: &Value, arguments: &Value) -> Result<(), String> {
    if !arguments.is_object() {
        return Err("arguments must be a JSON object".into());
    }
    bounded_json(arguments, false)?;
    let validator = compile(schema)?;
    validator.validate(arguments).map_err(|error| {
        // Display includes the rejected value; the instance path can include
        // secret-bearing dynamic object keys. Report only schema metadata.
        let path: String = error
            .schema_path()
            .to_string()
            .chars()
            .take(128)
            .flat_map(char::escape_default)
            .collect();
        format!("arguments do not match the provider schema rule at '{path}'")
    })
}

fn compile(schema: &Value) -> Result<jsonschema::Validator, String> {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err("the MCP input schema must declare an object".into());
    }
    bounded_json(schema, true)?;
    let draft = match schema.get("$schema").and_then(Value::as_str) {
        None | Some("https://json-schema.org/draft/2020-12/schema") => {
            jsonschema::Draft::Draft202012
        }
        Some("https://json-schema.org/draft/2019-09/schema") => jsonschema::Draft::Draft201909,
        Some("http://json-schema.org/draft-07/schema#")
        | Some("https://json-schema.org/draft-07/schema#") => jsonschema::Draft::Draft7,
        _ => return Err("the MCP schema declares an unsupported dialect".into()),
    };
    profile(schema, schema, &mut Vec::new(), 0, &mut 0, &mut 0)?;
    jsonschema::options()
        .with_draft(draft)
        .offline()
        // format remains an annotation; do not silently introduce assertions.
        .should_validate_formats(false)
        // Linear-time engine with explicit per-pattern compilation/cache limits.
        // Backreferences/look-around fail compilation instead of backtracking.
        .with_pattern_options(
            jsonschema::PatternOptions::regex()
                .size_limit(64 * 1024)
                .dfa_size_limit(64 * 1024),
        )
        .build(schema)
        .map_err(|_| "the provider schema is invalid or uses an unsupported pattern".into())
}

fn hidden(character: char) -> bool {
    (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        || matches!(character, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
            | '\u{200B}'..='\u{200F}' | '\u{FEFF}')
}

fn bounded_json(value: &Value, schema: bool) -> Result<(), String> {
    bounded_value(value, schema, 0, &mut 0)?;
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            if self.0 > MAX_ARGUMENT_BYTES {
                return Err(std::io::Error::other("tool JSON byte budget"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Count encoded bytes without allocating a second copy of untrusted data.
    serde_json::to_writer(Budget(0), value).map_err(|_| "tool JSON exceeds the 64 KiB limit".into())
}

fn bounded_value(
    value: &Value,
    schema: bool,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH || *nodes >= MAX_NODES {
        return Err("tool JSON exceeds the nesting or node budget".into());
    }
    *nodes += 1;
    match value {
        Value::String(text) if text.len() > MAX_ARGUMENT_BYTES => {
            return Err("tool JSON exceeds the 64 KiB limit".into());
        }
        Value::String(text) if schema && (text.len() > 4096 || text.chars().any(hidden)) => {
            return Err("a schema string exceeds its budget or contains hidden controls".into());
        }
        Value::Array(values) => {
            for value in values {
                bounded_value(value, schema, depth + 1, nodes)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if key.len() > MAX_ARGUMENT_BYTES {
                    return Err("tool JSON exceeds the 64 KiB limit".into());
                }
                if schema
                    && (key.len() > 512 || key.chars().any(|ch| ch.is_control() || hidden(ch)))
                {
                    return Err("a schema property name is invalid".into());
                }
                bounded_value(value, schema, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

// Traverse only schema-bearing keywords: an enum/default/example may legitimately
// contain data named "$ref". Local JSON Pointer references are expanded for the
// budget audit, never copied or retained. Cycles and fan-out over budget refuse.
fn profile<'a>(
    root: &'a Value,
    schema: &'a Value,
    active: &mut Vec<&'a Value>,
    depth: usize,
    nodes: &mut usize,
    patterns: &mut usize,
) -> Result<(), String> {
    if depth > MAX_DEPTH || *nodes >= MAX_NODES {
        return Err("schema reference expansion exceeds the budget".into());
    }
    *nodes += 1;
    if active.iter().any(|node| std::ptr::eq(*node, schema)) {
        return Err("recursive tool schemas are not supported".into());
    }
    let Some(object) = schema.as_object() else {
        return Ok(());
    };
    active.push(schema);
    if !std::ptr::eq(root, schema) && object.contains_key("$schema") {
        return Err("embedded schema dialect switches are not supported".into());
    }
    *patterns += usize::from(object.contains_key("pattern"))
        + object
            .get("patternProperties")
            .and_then(Value::as_object)
            .map_or(0, |values| values.len());
    if *patterns > 16 {
        return Err("schema exceeds the 16-pattern compilation budget".into());
    }
    if [
        "$id",
        "$anchor",
        "$dynamicAnchor",
        "$dynamicRef",
        "$recursiveAnchor",
        "$recursiveRef",
        "$vocabulary",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
    {
        return Err("schema resource scopes and dynamic references are not supported".into());
    }
    if let Some(reference) = object.get("$ref") {
        let pointer = reference
            .as_str()
            .and_then(|value| value.strip_prefix('#'))
            .filter(|value| value.is_empty() || value.starts_with('/'))
            .filter(|value| !value.contains('%'))
            .ok_or("only local, unencoded JSON Pointer schema references are supported")?;
        let target = root
            .pointer(pointer)
            .ok_or("the schema reference does not exist")?;
        profile(root, target, active, depth + 1, nodes, patterns)?;
    }
    for keyword in [
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "dependentSchemas",
    ] {
        if let Some(children) = object.get(keyword).and_then(Value::as_object) {
            for child in children.values() {
                profile(root, child, active, depth + 1, nodes, patterns)?;
            }
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(children) = object.get(keyword).and_then(Value::as_array) {
            for child in children {
                profile(root, child, active, depth + 1, nodes, patterns)?;
            }
        }
    }
    for keyword in [
        "items",
        "additionalItems",
        "additionalProperties",
        "unevaluatedItems",
        "unevaluatedProperties",
        "contains",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(child) = object.get(keyword) {
            if let Some(children) = child.as_array() {
                for child in children {
                    profile(root, child, active, depth + 1, nodes, patterns)?;
                }
            } else {
                profile(root, child, active, depth + 1, nodes, patterns)?;
            }
        }
    }
    // Draft 7 dependencies can be schemas or arrays of property names.
    if let Some(children) = object.get("dependencies").and_then(Value::as_object) {
        for child in children.values().filter(|child| !child.is_array()) {
            profile(root, child, active, depth + 1, nodes, patterns)?;
        }
    }
    active.pop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nested_types_arrays_enums_and_bounds_are_asserted() {
        let schema = json!({"type": "object", "required": ["items"], "additionalProperties": false,
            "properties": {"items": {"type": "array", "minItems": 1, "maxItems": 2,
                "uniqueItems": true, "items": {"type": "object", "required": ["count", "kind"],
                    "additionalProperties": false, "properties": {
                        "count": {"type": "integer", "minimum": 1, "maximum": 3},
                        "kind": {"enum": ["read", "write"]}}}}}});
        validate_arguments(&schema, &json!({"items": [{"count": 1, "kind": "read"}]})).unwrap();
        for arguments in [
            json!({}),
            json!({"items": []}),
            json!({"items": [{"count": "1", "kind": "read"}]}),
            json!({"items": [{"count": 1.5, "kind": "read"}]}),
            json!({"items": [{"count": 0, "kind": "read"}]}),
            json!({"items": [{"count": 1, "kind": "other"}]}),
            json!({"items": [{"count": 1, "kind": "read", "extra": true}]}),
            json!({"items": [{"count": 1, "kind": "read"}, {"count": 1, "kind": "read"}]}),
            json!({"items": [{"count": 1, "kind": "read"}], "extra": true}),
        ] {
            assert!(
                validate_arguments(&schema, &arguments).is_err(),
                "{arguments}"
            );
        }
    }

    #[test]
    fn composition_conditionals_and_unevaluated_properties_are_asserted() {
        let schema = json!({"type": "object", "allOf": [{"required": ["mode"],
            "properties": {"mode": {"enum": ["read", "write"]}}}],
            "if": {"properties": {"mode": {"const": "write"}}},
            "then": {"required": ["count"], "properties": {"count": {"type": "integer"}}},
            "else": {"properties": {"query": {"type": "string", "minLength": 2}}},
            "unevaluatedProperties": false});
        validate_arguments(&schema, &json!({"mode": "read", "query": "OK"})).unwrap();
        validate_arguments(&schema, &json!({"mode": "write", "count": 2})).unwrap();
        for args in [
            json!({"mode": "write"}),
            json!({"mode": "read", "query": "x"}),
            json!({"mode": "read", "extra": true}),
        ] {
            assert!(validate_arguments(&schema, &args).is_err());
        }
        let schema = json!({"type": "object", "properties": {"value": {
            "oneOf": [{"type": "integer"}, {"type": "string", "pattern": "^ok$"}]}}});
        validate_arguments(&schema, &json!({"value": "ok"})).unwrap();
        assert!(validate_arguments(&schema, &json!({"value": "other"})).is_err());
    }

    #[test]
    fn local_references_work_without_treating_literal_data_as_schemas() {
        let schema = json!({"type": "object", "$defs": {"count": {"type": "integer"}},
            "properties": {"count": {"$ref": "#/$defs/count"},
                "literal": {"const": {"$ref": "https://example.invalid/data"}}}});
        validate_arguments(
            &schema,
            &json!({"count": 3, "literal": {"$ref": "https://example.invalid/data"}}),
        )
        .unwrap();
        assert!(validate_arguments(&schema, &json!({"count": "3"})).is_err());
    }

    #[test]
    fn unsupported_references_and_patterns_fail_closed() {
        for child in [
            json!({"$ref": "https://example.invalid/schema"}),
            json!({"$ref": "file:///private/credentials"}),
            json!({"$ref": "#"}),
            json!({"$ref": "#/$defs/missing"}),
            json!({"$dynamicRef": "#"}),
            json!({"type": "string", "pattern": "(?=secret)secret"}),
            json!({"$id": "another-resource", "type": "string"}),
        ] {
            assert!(
                validate_definition(&json!({"type": "object", "properties": {"x": child}}))
                    .is_err()
            );
        }
        let mut definitions = serde_json::Map::new();
        for i in 0..13 {
            definitions.insert(
                format!("n{i}"),
                if i == 12 {
                    json!({"type": "string"})
                } else {
                    json!({"allOf": [{"$ref": format!("#/$defs/n{}", i + 1)},
                    {"$ref": format!("#/$defs/n{}", i + 1)}]})
                },
            );
        }
        assert!(
            validate_definition(&json!({"type": "object", "$defs": definitions,
            "properties": {"x": {"$ref": "#/$defs/n0"}}}))
            .is_err()
        );
    }

    #[test]
    fn malformed_keywords_and_unknown_dialects_are_rejected() {
        for schema in [
            json!({"type": "object", "required": [3]}),
            json!({"type": "object", "properties": {"x": {"type": "integer", "minimum": "bad"}}}),
            json!({"type": "object", "$schema": "https://example.invalid/custom"}),
            json!({"type": "object", "properties": {"x": {"$schema": "https://example.invalid/custom"}}}),
        ] {
            assert!(validate_definition(&schema).is_err());
        }
    }

    #[test]
    fn budgets_cover_arguments_definitions_and_regex_compilation() {
        let schema = json!({"type": "object"});
        assert!(parse_arguments(&" ".repeat(MAX_ARGUMENT_BYTES + 1), &schema).is_err());
        assert!(
            validate_arguments(&schema, &json!({"x": "s".repeat(MAX_ARGUMENT_BYTES)})).is_err()
        );
        assert!(validate_arguments(&schema, &json!({"x": vec![true; MAX_NODES]})).is_err());
        let mut deep = json!(true);
        for _ in 0..MAX_DEPTH + 2 {
            deep = json!({"x": deep});
        }
        assert!(validate_arguments(&schema, &deep).is_err());
        assert!(
            validate_definition(&json!({"type": "object", "description": "s".repeat(4097)}))
                .is_err()
        );
        let properties: serde_json::Map<_, _> = (0..MAX_NODES)
            .map(|i| (format!("p{i}"), json!(true)))
            .collect();
        assert!(validate_definition(&json!({"type": "object", "properties": properties})).is_err());
        let properties: serde_json::Map<_, _> = (0..20)
            .map(|i| (format!("p{i}"), json!({"description": "s".repeat(4000)})))
            .collect();
        assert!(validate_definition(&json!({"type": "object", "properties": properties})).is_err());
        let patterns: serde_json::Map<_, _> = (0..17)
            .map(|i| (format!("p{i}"), json!({"type": "string", "pattern": "^x$"})))
            .collect();
        assert!(validate_definition(&json!({"type": "object", "properties": patterns})).is_err());
    }

    #[test]
    fn errors_do_not_echo_sensitive_values_and_validation_does_not_coerce() {
        let schema = json!({"type": "object", "properties": {"token": {"type": "integer"}}});
        let arguments = json!({"token": "private-token"});
        let original = arguments.clone();
        let error = validate_arguments(&schema, &arguments).unwrap_err();
        assert!(!error.contains("private-token"));
        assert_eq!(arguments, original);
        assert!(error.contains("/token"));
        let dynamic = json!({"type": "object", "additionalProperties": {"type": "integer"}});
        let error = validate_arguments(
            &dynamic,
            &json!({"private-key-token": "private-value-token"}),
        )
        .unwrap_err();
        assert!(!error.contains("private-key-token"));
        assert!(!error.contains("private-value-token"));
        assert!(parse_arguments("[]", &schema).is_err());
        assert!(parse_arguments("invalid-json", &schema).is_err());
    }

    #[test]
    fn format_is_an_annotation_and_explicit_supported_dialects_work() {
        for dialect in [
            "https://json-schema.org/draft/2020-12/schema",
            "https://json-schema.org/draft/2019-09/schema",
            "http://json-schema.org/draft-07/schema#",
        ] {
            let schema = json!({"$schema": dialect, "type": "object",
                "properties": {"email": {"type": "string", "format": "email"}}});
            validate_arguments(&schema, &json!({"email": "not-an-email"})).unwrap();
        }
    }
}

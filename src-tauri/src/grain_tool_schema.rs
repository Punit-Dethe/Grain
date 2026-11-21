//! [GRAIN] Tool schemas as a provider will accept them.
//!
//! MCP servers publish JSON Schemas with extras no model needs: vendor
//! display hints (Composio's `human_parameter_name` /
//! `human_parameter_description`), `x-*` extensions, `$schema`, `$comment`.
//! OpenAI ignores unknown keys; Gemini (and gateways in front of it, such as
//! Antigravity) rejects the whole request: `Unknown name "..." Cannot find
//! field`. These functions rewrite only the copy sent to the model. The
//! original schema still validates the model's arguments before any call runs.
//!
//! Two tiers:
//! - [`portable`], always: keep standard JSON Schema keywords, drop the rest,
//!   and fold a display hint into `description` / `title` when those are
//!   missing, so nothing the model could use is lost.
//! - [`strict`], for an endpoint that rejected a portable schema: Gemini's
//!   OpenAPI subset. `$ref`s are inlined, `allOf` merged, `oneOf` becomes
//!   `anyOf`, type unions become `nullable`/`anyOf`, and constraints it cannot
//!   express (an unsupported `format`, non-string `enum`s, `const`) are moved
//!   into the description in words.

use serde_json::{Map, Value};

/// Standard JSON Schema keywords (draft-07 / 2020-12) plus OpenAPI's
/// `nullable`/`example`.
const PORTABLE_KEYS: &[&str] = &[
    "type",
    "description",
    "title",
    "properties",
    "required",
    "items",
    "prefixItems",
    "additionalProperties",
    "patternProperties",
    "propertyNames",
    "enum",
    "const",
    "default",
    "examples",
    "example",
    "format",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
    "contains",
    "minProperties",
    "maxProperties",
    "anyOf",
    "oneOf",
    "allOf",
    "not",
    "nullable",
    "$ref",
    "$defs",
    "definitions",
];

/// Gemini's `Schema` fields (OpenAPI 3.0 subset).
const STRICT_KEYS: &[&str] = &[
    "type",
    "format",
    "title",
    "description",
    "nullable",
    "enum",
    "maxItems",
    "minItems",
    "properties",
    "required",
    "minProperties",
    "maxProperties",
    "minLength",
    "maxLength",
    "pattern",
    "example",
    "anyOf",
    "default",
    "items",
    "minimum",
    "maximum",
];

/// Vendor display hints, folded into the standard keyword when it is absent.
const DESCRIPTION_HINTS: &[&str] = &[
    "human_parameter_description",
    "x-description",
    "x-human-description",
];
const TITLE_HINTS: &[&str] = &["human_parameter_name", "x-display-name", "x-title"];

/// `$ref` chains deeper than this (or cyclic) stop as a plain object.
const MAX_REF_DEPTH: usize = 8;

/// Standard keywords only; display hints folded in. Always safe to send.
pub(crate) fn portable(schema: &Value) -> Value {
    let Value::Object(node) = schema else {
        return schema.clone();
    };
    let mut out = Map::new();
    for (key, value) in node {
        if !PORTABLE_KEYS.contains(&key.as_str()) {
            continue;
        }
        let value = match key.as_str() {
            "properties" | "patternProperties" | "$defs" | "definitions" => {
                map_schemas(value, portable)
            }
            "items" | "prefixItems" | "anyOf" | "oneOf" | "allOf" => each_schema(value, portable),
            "additionalProperties" | "propertyNames" | "contains" | "not" => portable(value),
            _ => value.clone(),
        };
        out.insert(key.clone(), value);
    }
    fold_hints(node, &mut out);
    Value::Object(out)
}

/// Gemini's subset. `schema` is the whole tool schema (its `$defs` resolve
/// `$ref`s).
pub(crate) fn strict(schema: &Value) -> Value {
    let root = schema.clone();
    let mut out = strict_node(&root, schema, 0);
    // A tool's parameters are always an object.
    if let Value::Object(node) = &mut out {
        node.entry("type").or_insert_with(|| "object".into());
    }
    out
}

/// Whether an error body says the tool schema itself was refused.
pub(crate) fn is_schema_rejection(body: &str) -> bool {
    let body = body.to_ascii_lowercase();
    [
        "unknown name",
        "cannot find field",
        "function_declarations",
        "invalid json payload",
        "schema",
        "tools[",
    ]
    .iter()
    .any(|needle| body.contains(needle))
}

fn map_schemas(value: &Value, f: fn(&Value) -> Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), f(v))).collect()),
        other => other.clone(),
    }
}

fn each_schema(value: &Value, f: fn(&Value) -> Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(f).collect()),
        other => f(other),
    }
}

fn fold_hints(node: &Map<String, Value>, out: &mut Map<String, Value>) {
    for (keyword, hints) in [("description", DESCRIPTION_HINTS), ("title", TITLE_HINTS)] {
        if out
            .get(keyword)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
        {
            continue;
        }
        if let Some(hint) = hints
            .iter()
            .find_map(|hint| node.get(*hint).and_then(Value::as_str))
            .filter(|hint| !hint.trim().is_empty())
        {
            out.insert(keyword.into(), hint.into());
        }
    }
}

fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let path = reference.strip_prefix("#/")?;
    path.split('/').try_fold(root, |node, part| {
        node.get(part.replace("~1", "/").replace("~0", "~"))
    })
}

fn add_note(out: &mut Map<String, Value>, note: String) {
    let text = match out.get("description").and_then(Value::as_str) {
        Some(existing) if !existing.trim().is_empty() => format!("{} ({note})", existing.trim()),
        _ => note,
    };
    out.insert("description".into(), text.into());
}

fn strict_node(root: &Value, schema: &Value, depth: usize) -> Value {
    let Value::Object(node) = schema else {
        // `true` / `false` schemas: anything goes.
        return Value::Object(Map::new());
    };
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        let target = (depth < MAX_REF_DEPTH)
            .then(|| resolve(root, reference))
            .flatten();
        let mut out = match target {
            Some(target) => strict_node(root, target, depth + 1),
            None => serde_json::json!({ "type": "object" }),
        };
        // Sibling keywords (a description beside the `$ref`) win.
        let mut siblings = node.clone();
        siblings.remove("$ref");
        if let (Value::Object(out), Value::Object(extra)) = (
            &mut out,
            strict_node(root, &Value::Object(siblings), depth + 1),
        ) {
            out.extend(extra.into_iter().filter(|(_, v)| !v.is_null()));
        }
        return out;
    }

    let mut node = node.clone();
    if let Some(Value::Array(parts)) = node.remove("allOf") {
        merge_all_of(&mut node, &parts, root, depth);
    }
    if let Some(one_of) = node.remove("oneOf") {
        node.entry("anyOf").or_insert(one_of);
    }

    let mut out = Map::new();
    let mut nullable = node
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Type unions: `null` becomes `nullable`, several others become `anyOf`.
    match node.get("type") {
        Some(Value::Array(types)) => {
            let named: Vec<&str> = types.iter().filter_map(Value::as_str).collect();
            nullable |= named.contains(&"null");
            let real: Vec<&str> = named.into_iter().filter(|t| *t != "null").collect();
            match real.as_slice() {
                [] => {}
                [one] => {
                    out.insert("type".into(), (*one).into());
                }
                many => {
                    out.insert(
                        "anyOf".into(),
                        Value::Array(
                            many.iter()
                                .map(|t| serde_json::json!({ "type": t }))
                                .collect(),
                        ),
                    );
                }
            }
        }
        Some(Value::String(t)) if t == "null" => nullable = true,
        Some(t @ Value::String(_)) => {
            out.insert("type".into(), t.clone());
        }
        _ => {}
    }
    let kind = out
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    for (key, value) in &node {
        match key.as_str() {
            "type" | "nullable" | "required" => {}
            "properties" => {
                if let Value::Object(props) = value {
                    out.insert(
                        key.clone(),
                        Value::Object(
                            props
                                .iter()
                                .map(|(name, prop)| (name.clone(), strict_node(root, prop, depth)))
                                .collect(),
                        ),
                    );
                }
            }
            "items" => {
                // A tuple's first slot stands in for the list.
                let item = match value {
                    Value::Array(slots) => slots.first().cloned().unwrap_or(Value::Bool(true)),
                    other => other.clone(),
                };
                out.insert(key.clone(), strict_node(root, &item, depth));
            }
            "anyOf" => {
                if let Value::Array(options) = value {
                    let mut kept = Vec::new();
                    for option in options {
                        if option.get("type").and_then(Value::as_str) == Some("null") {
                            nullable = true;
                        } else {
                            kept.push(strict_node(root, option, depth));
                        }
                    }
                    match kept.len() {
                        0 => {}
                        1 => {
                            if let Value::Object(only) = kept.remove(0) {
                                for (k, v) in only {
                                    out.entry(k).or_insert(v);
                                }
                            }
                        }
                        _ => {
                            out.insert(key.clone(), Value::Array(kept));
                        }
                    }
                }
            }
            "enum" => match value {
                Value::Array(values) if values.iter().all(Value::is_string) => {
                    out.insert(key.clone(), value.clone());
                }
                Value::Array(values) => {
                    nullable |= values.iter().any(Value::is_null);
                    let allowed: Vec<String> = values
                        .iter()
                        .filter(|v| !v.is_null())
                        .map(Value::to_string)
                        .collect();
                    if !allowed.is_empty() {
                        add_note(&mut out, format!("one of: {}", allowed.join(", ")));
                    }
                }
                _ => {}
            },
            "const" => match value {
                Value::String(_) => {
                    out.insert("enum".into(), Value::Array(vec![value.clone()]));
                }
                other => add_note(&mut out, format!("must be {other}")),
            },
            "format" => {
                let supported = match kind.as_str() {
                    "string" => matches!(value.as_str(), Some("enum" | "date-time")),
                    "number" => matches!(value.as_str(), Some("float" | "double")),
                    "integer" => matches!(value.as_str(), Some("int32" | "int64")),
                    _ => false,
                };
                if supported {
                    out.insert(key.clone(), value.clone());
                } else if let Some(format) = value.as_str() {
                    add_note(&mut out, format!("format: {format}"));
                }
            }
            "examples" => {
                if let Some(first) = value.as_array().and_then(|v| v.first()) {
                    out.entry("example").or_insert_with(|| first.clone());
                }
            }
            "exclusiveMinimum" if value.is_number() => {
                out.entry("minimum").or_insert_with(|| value.clone());
            }
            "exclusiveMaximum" if value.is_number() => {
                out.entry("maximum").or_insert_with(|| value.clone());
            }
            "description" | "title" => {
                if let Some(text) = value.as_str() {
                    // A note added earlier (enum, format) stays after the text.
                    match out.get(key).and_then(Value::as_str) {
                        Some(note) if key == "description" => {
                            let text = format!("{} ({note})", text.trim());
                            out.insert(key.clone(), text.into());
                        }
                        _ => {
                            out.insert(key.clone(), text.into());
                        }
                    }
                }
            }
            _ if STRICT_KEYS.contains(&key.as_str()) => {
                out.insert(key.clone(), value.clone());
            }
            _ => {}
        }
    }

    // `required` may only name properties that exist.
    if let Some(Value::Array(required)) = node.get("required") {
        let props = out.get("properties").and_then(Value::as_object);
        let kept: Vec<Value> = required
            .iter()
            .filter(|name| {
                name.as_str()
                    .is_some_and(|n| props.is_some_and(|p| p.contains_key(n)))
            })
            .cloned()
            .collect();
        if !kept.is_empty() {
            out.insert("required".into(), Value::Array(kept));
        }
    }
    if nullable {
        out.insert("nullable".into(), true.into());
    }
    fold_hints(&node, &mut out);
    Value::Object(out)
}

/// Fold `allOf` parts into `node`: properties and `required` union, other
/// keywords keep the first value seen.
fn merge_all_of(node: &mut Map<String, Value>, parts: &[Value], root: &Value, depth: usize) {
    for part in parts {
        let part = match part.get("$ref").and_then(Value::as_str) {
            Some(reference) if depth < MAX_REF_DEPTH => {
                resolve(root, reference).cloned().unwrap_or_default()
            }
            _ => part.clone(),
        };
        let Value::Object(part) = part else { continue };
        for (key, value) in part {
            let Some(mine) = node.get_mut(&key) else {
                node.insert(key, value);
                continue;
            };
            match (key.as_str(), mine, value) {
                ("properties", Value::Object(mine), Value::Object(theirs)) => {
                    for (name, prop) in theirs {
                        mine.entry(name).or_insert(prop);
                    }
                }
                ("required", Value::Array(mine), Value::Array(theirs)) => {
                    for name in theirs {
                        if !mine.contains(&name) {
                            mine.push(name);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn composio() -> Value {
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "format": "email",
                    "human_parameter_name": "Recipient",
                    "human_parameter_description": "Who receives the email",
                    "x-ui": { "widget": "email" }
                },
                "cc": {
                    "type": ["array", "null"],
                    "items": { "type": "string" },
                    "description": "Copies",
                    "human_parameter_description": "ignored: description exists"
                },
                "priority": { "enum": [1, 2, 3] },
                "mode": { "const": "draft" },
                "body": { "$ref": "#/$defs/Body", "description": "The message" }
            },
            "required": ["to", "ghost"],
            "additionalProperties": false,
            "$defs": { "Body": { "type": "string", "minLength": 1 } }
        })
    }

    #[test]
    fn portable_drops_vendor_keys_and_folds_display_hints() {
        let out = portable(&composio());
        assert!(out.get("$schema").is_none());
        let to = &out["properties"]["to"];
        assert!(to.get("human_parameter_name").is_none());
        assert!(to.get("x-ui").is_none());
        assert_eq!(to["description"], "Who receives the email");
        assert_eq!(to["title"], "Recipient");
        assert_eq!(out["properties"]["cc"]["description"], "Copies");
        // Standard keywords survive untouched.
        assert_eq!(out["additionalProperties"], false);
        assert_eq!(out["properties"]["body"]["$ref"], "#/$defs/Body");
        assert!(!out.to_string().contains("human_parameter"));
    }

    #[test]
    fn a_property_named_like_a_vendor_key_is_kept() {
        let schema = json!({
            "type": "object",
            "properties": { "human_parameter_name": { "type": "string" } }
        });
        assert!(portable(&schema)["properties"]
            .get("human_parameter_name")
            .is_some());
    }

    #[test]
    fn strict_fits_gemini_and_keeps_meaning_in_words() {
        let out = strict(&portable(&composio()));
        let text = out.to_string();
        for banned in ["$ref", "$defs", "additionalProperties", "$schema", "const"] {
            assert!(!text.contains(banned), "{banned} in {text}");
        }
        let props = &out["properties"];
        assert_eq!(props["to"]["type"], "string");
        assert!(props["to"].get("format").is_none());
        assert!(props["to"]["description"]
            .as_str()
            .unwrap()
            .contains("format: email"));
        assert_eq!(props["cc"]["type"], "array");
        assert_eq!(props["cc"]["nullable"], true);
        assert!(props["priority"].get("enum").is_none());
        assert!(props["priority"]["description"]
            .as_str()
            .unwrap()
            .contains("1, 2, 3"));
        assert_eq!(props["mode"]["enum"], json!(["draft"]));
        assert_eq!(props["body"]["type"], "string");
        assert_eq!(props["body"]["minLength"], 1);
        assert_eq!(props["body"]["description"], "The message");
        assert_eq!(out["required"], json!(["to"]));
    }

    #[test]
    fn strict_survives_cycles_and_merges_all_of() {
        let schema = json!({
            "type": "object",
            "properties": {
                "node": { "$ref": "#/$defs/Node" },
                "both": { "allOf": [
                    { "type": "object", "properties": { "a": { "type": "string" } }, "required": ["a"] },
                    { "properties": { "b": { "type": "integer" } }, "required": ["b"] }
                ] },
                "pick": { "oneOf": [ { "type": "string" }, { "type": "integer" } ] }
            },
            "$defs": { "Node": { "type": "object", "properties": { "next": { "$ref": "#/$defs/Node" } } } }
        });
        let out = strict(&schema);
        assert!(!out.to_string().contains("$ref"));
        let both = &out["properties"]["both"];
        assert!(both["properties"].get("a").is_some() && both["properties"].get("b").is_some());
        assert_eq!(both["required"], json!(["a", "b"]));
        assert_eq!(
            out["properties"]["pick"]["anyOf"].as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn schema_rejections_are_recognised() {
        assert!(is_schema_rejection(
            r#"Unknown name "human_parameter_name" at 'request.tools[0].function_declarations[5]': Cannot find field."#
        ));
        assert!(!is_schema_rejection("Incorrect API key provided"));
    }
}

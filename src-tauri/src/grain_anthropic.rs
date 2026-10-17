//! Anthropic wire adaptation for Grain's shared plain/image/tool transport.
use serde_json::{json, Value};

pub(crate) fn request(input: &Value) -> Result<Value, String> {
    let mut messages = Vec::new();
    let mut system = Vec::new();
    for message in input["messages"].as_array().ok_or("Missing messages")? {
        let role = message["role"].as_str().ok_or("Missing message role")?;
        if role == "system" {
            if let Some(text) = message["content"].as_str() {
                system.push(text);
            }
            continue;
        }
        let mut content = Vec::new();
        if role == "tool" {
            content.push(
                json!({"type":"tool_result", "tool_use_id":message["tool_call_id"],
                "content":message["content"]}),
            );
        } else if let Some(text) = message["content"].as_str() {
            if !text.is_empty() {
                content.push(json!({"type":"text", "text":text}));
            }
        } else if let Some(parts) = message["content"].as_array() {
            for part in parts {
                match part["type"].as_str() {
                    Some("text") => content.push(part.clone()),
                    Some("image_url") => {
                        let uri = part["image_url"]["url"].as_str().ok_or("Missing image")?;
                        let (mime, data) = uri
                            .strip_prefix("data:")
                            .and_then(|uri| uri.split_once(";base64,"))
                            .ok_or("Anthropic images require base64 data")?;
                        content.push(json!({"type":"image", "source":{
                            "type":"base64", "media_type":mime, "data":data}}));
                    }
                    _ => return Err("Unsupported Anthropic content part".into()),
                }
            }
        }
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                let arguments = call["function"]["arguments"]
                    .as_str()
                    .ok_or("Missing tool arguments")?;
                let arguments: Value =
                    serde_json::from_str(arguments).map_err(|e| e.to_string())?;
                content.push(json!({"type":"tool_use", "id":call["id"],
                    "name":call["function"]["name"], "input":arguments}));
            }
        }
        if !content.is_empty() {
            let role = if role == "tool" { "user" } else { role };
            // Multiple tool results belong to one user turn.
            if let Some(last) = messages
                .last_mut()
                .filter(|last: &&mut Value| last["role"] == role)
            {
                last["content"].as_array_mut().unwrap().extend(content);
            } else {
                messages.push(json!({"role":role, "content":content}));
            }
        }
    }
    let mut body = json!({"model":input["model"], "messages":messages,
        "max_tokens":4096, "stream":false});
    if !system.is_empty() {
        body["system"] = json!(system.join("\n\n"));
    }
    if let Some(tools) = input["tools"].as_array().filter(|tools| !tools.is_empty()) {
        body["tools"] = tools
            .iter()
            .map(|tool| {
                json!({
                    "name":tool["function"]["name"], "description":tool["function"]["description"],
                    "input_schema":tool["function"]["parameters"]
                })
            })
            .collect();
        body["tool_choice"] = json!({"type":"auto"});
    }
    Ok(body)
}

pub(crate) fn response(input: Value) -> Result<Value, String> {
    if input["stop_reason"] == "max_tokens" {
        return Err("Response was truncated at the provider's output token limit".into());
    }
    let blocks = input["content"]
        .as_array()
        .ok_or("Missing Anthropic content")?;
    let mut text = String::new();
    let mut calls = Vec::new();
    for block in blocks {
        match block["type"].as_str() {
            Some("text") => text.push_str(block["text"].as_str().ok_or("Missing text")?),
            Some("tool_use") => calls.push(json!({"id":block["id"], "type":"function",
                "function":{"name":block["name"], "arguments":serde_json::to_string(&block["input"])
                    .map_err(|e| e.to_string())?}})),
            _ => {} // Thinking and other non-text blocks never become pasted text.
        }
    }
    Ok(json!({"choices":[{"message":{
        "content":if text.is_empty() { Value::Null } else { json!(text) },
        "tool_calls":calls
    }}]}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_and_tool_turns_use_anthropic_shapes_and_no_openai_reasoning_fields() {
        let body = request(&json!({"model":"claude", "reasoning_effort":"none", "messages":[
            {"role":"system","content":"Be concise"},
            {"role":"user","content":[{"type":"text","text":"Read this"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,abc"}}]},
            {"role":"assistant","tool_calls":[{"id":"call1","function":{"name":"lookup","arguments":"{\"key\":1}"}}]},
            {"role":"tool","tool_call_id":"call1","content":"done"}
        ],"tools":[{"function":{"name":"lookup","description":"Lookup","parameters":{"type":"object"}}}]})).unwrap();
        assert_eq!(body["system"], "Be concise");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(
            body["messages"][0]["content"][1]["source"]["media_type"],
            "image/png"
        );
        assert_eq!(body["messages"][1]["content"][0]["input"]["key"], 1);
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "call1");
        assert!(body.get("reasoning_effort").is_none());
    }
    #[test]
    fn response_keeps_text_and_tools_but_rejects_truncated_output() {
        let body = response(json!({"content":[{"type":"thinking","thinking":"private"},
            {"type":"text","text":"Hello."},
            {"type":"tool_use","id":"call1","name":"lookup","input":{"key":1}}]}))
        .unwrap();
        assert_eq!(body["choices"][0]["message"]["content"], "Hello.");
        assert_eq!(
            body["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"key\":1}"
        );
        assert!(response(json!({"stop_reason":"max_tokens","content":[]})).is_err());
    }
}

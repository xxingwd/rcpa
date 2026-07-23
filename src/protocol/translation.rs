use crate::config::ProviderProtocol;
use crate::error::{AppError, AppResult};
use crate::protocol::common::Operation;

pub fn source_protocol(operation: Operation) -> ProviderProtocol {
    operation.provider_protocol()
}

pub fn candidate_protocols(operation: Operation, _stream: bool) -> Vec<ProviderProtocol> {
    let native = source_protocol(operation);
    match operation {
        Operation::Completions => {
            vec![ProviderProtocol::Completions, ProviderProtocol::Messages]
        }
        Operation::Messages => {
            vec![ProviderProtocol::Messages, ProviderProtocol::Completions]
        }
        Operation::Responses | Operation::Embeddings | Operation::ListModels => vec![native],
    }
}

pub fn operation_for_target(
    source_operation: Operation,
    target_protocol: ProviderProtocol,
) -> Option<Operation> {
    let source_protocol = source_protocol(source_operation);
    if target_protocol == source_protocol {
        return Some(source_operation);
    }

    match (source_operation, target_protocol) {
        (Operation::Completions, ProviderProtocol::Messages) => Some(Operation::Messages),
        (Operation::Messages, ProviderProtocol::Completions) => Some(Operation::Completions),
        _ => None,
    }
}

pub fn translate_request_body(
    source_operation: Operation,
    target_protocol: ProviderProtocol,
    body: &serde_json::Value,
    model: &str,
    stream: bool,
) -> AppResult<serde_json::Value> {
    let source_protocol = source_protocol(source_operation);
    if source_protocol == target_protocol {
        return Ok(body.clone());
    }
    match (source_operation, target_protocol) {
        (Operation::Completions, ProviderProtocol::Messages) => {
            Ok(chat_completions_to_messages_request(body, model, stream))
        }
        (Operation::Messages, ProviderProtocol::Completions) => {
            Ok(messages_to_chat_completions_request(body, model, stream))
        }
        _ => Err(AppError::ProtocolError(format!(
            "Protocol conversion from '{}' to '{}' is not supported",
            source_protocol, target_protocol
        ))),
    }
}

pub fn translate_response_body(
    upstream_protocol: ProviderProtocol,
    entry_operation: Operation,
    body: serde_json::Value,
    public_model: &str,
) -> AppResult<serde_json::Value> {
    let entry_protocol = source_protocol(entry_operation);
    if upstream_protocol == entry_protocol {
        return Ok(body);
    }

    match (upstream_protocol, entry_operation) {
        (ProviderProtocol::Completions, Operation::Messages) => {
            Ok(chat_completions_to_messages_response(&body, public_model))
        }
        (ProviderProtocol::Messages, Operation::Completions) => {
            Ok(messages_to_chat_completions_response(&body, public_model))
        }
        _ => Err(AppError::ProtocolError(format!(
            "Protocol response conversion from '{}' to '{}' is not supported",
            upstream_protocol, entry_protocol
        ))),
    }
}

fn messages_to_chat_completions_request(
    body: &serde_json::Value,
    model: &str,
    stream: bool,
) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "model".to_string(),
        serde_json::Value::String(model.to_string()),
    );
    out.insert("stream".to_string(), serde_json::Value::Bool(stream));

    let mut messages = Vec::new();
    if let Some(system) = body.get("system") {
        let system_text = anthropic_content_to_text(system);
        if !system_text.is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system_text
            }));
        }
    }
    if let Some(input_messages) = body.get("messages").and_then(|value| value.as_array()) {
        for message in input_messages {
            messages.extend(anthropic_message_to_chat_messages(message));
        }
    }
    out.insert("messages".to_string(), serde_json::Value::Array(messages));

    copy_field(body, &mut out, "temperature", "temperature");
    copy_field(body, &mut out, "top_p", "top_p");
    copy_field(body, &mut out, "max_tokens", "max_tokens");
    copy_field(body, &mut out, "metadata", "metadata");
    copy_metadata_user_id_to_user(body, &mut out);
    if let Some(tools) = body.get("tools") {
        out.insert("tools".to_string(), anthropic_tools_to_openai_tools(tools));
    }
    if let Some(tool_choice) = body.get("tool_choice") {
        out.insert(
            "tool_choice".to_string(),
            anthropic_tool_choice_to_openai(tool_choice),
        );
    }
    if let Some(stop) = body.get("stop_sequences") {
        out.insert("stop".to_string(), stop.clone());
    }

    serde_json::Value::Object(out)
}

fn chat_completions_to_messages_request(
    body: &serde_json::Value,
    model: &str,
    stream: bool,
) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "model".to_string(),
        serde_json::Value::String(model.to_string()),
    );
    out.insert("stream".to_string(), serde_json::Value::Bool(stream));
    out.insert(
        "max_tokens".to_string(),
        body.get("max_tokens")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Number(serde_json::Number::from(1024))),
    );

    if let Some(system) = first_system_text(body) {
        out.insert("system".to_string(), serde_json::Value::String(system));
    }

    let messages = body
        .get("messages")
        .and_then(|value| value.as_array())
        .map(|messages| {
            messages
                .iter()
                .filter(|message| message.get("role").and_then(|v| v.as_str()) != Some("system"))
                .flat_map(chat_message_to_anthropic_messages)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    out.insert("messages".to_string(), serde_json::Value::Array(messages));

    copy_field(body, &mut out, "temperature", "temperature");
    copy_field(body, &mut out, "top_p", "top_p");
    if let Some(metadata) = messages_metadata_from_source(body) {
        out.insert("metadata".to_string(), metadata);
    }
    if let Some(tools) = body.get("tools") {
        out.insert("tools".to_string(), openai_tools_to_anthropic_tools(tools));
    }
    if let Some(tool_choice) = body.get("tool_choice") {
        out.insert(
            "tool_choice".to_string(),
            openai_tool_choice_to_anthropic(tool_choice),
        );
    }
    if let Some(stop) = body.get("stop") {
        out.insert("stop_sequences".to_string(), stop.clone());
    }

    serde_json::Value::Object(out)
}

fn chat_message_to_anthropic_messages(message: &serde_json::Value) -> Vec<serde_json::Value> {
    let role = message
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("user");
    if role == "tool" {
        return vec![serde_json::json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": message.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or(""),
                "content": message_content_to_output_text(message.get("content").unwrap_or(&serde_json::Value::Null))
            }]
        })];
    }

    let mut content = anthropic_content_array_from_openai(
        message.get("content").unwrap_or(&serde_json::Value::Null),
    );
    if let Some(reasoning) = message.get("reasoning") {
        content.push(serde_json::json!({
            "type": "thinking",
            "thinking": message_content_to_output_text(reasoning)
        }));
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(|value| value.as_array()) {
        content.extend(
            tool_calls
                .iter()
                .map(openai_tool_call_to_anthropic_tool_use),
        );
    }
    if content.is_empty() {
        content.push(serde_json::json!({ "type": "text", "text": "" }));
    }

    vec![serde_json::json!({
        "role": if role == "assistant" { "assistant" } else { "user" },
        "content": content
    })]
}

fn anthropic_message_to_chat_messages(message: &serde_json::Value) -> Vec<serde_json::Value> {
    let role = message
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("user");
    let content = message.get("content").unwrap_or(&serde_json::Value::Null);
    let mut messages = Vec::new();

    if let Some(parts) = content.as_array() {
        let mut normal_parts = Vec::new();
        let mut tool_calls = Vec::new();
        let mut reasoning = Vec::new();
        for part in parts {
            match part.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "tool_result" => messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": part.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "content": anthropic_tool_result_content_to_text(part.get("content").unwrap_or(&serde_json::Value::Null))
                })),
                "tool_use" => tool_calls.push(anthropic_tool_use_to_openai_tool_call(part)),
                "thinking" | "redacted_thinking" => reasoning.push(part.clone()),
                _ => normal_parts.push(part.clone()),
            }
        }

        if !normal_parts.is_empty() || !tool_calls.is_empty() || !reasoning.is_empty() {
            let mut out = serde_json::json!({
                "role": if role == "assistant" { "assistant" } else { "user" },
                "content": anthropic_content_to_openai_content(&serde_json::Value::Array(normal_parts))
            });
            if !tool_calls.is_empty() {
                out["tool_calls"] = serde_json::Value::Array(tool_calls);
            }
            if !reasoning.is_empty() {
                out["reasoning"] = serde_json::Value::Array(reasoning);
            }
            messages.insert(0, out);
        }
    } else {
        messages.push(serde_json::json!({
            "role": if role == "assistant" { "assistant" } else { "user" },
            "content": anthropic_content_to_openai_content(content)
        }));
    }

    messages
}

fn first_system_text(body: &serde_json::Value) -> Option<String> {
    body.get("messages")
        .and_then(|value| value.as_array())
        .and_then(|messages| {
            messages
                .iter()
                .find(|message| message.get("role").and_then(|v| v.as_str()) == Some("system"))
        })
        .and_then(|message| message.get("content"))
        .map(openai_content_to_text)
        .filter(|text| !text.is_empty())
}

fn openai_content_to_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(|v| v.as_str())
                    .or_else(|| part.get("content").and_then(|v| v.as_str()))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn anthropic_content_to_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn message_content_to_output_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .map(|part| {
                part.get("text")
                    .and_then(|value| value.as_str())
                    .or_else(|| part.get("content").and_then(|value| value.as_str()))
                    .map(str::to_string)
                    .unwrap_or_else(|| part.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn openai_content_to_anthropic_content(content: &serde_json::Value) -> serde_json::Value {
    match content {
        serde_json::Value::String(text) => {
            serde_json::json!([{ "type": "text", "text": text }])
        }
        serde_json::Value::Array(parts) => serde_json::Value::Array(
            parts
                .iter()
                .filter_map(|part| {
                    let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match part_type {
                        "text" | "input_text" => Some(serde_json::json!({
                            "type": "text",
                            "text": part.get("text").and_then(|v| v.as_str()).unwrap_or("")
                        })),
                        "image_url" => Some(openai_image_part_to_anthropic(part)),
                        _ => None,
                    }
                })
                .collect(),
        ),
        _ => serde_json::json!([]),
    }
}

fn anthropic_content_array_from_openai(content: &serde_json::Value) -> Vec<serde_json::Value> {
    openai_content_to_anthropic_content(content)
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn anthropic_content_to_openai_content(content: &serde_json::Value) -> serde_json::Value {
    match content {
        serde_json::Value::String(text) => serde_json::Value::String(text.clone()),
        serde_json::Value::Array(parts) => serde_json::Value::Array(
            parts
                .iter()
                .filter_map(anthropic_content_part_to_openai)
                .collect(),
        ),
        _ => serde_json::Value::String(String::new()),
    }
}

fn openai_image_part_to_anthropic(part: &serde_json::Value) -> serde_json::Value {
    let url = part
        .pointer("/image_url/url")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if let Some(data) = url.strip_prefix("data:") {
        let (media_type, data) = data.split_once(";base64,").unwrap_or(("image/png", data));
        serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media_type,
                "data": data
            }
        })
    } else {
        serde_json::json!({
            "type": "image",
            "source": {
                "type": "url",
                "url": url
            }
        })
    }
}

fn anthropic_content_part_to_openai(part: &serde_json::Value) -> Option<serde_json::Value> {
    match part
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("")
    {
        "text" => Some(serde_json::json!({
            "type": "text",
            "text": part.get("text").and_then(|v| v.as_str()).unwrap_or("")
        })),
        "image" => {
            let source = part.get("source").unwrap_or(&serde_json::Value::Null);
            let url = match source.get("type").and_then(|value| value.as_str()) {
                Some("base64") => format!(
                    "data:{};base64,{}",
                    source
                        .get("media_type")
                        .and_then(|value| value.as_str())
                        .unwrap_or("image/png"),
                    source
                        .get("data")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                ),
                _ => source
                    .get("url")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
            };
            Some(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": url }
            }))
        }
        _ => None,
    }
}

fn openai_tool_call_to_anthropic_tool_use(tool_call: &serde_json::Value) -> serde_json::Value {
    let arguments = tool_call
        .pointer("/function/arguments")
        .and_then(|value| value.as_str())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    serde_json::json!({
        "type": "tool_use",
        "id": tool_call.get("id").and_then(|value| value.as_str()).unwrap_or("call_0"),
        "name": tool_call.pointer("/function/name").and_then(|value| value.as_str()).unwrap_or(""),
        "input": arguments
    })
}

fn anthropic_tool_use_to_openai_tool_call(part: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": part.get("id").and_then(|value| value.as_str()).unwrap_or("call_0"),
        "type": "function",
        "function": {
            "name": part.get("name").and_then(|value| value.as_str()).unwrap_or(""),
            "arguments": part.get("input").cloned().unwrap_or_else(|| serde_json::json!({})).to_string()
        }
    })
}

fn anthropic_tool_result_content_to_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .map(|part| {
                part.get("text")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| part.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn openai_tools_to_anthropic_tools(tools: &serde_json::Value) -> serde_json::Value {
    serde_json::Value::Array(
        tools
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| {
                if tool.get("type").and_then(|value| value.as_str()) != Some("function") {
                    return None;
                }
                Some(serde_json::json!({
                    "name": tool.pointer("/function/name").and_then(|value| value.as_str()).unwrap_or(""),
                    "description": tool.pointer("/function/description").and_then(|value| value.as_str()).unwrap_or(""),
                    "input_schema": tool.pointer("/function/parameters").cloned().unwrap_or_else(|| serde_json::json!({ "type": "object" }))
                }))
            })
            .collect(),
    )
}

fn anthropic_tools_to_openai_tools(tools: &serde_json::Value) -> serde_json::Value {
    serde_json::Value::Array(
        tools
            .as_array()
            .into_iter()
            .flatten()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").and_then(|value| value.as_str()).unwrap_or(""),
                        "description": tool.get("description").and_then(|value| value.as_str()).unwrap_or(""),
                        "parameters": tool.get("input_schema").cloned().unwrap_or_else(|| serde_json::json!({ "type": "object" }))
                    }
                })
            })
            .collect(),
    )
}

fn openai_tool_choice_to_anthropic(tool_choice: &serde_json::Value) -> serde_json::Value {
    match tool_choice {
        serde_json::Value::String(value) if value == "auto" => {
            serde_json::json!({ "type": "auto" })
        }
        serde_json::Value::String(value) if value == "required" => {
            serde_json::json!({ "type": "any" })
        }
        serde_json::Value::String(value) if value == "none" => {
            serde_json::json!({ "type": "none" })
        }
        serde_json::Value::Object(_) => serde_json::json!({
            "type": "tool",
            "name": tool_choice.pointer("/function/name").and_then(|value| value.as_str()).unwrap_or("")
        }),
        _ => tool_choice.clone(),
    }
}

fn anthropic_tool_choice_to_openai(tool_choice: &serde_json::Value) -> serde_json::Value {
    match tool_choice.get("type").and_then(|value| value.as_str()) {
        Some("auto") => serde_json::json!("auto"),
        Some("any") => serde_json::json!("required"),
        Some("none") => serde_json::json!("none"),
        Some("tool") => serde_json::json!({
            "type": "function",
            "function": {
                "name": tool_choice.get("name").and_then(|value| value.as_str()).unwrap_or("")
            }
        }),
        _ => tool_choice.clone(),
    }
}

fn messages_to_chat_completions_response(
    body: &serde_json::Value,
    public_model: &str,
) -> serde_json::Value {
    let message = anthropic_response_to_chat_message(body);
    let mut out = serde_json::json!({
        "id": body.get("id").and_then(|v| v.as_str()).unwrap_or("chatcmpl_converted"),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": public_model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": anthropic_stop_reason_to_openai(body.get("stop_reason").and_then(|v| v.as_str()))
        }]
    });

    if let Some(usage) = body.get("usage").and_then(anthropic_usage_to_openai_usage) {
        out["usage"] = usage;
    }
    out
}

fn chat_completions_to_messages_response(
    body: &serde_json::Value,
    public_model: &str,
) -> serde_json::Value {
    let message = body
        .pointer("/choices/0/message")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({ "role": "assistant", "content": "" }));
    let content = chat_message_to_anthropic_content(&message);
    let mut out = serde_json::json!({
        "id": body.get("id").and_then(|v| v.as_str()).unwrap_or("msg_converted"),
        "type": "message",
        "role": "assistant",
        "model": public_model,
        "content": content,
        "stop_reason": openai_finish_reason_to_anthropic(body.pointer("/choices/0/finish_reason").and_then(|v| v.as_str()))
    });

    if let Some(usage) = body.get("usage").and_then(openai_usage_to_anthropic_usage) {
        out["usage"] = usage;
    }
    out
}

fn chat_message_to_anthropic_content(message: &serde_json::Value) -> serde_json::Value {
    let mut content = anthropic_content_array_from_openai(
        message.get("content").unwrap_or(&serde_json::Value::Null),
    );
    if let Some(reasoning) = message.get("reasoning") {
        content.push(serde_json::json!({
            "type": "thinking",
            "thinking": message_content_to_output_text(reasoning)
        }));
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(|value| value.as_array()) {
        content.extend(
            tool_calls
                .iter()
                .map(openai_tool_call_to_anthropic_tool_use),
        );
    }
    if content.is_empty() {
        content.push(serde_json::json!({ "type": "text", "text": "" }));
    }
    serde_json::Value::Array(content)
}

fn anthropic_response_to_chat_message(body: &serde_json::Value) -> serde_json::Value {
    let messages = anthropic_message_to_chat_messages(&serde_json::json!({
        "role": "assistant",
        "content": body.get("content").cloned().unwrap_or_else(|| serde_json::json!([]))
    }));
    let mut message = messages
        .into_iter()
        .find(|message| message.get("role").and_then(|value| value.as_str()) == Some("assistant"))
        .unwrap_or_else(|| serde_json::json!({ "role": "assistant", "content": "" }));
    if message.get("tool_calls").is_none() && message.get("reasoning").is_none() {
        if let Some(parts) = message.get("content").and_then(|value| value.as_array()) {
            message["content"] = openai_content_as_chat_value(parts.clone());
        }
    }
    message
}

fn openai_content_as_chat_value(parts: Vec<serde_json::Value>) -> serde_json::Value {
    if parts
        .iter()
        .all(|part| part.get("type").and_then(|value| value.as_str()) == Some("text"))
    {
        return serde_json::Value::String(
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    serde_json::Value::Array(parts)
}

fn copy_field(
    source: &serde_json::Value,
    target: &mut serde_json::Map<String, serde_json::Value>,
    from: &str,
    to: &str,
) {
    if let Some(value) = source.get(from) {
        target.insert(to.to_string(), value.clone());
    }
}

fn messages_metadata_from_source(source: &serde_json::Value) -> Option<serde_json::Value> {
    let user_id = source
        .get("metadata")
        .and_then(|metadata| metadata.get("user_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            source
                .get("conversation_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            source
                .get("user")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })?;

    Some(serde_json::json!({ "user_id": user_id }))
}

fn copy_metadata_user_id_to_user(
    source: &serde_json::Value,
    target: &mut serde_json::Map<String, serde_json::Value>,
) {
    if target.contains_key("user") {
        return;
    }
    let Some(user_id) = source
        .get("metadata")
        .and_then(|metadata| metadata.get("user_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    target.insert(
        "user".to_string(),
        serde_json::Value::String(user_id.to_string()),
    );
}

fn anthropic_usage_to_openai_usage(usage: &serde_json::Value) -> Option<serde_json::Value> {
    let input = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        + usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
        + usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Some(serde_json::json!({
        "prompt_tokens": input,
        "completion_tokens": output,
        "total_tokens": input + output
    }))
}

fn openai_usage_to_anthropic_usage(usage: &serde_json::Value) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "input_tokens": usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        "output_tokens": usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
    }))
}

fn anthropic_stop_reason_to_openai(reason: Option<&str>) -> &'static str {
    match reason {
        Some("max_tokens") => "length",
        Some("tool_use") => "tool_calls",
        _ => "stop",
    }
}

fn openai_finish_reason_to_anthropic(reason: Option<&str>) -> &'static str {
    match reason {
        Some("length") => "max_tokens",
        Some("tool_calls") => "tool_use",
        _ => "end_turn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_request_cross_protocol_conversion_is_rejected() {
        let body = serde_json::json!({
            "model": "public",
            "instructions": "be direct",
            "input": "hello",
            "max_output_tokens": 20
        });

        for target in [ProviderProtocol::Completions, ProviderProtocol::Messages] {
            let err =
                translate_request_body(Operation::Responses, target, &body, "gpt-real", false)
                    .unwrap_err();
            assert!(err.to_string().contains("not supported"));
        }
    }

    #[test]
    fn messages_response_converts_to_chat_completions() {
        let body = serde_json::json!({
            "id": "msg_1",
            "content": [{ "type": "text", "text": "done" }],
            "usage": { "input_tokens": 2, "output_tokens": 3 }
        });

        let out = translate_response_body(
            ProviderProtocol::Messages,
            Operation::Completions,
            body,
            "claude-public",
        )
        .unwrap();

        assert_eq!(out["object"], "chat.completion");
        assert_eq!(out["model"], "claude-public");
        assert_eq!(out["choices"][0]["message"]["content"], "done");
        assert_eq!(out["usage"]["total_tokens"], 5);
    }

    #[test]
    fn chat_request_to_messages_preserves_tools_images_and_tool_results() {
        let body = serde_json::json!({
            "model": "public",
            "messages": [
                { "role": "system", "content": "be precise" },
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": "look" },
                        { "type": "image_url", "image_url": { "url": "https://example.com/a.png" } }
                    ]
                },
                {
                    "role": "assistant",
                    "content": "calling",
                    "reasoning": "need weather",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "weather", "arguments": "{\"city\":\"Paris\"}" }
                    }]
                },
                { "role": "tool", "tool_call_id": "call_1", "content": "sunny" }
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "weather",
                    "description": "Get weather",
                    "parameters": { "type": "object", "properties": { "city": { "type": "string" } } }
                }
            }],
            "tool_choice": { "type": "function", "function": { "name": "weather" } }
        });

        let out = translate_request_body(
            Operation::Completions,
            ProviderProtocol::Messages,
            &body,
            "claude-real",
            false,
        )
        .unwrap();

        assert_eq!(out["model"], "claude-real");
        assert_eq!(out["system"], "be precise");
        assert_eq!(out["messages"][0]["content"][1]["type"], "image");
        assert_eq!(
            out["messages"][0]["content"][1]["source"]["url"],
            "https://example.com/a.png"
        );
        assert_eq!(out["messages"][1]["content"][1]["type"], "thinking");
        assert_eq!(out["messages"][1]["content"][2]["type"], "tool_use");
        assert_eq!(out["messages"][1]["content"][2]["input"]["city"], "Paris");
        assert_eq!(out["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(out["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(out["tool_choice"]["type"], "tool");
        assert_eq!(out["tool_choice"]["name"], "weather");
    }

    #[test]
    fn messages_request_to_chat_preserves_tool_use_and_images() {
        let body = serde_json::json!({
            "model": "public",
            "system": "be precise",
            "metadata": { "user_id": "claude-session-user" },
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": "look" },
                        { "type": "image", "source": { "type": "url", "url": "https://example.com/a.png" } }
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        { "type": "thinking", "thinking": "need weather" },
                        { "type": "tool_use", "id": "call_1", "name": "weather", "input": { "city": "Paris" } }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "tool_result", "tool_use_id": "call_1", "content": "sunny" }
                    ]
                }
            ],
            "tools": [{
                "name": "weather",
                "description": "Get weather",
                "input_schema": { "type": "object" }
            }],
            "tool_choice": { "type": "tool", "name": "weather" }
        });

        let out = translate_request_body(
            Operation::Messages,
            ProviderProtocol::Completions,
            &body,
            "gpt-real",
            false,
        )
        .unwrap();

        assert_eq!(out["model"], "gpt-real");
        assert_eq!(out["messages"][0]["role"], "system");
        assert_eq!(
            out["messages"][1]["content"][1]["image_url"]["url"],
            "https://example.com/a.png"
        );
        assert_eq!(out["messages"][2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(
            out["messages"][2]["tool_calls"][0]["function"]["arguments"],
            "{\"city\":\"Paris\"}"
        );
        assert_eq!(out["messages"][2]["reasoning"][0]["type"], "thinking");
        assert_eq!(out["messages"][3]["role"], "tool");
        assert_eq!(out["tools"][0]["function"]["parameters"]["type"], "object");
        assert_eq!(out["tool_choice"]["function"]["name"], "weather");
        assert_eq!(out["metadata"]["user_id"], "claude-session-user");
        assert_eq!(out["user"], "claude-session-user");
    }

    #[test]
    fn chat_request_to_messages_preserves_sticky_session_metadata() {
        let body = serde_json::json!({
            "model": "public",
            "metadata": {
                "user_id": "user_hash_account_acc_session_claude-session",
                "ignored_by_messages": "must-not-leak"
            },
            "user": "openai-user",
            "messages": [{ "role": "user", "content": "hello" }]
        });

        let out = translate_request_body(
            Operation::Completions,
            ProviderProtocol::Messages,
            &body,
            "claude-real",
            false,
        )
        .unwrap();

        assert_eq!(
            out["metadata"],
            serde_json::json!({ "user_id": "user_hash_account_acc_session_claude-session" })
        );
    }

    #[test]
    fn chat_request_to_messages_uses_stable_session_fallbacks() {
        let body = serde_json::json!({
            "model": "public",
            "user": "openai-user",
            "messages": [{ "role": "user", "content": "hello" }]
        });

        let out = translate_request_body(
            Operation::Completions,
            ProviderProtocol::Messages,
            &body,
            "claude-real",
            false,
        )
        .unwrap();

        assert_eq!(out["metadata"]["user_id"], "openai-user");

        let body = serde_json::json!({
            "model": "public",
            "conversation_id": "conversation-a",
            "user": "openai-user",
            "messages": [{ "role": "user", "content": "hello" }]
        });
        let out = translate_request_body(
            Operation::Completions,
            ProviderProtocol::Messages,
            &body,
            "claude-real",
            false,
        )
        .unwrap();

        assert_eq!(out["metadata"]["user_id"], "conversation-a");
    }

    #[test]
    fn completions_and_messages_requests_cannot_target_responses() {
        let body = serde_json::json!({
            "model": "public",
            "messages": [{ "role": "user", "content": "hello" }]
        });

        for source in [Operation::Completions, Operation::Messages] {
            let err = translate_request_body(
                source,
                ProviderProtocol::Responses,
                &body,
                "response-real",
                false,
            )
            .unwrap_err();
            assert!(err.to_string().contains("not supported"));
        }
    }

    #[test]
    fn responses_cross_protocol_response_conversion_is_rejected() {
        let body = serde_json::json!({
            "id": "resp_1",
            "status": "completed",
            "output": [
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        { "type": "output_text", "text": "done" },
                        { "type": "reasoning_text", "text": "because" }
                    ]
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "weather",
                    "arguments": "{\"city\":\"Paris\"}"
                }
            ]
        });

        for entry in [Operation::Completions, Operation::Messages] {
            let err = translate_response_body(
                ProviderProtocol::Responses,
                entry,
                body.clone(),
                "gpt-public",
            )
            .unwrap_err();
            assert!(err.to_string().contains("not supported"));
        }
    }

    #[test]
    fn chat_response_to_messages_preserves_tool_calls_and_reasoning() {
        let body = serde_json::json!({
            "id": "chatcmpl_1",
            "choices": [{
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": "calling",
                    "reasoning": "need weather",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "weather", "arguments": "{\"city\":\"Paris\"}" }
                    }]
                }
            }]
        });

        let out = translate_response_body(
            ProviderProtocol::Completions,
            Operation::Messages,
            body,
            "claude-public",
        )
        .unwrap();

        assert_eq!(out["content"][0]["text"], "calling");
        assert_eq!(out["content"][1]["type"], "thinking");
        assert_eq!(out["content"][2]["type"], "tool_use");
        assert_eq!(out["content"][2]["input"]["city"], "Paris");
        assert_eq!(out["stop_reason"], "tool_use");
    }

    #[test]
    fn completions_and_messages_are_the_only_advertised_protocol_conversions() {
        for stream in [false, true] {
            assert_eq!(
                candidate_protocols(Operation::Completions, stream),
                vec![ProviderProtocol::Completions, ProviderProtocol::Messages]
            );
            assert_eq!(
                candidate_protocols(Operation::Messages, stream),
                vec![ProviderProtocol::Messages, ProviderProtocol::Completions]
            );
            assert_eq!(
                candidate_protocols(Operation::Responses, stream),
                vec![ProviderProtocol::Responses]
            );
        }
    }
}

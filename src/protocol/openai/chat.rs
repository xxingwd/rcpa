use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::Stream;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use crate::config_service::ModelEndpoint;
use crate::error::{AppError, AppResult};
use crate::middleware::auth;
use crate::protocol::audit;
use crate::protocol::common::{
    Operation, Protocol, ProxyContext, ProxyRequest, SessionAffinityMode, TokenUsage,
};
use crate::protocol::translation;
use crate::retry::policy::RetryPolicy;
use crate::server::AppState;
use crate::stats::cost::CostCalculator;
use crate::store::NewRequestLog;

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    crate::protocol::common::handle_llm_request(
        state,
        &headers,
        body,
        Protocol::Completions,
        Operation::Completions,
        SessionAffinityMode::Enabled,
    )
    .await
}

/// Build an SSE streaming response from a ProviderStreamResponse.
/// When `alias` is Some, rewrite the `model` field in each SSE data event
/// so the user never sees the real provider model name.
fn stream_response(
    stream_resp: crate::provider::ProviderStreamResponse,
    alias: Option<String>,
    audit: StreamAudit,
    transform: Option<SseTransform>,
) -> Response {
    let status =
        StatusCode::from_u16(stream_resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let body = axum::body::Body::from_stream(AuditedSseStream {
        inner: stream_resp.stream,
        alias,
        audit,
        transform,
        pending_output: VecDeque::new(),
        inner_finished: false,
        terminated: false,
    });

    Response::builder()
        .status(status)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .body(body)
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::empty())
                .unwrap()
        })
}

/// Rewrite the `"model"` field in SSE data lines within a byte chunk.
fn rewrite_sse_model(chunk: &[u8], alias: &str) -> Vec<u8> {
    let text = match std::str::from_utf8(chunk) {
        Ok(t) => t,
        Err(_) => return chunk.to_vec(),
    };

    let mut output = String::with_capacity(chunk.len());
    for line in text.split_inclusive('\n') {
        if let Some(json_str) = line.strip_prefix("data: ") {
            let json_str = json_str.trim_end_matches('\n');
            if json_str == "[DONE]" {
                output.push_str(line);
            } else if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert(
                        "model".to_string(),
                        serde_json::Value::String(alias.to_string()),
                    );
                }
                output.push_str("data: ");
                output.push_str(&v.to_string());
                if line.ends_with('\n') {
                    output.push('\n');
                }
            } else {
                output.push_str(line);
            }
        } else {
            output.push_str(line);
        }
    }
    output.into_bytes()
}

fn sse_json_frame(event: Option<&str>, value: serde_json::Value) -> Bytes {
    let mut text = String::new();
    if let Some(event) = event {
        text.push_str("event: ");
        text.push_str(event);
        text.push('\n');
    }
    text.push_str("data: ");
    text.push_str(&value.to_string());
    text.push_str("\n\n");
    Bytes::from(text)
}

fn sse_done_frame() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}

fn trim_ascii_whitespace(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

fn trim_ascii_start(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    value
}

fn parse_sse_event_block(block: &[u8]) -> (Option<String>, Option<Vec<u8>>) {
    let mut event = None;
    let mut data = Vec::new();

    for raw_line in block.split(|byte| *byte == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        if let Some(value) = line.strip_prefix(b"event:") {
            event = std::str::from_utf8(trim_ascii_whitespace(value))
                .ok()
                .map(ToString::to_string);
        } else if let Some(value) = line.strip_prefix(b"data:") {
            if !data.is_empty() {
                data.push(b'\n');
            }
            data.extend_from_slice(trim_ascii_start(value));
        }
    }

    (event, (!data.is_empty()).then_some(data))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn first_sse_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find_bytes(buffer, b"\n\n").map(|pos| (pos, 2));
    let crlf = find_bytes(buffer, b"\r\n\r\n").map(|pos| (pos, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(boundary), None) | (None, Some(boundary)) => Some(boundary),
        (None, None) => None,
    }
}

/// Drain the next complete SSE event from `buffer`.
///
/// Standards-compliant SSE terminates events with a blank line. Some OpenAI-compatible
/// providers instead send one complete JSON `data:` line followed by a single newline.
/// Accept both forms so protocol conversion does not wait for the upstream request timeout.
fn take_next_sse_block(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let leading_newlines = buffer
        .iter()
        .take_while(|byte| matches!(byte, b'\r' | b'\n'))
        .count();
    if leading_newlines > 0 {
        buffer.drain(..leading_newlines);
    }

    if let Some((pos, delimiter_len)) = first_sse_event_boundary(buffer) {
        let block = buffer[..pos].to_vec();
        buffer.drain(..pos + delimiter_len);
        return Some(block);
    }

    let mut search_start = 0;
    while let Some(relative_pos) = buffer[search_start..]
        .iter()
        .position(|byte| *byte == b'\n')
    {
        let pos = search_start + relative_pos;
        let candidate = &buffer[..pos];
        let (_, data) = parse_sse_event_block(candidate);
        let complete = data.as_deref().is_some_and(|data| {
            data == b"[DONE]" || serde_json::from_slice::<serde_json::Value>(data).is_ok()
        });
        if complete {
            let block = candidate.to_vec();
            buffer.drain(..=pos);
            return Some(block);
        }
        search_start = pos + 1;
    }

    None
}

fn openai_finish_reason_to_anthropic_stream(reason: Option<&str>) -> &'static str {
    match reason {
        Some("length") => "max_tokens",
        Some("tool_calls") => "tool_use",
        _ => "end_turn",
    }
}

fn anthropic_stop_reason_to_openai_stream(reason: Option<&str>) -> &'static str {
    match reason {
        Some("max_tokens") => "length",
        Some("tool_use") => "tool_calls",
        _ => "stop",
    }
}

fn openai_usage_to_anthropic_stream(usage: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "input_tokens": usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        "output_tokens": usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
    })
}

#[derive(Clone)]
enum SseTransform {
    AnthropicToOpenAi(AnthropicToOpenAiStream),
    OpenAiToAnthropic(OpenAiToAnthropicStream),
}

impl SseTransform {
    fn for_route(
        upstream_protocol: crate::config::ProviderProtocol,
        entry_operation: Operation,
        public_model: &str,
    ) -> Option<Self> {
        match (upstream_protocol, entry_operation) {
            (crate::config::ProviderProtocol::Messages, Operation::Completions) => Some(
                Self::AnthropicToOpenAi(AnthropicToOpenAiStream::new(public_model)),
            ),
            (crate::config::ProviderProtocol::Completions, Operation::Messages) => Some(
                Self::OpenAiToAnthropic(OpenAiToAnthropicStream::new(public_model)),
            ),
            _ => None,
        }
    }

    fn push_chunk(&mut self, chunk: &[u8]) -> Vec<Bytes> {
        match self {
            SseTransform::AnthropicToOpenAi(state) => state.push_chunk(chunk),
            SseTransform::OpenAiToAnthropic(state) => state.push_chunk(chunk),
        }
    }

    fn finish(&mut self) -> Vec<Bytes> {
        match self {
            SseTransform::AnthropicToOpenAi(state) => state.finish(),
            SseTransform::OpenAiToAnthropic(state) => state.finish(),
        }
    }

    fn is_finished(&self) -> bool {
        match self {
            SseTransform::AnthropicToOpenAi(state) => state.done_emitted,
            SseTransform::OpenAiToAnthropic(state) => state.emitted_final,
        }
    }
}

#[derive(Clone)]
struct AnthropicToOpenAiStream {
    public_model: String,
    buffer: Vec<u8>,
    stream_id: String,
    emitted_role: bool,
    done_emitted: bool,
    next_tool_call_index: usize,
    tool_index_map: HashMap<usize, usize>,
}

impl AnthropicToOpenAiStream {
    fn new(public_model: &str) -> Self {
        Self {
            public_model: public_model.to_string(),
            buffer: Vec::new(),
            stream_id: "chatcmpl_converted".to_string(),
            emitted_role: false,
            done_emitted: false,
            next_tool_call_index: 0,
            tool_index_map: HashMap::new(),
        }
    }

    fn chunk_frame(
        &self,
        delta: serde_json::Value,
        finish_reason: Option<&str>,
        usage: Option<serde_json::Value>,
    ) -> Bytes {
        let mut payload = serde_json::json!({
            "id": self.stream_id,
            "object": "chat.completion.chunk",
            "created": chrono::Utc::now().timestamp(),
            "model": self.public_model,
            "choices": [{
                "index": 0,
                "delta": delta,
                "finish_reason": finish_reason
            }]
        });

        if let Some(usage) = usage {
            payload["usage"] = usage;
        }

        sse_json_frame(None, payload)
    }

    fn role_frame(&mut self) -> Option<Bytes> {
        if self.emitted_role {
            return None;
        }
        self.emitted_role = true;
        Some(self.chunk_frame(serde_json::json!({ "role": "assistant" }), None, None))
    }

    fn push_chunk(&mut self, chunk: &[u8]) -> Vec<Bytes> {
        self.buffer.extend_from_slice(chunk);

        let mut outputs = Vec::new();
        while let Some(block) = take_next_sse_block(&mut self.buffer) {
            outputs.extend(self.process_block(&block));
        }
        outputs
    }

    fn process_block(&mut self, block: &[u8]) -> Vec<Bytes> {
        let (event_name, data) = parse_sse_event_block(block);
        let Some(data) = data else {
            return Vec::new();
        };
        if data == b"[DONE]" {
            return self.emit_done();
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) else {
            return Vec::new();
        };

        let event_name = event_name
            .as_deref()
            .or_else(|| value.get("type").and_then(|v| v.as_str()))
            .unwrap_or("");

        match event_name {
            "message_start" => {
                if let Some(id) = value
                    .get("message")
                    .and_then(|message| message.get("id"))
                    .and_then(|v| v.as_str())
                {
                    self.stream_id = id.to_string();
                }
                self.role_frame().into_iter().collect()
            }
            "content_block_start" => {
                let mut outputs = Vec::new();
                if let Some(role) = self.role_frame() {
                    outputs.push(role);
                }
                let Some(content_block) = value.get("content_block") else {
                    return outputs;
                };
                if content_block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                    return outputs;
                }
                let source_index =
                    value.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let tool_call_index =
                    *self.tool_index_map.entry(source_index).or_insert_with(|| {
                        let next = self.next_tool_call_index;
                        self.next_tool_call_index += 1;
                        next
                    });
                outputs.push(self.chunk_frame(
                    serde_json::json!({
                        "tool_calls": [{
                            "index": tool_call_index,
                            "id": content_block.get("id").and_then(|v| v.as_str()).unwrap_or("tool_call"),
                            "type": "function",
                            "function": {
                                "name": content_block.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                "arguments": ""
                            }
                        }]
                    }),
                    None,
                    None,
                ));
                outputs
            }
            "content_block_delta" => {
                let mut outputs = Vec::new();
                if let Some(role) = self.role_frame() {
                    outputs.push(role);
                }
                let Some(delta) = value.get("delta") else {
                    return outputs;
                };
                match delta.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                    "text_delta" => {
                        let text = delta.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        if !text.is_empty() {
                            outputs.push(self.chunk_frame(
                                serde_json::json!({ "content": text }),
                                None,
                                None,
                            ));
                        }
                    }
                    "input_json_delta" => {
                        let source_index =
                            value.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let tool_call_index =
                            *self.tool_index_map.entry(source_index).or_insert_with(|| {
                                let next = self.next_tool_call_index;
                                self.next_tool_call_index += 1;
                                next
                            });
                        let partial_json = delta
                            .get("partial_json")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !partial_json.is_empty() {
                            outputs.push(self.chunk_frame(
                                serde_json::json!({
                                    "tool_calls": [{
                                        "index": tool_call_index,
                                        "type": "function",
                                        "function": {
                                            "arguments": partial_json
                                        }
                                    }]
                                }),
                                None,
                                None,
                            ));
                        }
                    }
                    "thinking_delta" => {
                        let text = delta.get("thinking").and_then(|v| v.as_str()).unwrap_or("");
                        if !text.is_empty() {
                            outputs.push(self.chunk_frame(
                                serde_json::json!({ "reasoning": text }),
                                None,
                                None,
                            ));
                        }
                    }
                    _ => {}
                }
                outputs
            }
            "message_delta" => {
                let finish_reason = anthropic_stop_reason_to_openai_stream(
                    value
                        .get("delta")
                        .and_then(|delta| delta.get("stop_reason"))
                        .and_then(|v| v.as_str()),
                );
                let usage = value
                    .get("usage")
                    .and_then(TokenUsage::from_anthropic_usage)
                    .map(|usage| {
                        serde_json::json!({
                            "prompt_tokens": usage.prompt_tokens,
                            "completion_tokens": usage.completion_tokens,
                            "total_tokens": usage.total_tokens
                        })
                    });
                vec![self.chunk_frame(serde_json::json!({}), Some(finish_reason), usage)]
            }
            "message_stop" => self.emit_done(),
            _ => Vec::new(),
        }
    }

    fn emit_done(&mut self) -> Vec<Bytes> {
        if self.done_emitted {
            return Vec::new();
        }
        self.done_emitted = true;
        vec![sse_done_frame()]
    }

    fn finish(&mut self) -> Vec<Bytes> {
        if !trim_ascii_whitespace(&self.buffer).is_empty() {
            let block = std::mem::take(&mut self.buffer);
            let mut outputs = self.process_block(trim_ascii_whitespace(&block));
            outputs.extend(self.emit_done());
            return outputs;
        }
        self.emit_done()
    }
}

#[derive(Debug, Clone)]
struct OpenAiToolBlock {
    content_block_index: usize,
    id: String,
    name: String,
    started: bool,
    closed: bool,
}

#[derive(Clone)]
struct OpenAiToAnthropicStream {
    public_model: String,
    buffer: Vec<u8>,
    stream_id: String,
    started: bool,
    emitted_final: bool,
    text_block_open: bool,
    text_block_closed: bool,
    text_block_index: usize,
    next_content_block_index: usize,
    tool_blocks: HashMap<usize, OpenAiToolBlock>,
    pending_stop_reason: Option<String>,
    latest_usage: Option<serde_json::Value>,
}

impl OpenAiToAnthropicStream {
    fn new(public_model: &str) -> Self {
        Self {
            public_model: public_model.to_string(),
            buffer: Vec::new(),
            stream_id: "msg_converted".to_string(),
            started: false,
            emitted_final: false,
            text_block_open: false,
            text_block_closed: false,
            text_block_index: 0,
            next_content_block_index: 1,
            tool_blocks: HashMap::new(),
            pending_stop_reason: None,
            latest_usage: None,
        }
    }

    fn push_chunk(&mut self, chunk: &[u8]) -> Vec<Bytes> {
        self.buffer.extend_from_slice(chunk);

        let mut outputs = Vec::new();
        while let Some(block) = take_next_sse_block(&mut self.buffer) {
            outputs.extend(self.process_block(&block));
        }
        outputs
    }

    fn process_block(&mut self, block: &[u8]) -> Vec<Bytes> {
        let (_event_name, data) = parse_sse_event_block(block);
        let Some(data) = data else {
            return Vec::new();
        };
        if data == b"[DONE]" {
            return self.finalize();
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) else {
            return Vec::new();
        };
        self.process_value(&value)
    }

    fn ensure_message_start(&mut self) -> Option<Bytes> {
        if self.started {
            return None;
        }
        self.started = true;
        Some(sse_json_frame(
            Some("message_start"),
            serde_json::json!({
                "type": "message_start",
                "message": {
                    "id": self.stream_id,
                    "type": "message",
                    "role": "assistant",
                    "model": self.public_model,
                    "content": [],
                    "stop_reason": serde_json::Value::Null,
                    "stop_sequence": serde_json::Value::Null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0
                    }
                }
            }),
        ))
    }

    fn ensure_text_block_start(&mut self) -> Option<Bytes> {
        if self.text_block_open || self.text_block_closed {
            return None;
        }
        self.text_block_open = true;
        Some(sse_json_frame(
            Some("content_block_start"),
            serde_json::json!({
                "type": "content_block_start",
                "index": self.text_block_index,
                "content_block": {
                    "type": "text",
                    "text": ""
                }
            }),
        ))
    }

    fn close_text_block(&mut self) -> Option<Bytes> {
        if !self.text_block_open {
            return None;
        }
        self.text_block_open = false;
        self.text_block_closed = true;
        Some(sse_json_frame(
            Some("content_block_stop"),
            serde_json::json!({
                "type": "content_block_stop",
                "index": self.text_block_index
            }),
        ))
    }

    fn tool_block_mut(&mut self, index: usize) -> &mut OpenAiToolBlock {
        self.tool_blocks.entry(index).or_insert_with(|| {
            let content_block_index = self.next_content_block_index;
            self.next_content_block_index += 1;
            OpenAiToolBlock {
                content_block_index,
                id: format!("toolu_{}", index),
                name: String::new(),
                started: false,
                closed: false,
            }
        })
    }

    fn process_value(&mut self, value: &serde_json::Value) -> Vec<Bytes> {
        if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
            self.stream_id = id.to_string();
        }

        let mut outputs = Vec::new();
        if let Some(frame) = self.ensure_message_start() {
            outputs.push(frame);
        }

        if let Some(usage) = value.get("usage") {
            self.latest_usage = Some(openai_usage_to_anthropic_stream(usage));
        }

        let choice = value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .and_then(|choices| choices.first());
        let Some(choice) = choice else {
            return outputs;
        };

        if let Some(delta) = choice.get("delta") {
            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                if !content.is_empty() {
                    if let Some(frame) = self.ensure_text_block_start() {
                        outputs.push(frame);
                    }
                    outputs.push(sse_json_frame(
                        Some("content_block_delta"),
                        serde_json::json!({
                            "type": "content_block_delta",
                            "index": self.text_block_index,
                            "delta": {
                                "type": "text_delta",
                                "text": content
                            }
                        }),
                    ));
                }
            }

            if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                if let Some(frame) = self.close_text_block() {
                    outputs.push(frame);
                }

                for tool_call in tool_calls {
                    let stream_index =
                        tool_call.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let state = self.tool_block_mut(stream_index);
                    if let Some(id) = tool_call.get("id").and_then(|v| v.as_str()) {
                        state.id = id.to_string();
                    }
                    if let Some(name) = tool_call
                        .get("function")
                        .and_then(|function| function.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        state.name = name.to_string();
                    }
                    if !state.started {
                        state.started = true;
                        outputs.push(sse_json_frame(
                            Some("content_block_start"),
                            serde_json::json!({
                                "type": "content_block_start",
                                "index": state.content_block_index,
                                "content_block": {
                                    "type": "tool_use",
                                    "id": state.id,
                                    "name": state.name,
                                    "input": {}
                                }
                            }),
                        ));
                    }
                    let arguments = tool_call
                        .get("function")
                        .and_then(|function| function.get("arguments"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !arguments.is_empty() {
                        outputs.push(sse_json_frame(
                            Some("content_block_delta"),
                            serde_json::json!({
                                "type": "content_block_delta",
                                "index": state.content_block_index,
                                "delta": {
                                    "type": "input_json_delta",
                                    "partial_json": arguments
                                }
                            }),
                        ));
                    }
                }
            }
        }

        if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
            self.pending_stop_reason = Some(finish_reason.to_string());
        }

        outputs
    }

    fn finalize(&mut self) -> Vec<Bytes> {
        if self.emitted_final {
            return Vec::new();
        }
        self.emitted_final = true;

        let mut outputs = Vec::new();
        if !self.started {
            if let Some(frame) = self.ensure_message_start() {
                outputs.push(frame);
            }
        }
        if let Some(frame) = self.close_text_block() {
            outputs.push(frame);
        }
        for tool in self.tool_blocks.values_mut() {
            if tool.started && !tool.closed {
                tool.closed = true;
                outputs.push(sse_json_frame(
                    Some("content_block_stop"),
                    serde_json::json!({
                        "type": "content_block_stop",
                        "index": tool.content_block_index
                    }),
                ));
            }
        }

        let stop_reason =
            openai_finish_reason_to_anthropic_stream(self.pending_stop_reason.as_deref());
        outputs.push(sse_json_frame(
            Some("message_delta"),
            serde_json::json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": stop_reason,
                    "stop_sequence": serde_json::Value::Null
                },
                "usage": self.latest_usage.clone().unwrap_or_else(|| serde_json::json!({
                    "input_tokens": 0,
                    "output_tokens": 0
                }))
            }),
        ));
        outputs.push(sse_json_frame(
            Some("message_stop"),
            serde_json::json!({
                "type": "message_stop"
            }),
        ));
        outputs
    }

    fn finish(&mut self) -> Vec<Bytes> {
        if !trim_ascii_whitespace(&self.buffer).is_empty() {
            let block = std::mem::take(&mut self.buffer);
            let mut outputs = self.process_block(trim_ascii_whitespace(&block));
            outputs.extend(self.finalize());
            return outputs;
        }
        self.finalize()
    }
}

fn calculate_cost_cents(
    snapshot: &crate::config_service::ConfigSnapshot,
    resolved_model: &str,
    provider_name: &str,
    tokens: Option<&TokenUsage>,
) -> u64 {
    let Some(tokens) = tokens else {
        return 0;
    };

    let provider_pricing = snapshot.provider_model_pricing(provider_name, resolved_model);

    if let Some(rule) = provider_pricing {
        let input_cost = (tokens.prompt_tokens as f64 / 1000.0) * rule.input_per_1k;
        let output_cost = (tokens.completion_tokens as f64 / 1000.0) * rule.output_per_1k;
        let cents = (input_cost + output_cost) * 100.0;
        if cents > 0.0 && cents < 1.0 {
            1
        } else {
            cents.round() as u64
        }
    } else {
        CostCalculator::from_config(&snapshot.config.cost).calculate(
            resolved_model,
            tokens.prompt_tokens,
            tokens.completion_tokens,
        )
    }
}

fn token_log_fields(tokens: Option<&TokenUsage>) -> (i64, i64, i64, i64, i64) {
    tokens
        .map(|t| {
            (
                t.prompt_tokens as i64,
                t.completion_tokens as i64,
                t.total_tokens as i64,
                t.cached_tokens as i64,
                t.cache_write_tokens as i64,
            )
        })
        .unwrap_or((0, 0, 0, 0, 0))
}

fn next_retry_backoff_ms(
    retry_policy: &RetryPolicy,
    attempt: u32,
    max_attempts: u32,
) -> Option<u64> {
    (attempt + 1 < max_attempts).then(|| retry_policy.backoff_for(attempt).as_millis() as u64)
}

fn record_provider_failure(state: &AppState, provider_name: &str) {
    if state.router.record_provider_failure(provider_name) {
        state.sticky_sessions.invalidate_provider(provider_name);
    }
}

fn record_sticky_session_success(state: &AppState, ctx: &ProxyContext, provider_name: &str) {
    if !ctx.config_snapshot.config.routing.sticky.enabled {
        return;
    }
    let Some(session_affinity) = &ctx.session_affinity else {
        return;
    };
    let ttl = std::time::Duration::from_secs(ctx.config_snapshot.config.routing.sticky.ttl_secs);
    state.sticky_sessions.set_with_ttl(
        session_affinity.key.clone().into_string(),
        provider_name.to_string(),
        ttl,
    );
}

struct ErrorLogInput<'a> {
    request_id: &'a str,
    api_key_id: &'a str,
    session_hash: Option<&'a str>,
    provider_name: &'a str,
    protocol: &'a str,
    model: &'a str,
    operation: &'a str,
    status_code: i64,
    success: bool,
    metadata_json: &'a str,
    latency_ms: i64,
    request_body: Option<&'a [u8]>,
}

/// Build a NewRequestLog for an error response (zero tokens, zero cost).
fn error_log_entry(input: ErrorLogInput<'_>) -> NewRequestLog<'_> {
    NewRequestLog {
        request_id: input.request_id,
        api_key_id: input.api_key_id,
        session_hash: input.session_hash,
        provider_name: input.provider_name,
        protocol: input.protocol,
        model: input.model,
        operation: input.operation,
        status_code: input.status_code,
        success: input.success,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        cached_tokens: 0,
        cache_write_tokens: 0,
        cost_cents: 0,
        latency_ms: input.latency_ms,
        first_byte_latency_ms: input.latency_ms,
        metadata_json: input.metadata_json,
        request_body: input.request_body,
        response_body: None,
    }
}

struct LogMetadataInput<'a> {
    ctx: &'a ProxyContext,
    provider_name: &'a str,
    protocol: &'a str,
    provider_model: &'a str,
    upstream_base_url: Option<&'a str>,
    upstream_operation: Option<&'a str>,
    status_code: i64,
    error_code: Option<&'a str>,
    error_message: Option<&'a str>,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    sticky_hit: Option<bool>,
    selected_provider_reason: Option<&'a str>,
    attempts: &'a [RetryAttemptLog],
    upstream_path: Option<&'a str>,
}

struct LogSessionMetadata<'a> {
    id: &'a str,
    source: Option<&'a str>,
    hash: Option<&'a str>,
    affinity_key: Option<&'a str>,
}

struct LogMetadata<'a> {
    request_id: &'a str,
    request_headers: &'a BTreeMap<String, String>,
    request_protocol: &'a str,
    request_operation: &'a str,
    request_stream: bool,
    session: Option<LogSessionMetadata<'a>>,
    requested_model: &'a str,
    resolved_model: &'a str,
    provider_model: &'a str,
    sticky_enabled: bool,
    provider_name: &'a str,
    protocol: &'a str,
    status_code: i64,
    error_code: Option<&'a str>,
    error_message: Option<&'a str>,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    sticky_hit: Option<bool>,
    selected_provider_reason: Option<&'a str>,
    attempts: &'a [RetryAttemptLog],
    upstream_base_url: Option<&'a str>,
    upstream_operation: Option<&'a str>,
    upstream_path: Option<&'a str>,
    currency: &'a str,
}

#[derive(Debug, Clone)]
struct RetryAttemptLog {
    attempt: u32,
    provider_name: String,
    protocol: String,
    provider_model: String,
    status_code: i64,
    error_code: Option<String>,
    error_message: Option<String>,
    retryable: bool,
    backoff_ms_before_next: Option<u64>,
    selected_via: &'static str,
    sticky_hit: bool,
    provider_healthy_before_attempt: bool,
}

impl RetryAttemptLog {
    fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "attempt": self.attempt,
            "provider_name": self.provider_name,
            "protocol": self.protocol,
            "provider_model": self.provider_model,
            "status_code": self.status_code,
            "error_code": self.error_code,
            "error_message": self.error_message,
            "retryable": self.retryable,
            "backoff_ms_before_next": self.backoff_ms_before_next,
            "selected_via": self.selected_via,
            "sticky_hit": self.sticky_hit,
            "provider_healthy_before_attempt": self.provider_healthy_before_attempt
        })
    }
}

fn log_metadata_json(input: LogMetadataInput<'_>) -> String {
    let request_id = input.ctx.request_id.to_string();
    let request_protocol = input.ctx.protocol.to_string();
    let request_operation = input.ctx.operation.to_string();
    let session = input
        .ctx
        .session_affinity
        .as_ref()
        .map(|affinity| LogSessionMetadata {
            id: affinity.id.as_str(),
            source: Some(affinity.source.as_str()),
            hash: Some(affinity.hash.as_str()),
            affinity_key: Some(affinity.key.as_str()),
        });

    build_log_metadata_json(LogMetadata {
        request_id: &request_id,
        request_headers: &input.ctx.request_headers,
        request_protocol: &request_protocol,
        request_operation: &request_operation,
        request_stream: input.ctx.stream,
        session,
        requested_model: &input.ctx.model,
        resolved_model: &input.ctx.resolved_model,
        provider_model: input.provider_model,
        sticky_enabled: input.ctx.config_snapshot.config.routing.sticky.enabled,
        provider_name: input.provider_name,
        protocol: input.protocol,
        status_code: input.status_code,
        error_code: input.error_code,
        error_message: input.error_message,
        attempt_count: input.attempt_count,
        retry_count: input.retry_count,
        total_backoff_ms: input.total_backoff_ms,
        sticky_hit: input.sticky_hit,
        selected_provider_reason: input.selected_provider_reason,
        attempts: input.attempts,
        upstream_base_url: input.upstream_base_url,
        upstream_operation: input.upstream_operation,
        upstream_path: input.upstream_path,
        currency: &input.ctx.config_snapshot.config.cost.currency,
    })
}

fn build_log_metadata_json(input: LogMetadata<'_>) -> String {
    let session = input.session.map(|session| {
        serde_json::json!({
            "id": session.id,
            "source": session.source,
            "hash": session.hash,
            "affinity_key": session.affinity_key
        })
    });
    let error = input.error_code.or(input.error_message).map(|_| {
        serde_json::json!({
            "code": input.error_code,
            "message": input.error_message,
            "retryable": false
        })
    });
    let attempts: Vec<serde_json::Value> = input
        .attempts
        .iter()
        .map(RetryAttemptLog::as_json)
        .collect();

    serde_json::json!({
        "request": {
            "id": input.request_id,
            "protocol": input.request_protocol,
            "operation": input.request_operation,
            "model": input.requested_model,
            "resolved_model": input.resolved_model,
            "stream": input.request_stream,
            "headers": input.request_headers
        },
        "session": session,
        "models": {
            "requested": input.requested_model,
            "resolved": input.resolved_model,
            "provider": input.provider_model
        },
        "routing": {
            "sticky_enabled": input.sticky_enabled,
            "sticky_hit": input.sticky_hit,
            "selected_provider_reason": input.selected_provider_reason,
            "selected_provider": input.provider_name,
            "target_protocol": input.protocol,
            "target_operation": input.upstream_operation,
            "target_model": input.provider_model,
            "upstream_base_url": input.upstream_base_url,
            "candidates": null
        },
        "retry": {
            "attempt_count": input.attempt_count,
            "retry_count": input.retry_count,
            "total_backoff_ms": input.total_backoff_ms,
            "attempts": attempts
        },
        "upstream": {
            "path": input.upstream_path,
            "base_url": input.upstream_base_url,
            "operation": input.upstream_operation,
            "protocol": input.protocol,
            "model": input.provider_model,
            "request_id": null,
            "status_code": input.status_code
        },
        "pricing": {
            "currency": input.currency
        },
        "error": error,
        "body": {
            "request_body": "upstream_request_body",
            "response_body": "upstream_response_body"
        },
        "provider": {
            "name": input.provider_name,
            "protocol": input.protocol
        }
    })
    .to_string()
}

#[derive(Clone)]
struct PreparedAttempt {
    provider_name: String,
    provider: Arc<dyn crate::provider::ProviderAdapter>,
    protocol: String,
    upstream_protocol: crate::config::ProviderProtocol,
    upstream_base_url: String,
    operation: String,
    actual_model: String,
    modified_req: ProxyRequest,
    request_body_bytes: Option<Vec<u8>>,
    selected_via: &'static str,
    sticky_hit: bool,
    provider_healthy_before_attempt: bool,
    stream_transform: Option<SseTransform>,
}

enum AttemptExecution {
    Response(Response),
    Retry(AppError),
    RetryNextProvider(AppError),
    Fail(AppError),
}

struct RetryAttemptInput<'a> {
    attempt: u32,
    provider_name: &'a str,
    protocol: &'a str,
    provider_model: &'a str,
    status_code: i64,
    error_code: Option<String>,
    error_message: Option<String>,
    retryable: bool,
    backoff_ms_before_next: Option<u64>,
    selected_via: &'static str,
    sticky_hit: bool,
    provider_healthy_before_attempt: bool,
}

fn build_retry_attempt_log(input: RetryAttemptInput<'_>) -> RetryAttemptLog {
    RetryAttemptLog {
        attempt: input.attempt,
        provider_name: input.provider_name.to_string(),
        protocol: input.protocol.to_string(),
        provider_model: input.provider_model.to_string(),
        status_code: input.status_code,
        error_code: input.error_code,
        error_message: input.error_message,
        retryable: input.retryable,
        backoff_ms_before_next: input.backoff_ms_before_next,
        selected_via: input.selected_via,
        sticky_hit: input.sticky_hit,
        provider_healthy_before_attempt: input.provider_healthy_before_attempt,
    }
}

struct RequestOutcome<'a> {
    state: &'a AppState,
    provider_name: &'a str,
    actual_model: &'a str,
    latency: std::time::Duration,
    tokens: Option<&'a TokenUsage>,
    cost_cents: u64,
    api_key_id: &'a str,
    success: bool,
}

struct PersistErrorRequestLogInput<'a> {
    state: &'a AppState,
    ctx: &'a ProxyContext,
    api_key_id: &'a str,
    provider_name: &'a str,
    protocol: &'a str,
    provider_model: &'a str,
    error: &'a AppError,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    sticky_hit: Option<bool>,
    selected_provider_reason: Option<&'a str>,
    attempts: &'a [RetryAttemptLog],
    upstream_base_url: Option<&'a str>,
    upstream_operation: Option<&'a str>,
    request_body: Option<&'a [u8]>,
    latency_ms: i64,
}

struct AttemptContext<'a> {
    state: &'a Arc<AppState>,
    ctx: &'a ProxyContext,
    retry_policy: &'a RetryPolicy,
    api_key_id: &'a str,
    start: Instant,
    max_attempts: u32,
    total_backoff_ms: u64,
}

struct ProxyFailureInput<'a> {
    state: &'a AppState,
    ctx: &'a ProxyContext,
    api_key_id: &'a str,
    start: Instant,
    max_attempts: u32,
    total_backoff_ms: u64,
    last_error: Option<AppError>,
    last_provider_name: Option<&'a str>,
    last_protocol: Option<&'a str>,
    last_upstream_base_url: Option<&'a str>,
    last_upstream_operation: Option<&'a str>,
    last_actual_model: Option<&'a str>,
    retry_attempts: &'a [RetryAttemptLog],
    request_body: Option<&'a [u8]>,
}

struct StreamAuditInit<'a> {
    state: Arc<AppState>,
    ctx: &'a ProxyContext,
    provider_name: &'a str,
    protocol: &'a str,
    upstream_base_url: &'a str,
    upstream_operation: &'a str,
    actual_model: &'a str,
    status_code: u16,
    start: Instant,
    first_byte_latency_ms: i64,
    request_body: Option<Vec<u8>>,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    routing_selected_reason: &'static str,
    routing_sticky_hit: bool,
    attempts: Vec<RetryAttemptLog>,
}

struct StreamAuditCompletion {
    success: bool,
    latency: std::time::Duration,
    latency_ms: i64,
    cost_cents: u64,
    error_code: Option<String>,
    error_message: Option<String>,
}

fn should_retry_provider_error(error: &AppError, retry_policy: &RetryPolicy) -> bool {
    match error {
        AppError::ProviderError {
            status_code: Some(status),
            ..
        } => retry_policy.should_retry(status.as_u16()),
        AppError::ProviderError {
            status_code: None, ..
        } => true,
        AppError::ProviderTimeout(_) | AppError::ServiceUnavailable(_) => true,
        _ => false,
    }
}

fn record_completed_request_outcome(input: RequestOutcome<'_>) {
    if input.success {
        input
            .state
            .router
            .record_provider_success(input.provider_name);
        input.state.stats.record_success(
            input.actual_model,
            input.provider_name,
            input.latency,
            input.tokens,
        );
        input.state.stats.record_cost(input.cost_cents);
        input
            .state
            .stats
            .record_key_usage(input.api_key_id, input.actual_model, input.cost_cents);
    } else {
        record_provider_failure(input.state, input.provider_name);
        input
            .state
            .stats
            .record_error(input.actual_model, input.provider_name);
    }
}

fn prepare_attempt(
    state: &AppState,
    req: &ProxyRequest,
    ctx: &ProxyContext,
    attempted_providers: &mut HashSet<String>,
) -> Result<PreparedAttempt, AppError> {
    let snapshot = ctx.config_snapshot.clone();

    let candidate_protocols = translation::candidate_protocols(req.operation, req.stream);
    let all_endpoints: Vec<ModelEndpoint> = snapshot
        .endpoints_for_alias(&req.model, Some(&ctx.auth_key))
        .into_iter()
        .filter(|ep| candidate_protocols.contains(&ep.protocol))
        .collect();

    let unique_provider_count = all_endpoints
        .iter()
        .map(|ep| ep.provider_name.as_str())
        .collect::<HashSet<_>>()
        .len();

    if unique_provider_count > 1
        && !all_endpoints.is_empty()
        && all_endpoints
            .iter()
            .all(|ep| attempted_providers.contains(&ep.provider_name))
    {
        attempted_providers.clear();
    }

    let session_key = ctx
        .session_affinity
        .as_ref()
        .map(|affinity| affinity.key.as_str())
        .unwrap_or("");
    let route_decision = state.router.route_decision_for_protocols_with_exclusions(
        &req.model,
        &req.operation,
        &candidate_protocols,
        state,
        &snapshot,
        session_key,
        Some(&ctx.auth_key),
        Some(attempted_providers),
    )?;

    let provider_name = route_decision.endpoint.provider_name.clone();
    let upstream_base_url = route_decision.endpoint.base_url.clone();
    attempted_providers.insert(provider_name.clone());
    let provider_healthy_before_attempt = state.router.is_provider_healthy(&provider_name);
    let upstream_protocol = route_decision.endpoint.protocol;
    let provider = snapshot
        .registry
        .get(&provider_name, upstream_protocol)
        .ok_or_else(|| AppError::NoProviderAvailable(ctx.model.clone()))?;
    let protocol = upstream_protocol.to_string();
    let actual_model = route_decision.endpoint.name.clone();
    let target_operation = translation::operation_for_target(req.operation, upstream_protocol)
        .ok_or_else(|| {
            AppError::ProtocolError(format!(
                "Protocol conversion from '{}' to '{}' is not supported",
                req.operation.provider_protocol(),
                upstream_protocol
            ))
        })?;
    let translated_body = translation::translate_request_body(
        req.operation,
        upstream_protocol,
        &req.body,
        &actual_model,
        req.stream,
    )?;

    let mut modified_req = req.clone();
    modified_req.operation = target_operation;
    modified_req.protocol = match upstream_protocol {
        crate::config::ProviderProtocol::Completions
        | crate::config::ProviderProtocol::Embeddings => Protocol::Completions,
        crate::config::ProviderProtocol::Responses => Protocol::Responses,
        crate::config::ProviderProtocol::Messages => Protocol::Messages,
    };
    modified_req.model = actual_model.clone();
    modified_req.body = translated_body;
    let outbound_request_body = provider.serialize_request_body(&modified_req);
    let request_body_bytes = serde_json::to_vec(&outbound_request_body).ok();
    let stream_transform = SseTransform::for_route(upstream_protocol, req.operation, &ctx.model);

    Ok(PreparedAttempt {
        provider_name,
        provider,
        protocol,
        upstream_protocol,
        upstream_base_url,
        operation: target_operation.to_string(),
        actual_model,
        modified_req,
        request_body_bytes,
        selected_via: route_decision.selection_reason.as_str(),
        sticky_hit: route_decision.sticky_hit,
        provider_healthy_before_attempt,
        stream_transform,
    })
}

async fn persist_error_request_log(input: PersistErrorRequestLogInput<'_>) {
    let error_code = input.error.error_code();
    let error_message = input.error.to_string();
    let request_id = input.ctx.request_id.to_string();
    let operation = input.ctx.operation.to_string();
    let metadata = log_metadata_json(LogMetadataInput {
        ctx: input.ctx,
        provider_name: input.provider_name,
        protocol: input.protocol,
        provider_model: input.provider_model,
        upstream_base_url: input.upstream_base_url,
        upstream_operation: input.upstream_operation,
        status_code: input.error.status_code().as_u16() as i64,
        error_code: Some(error_code.as_ref()),
        error_message: Some(&error_message),
        attempt_count: input.attempt_count,
        retry_count: input.retry_count,
        total_backoff_ms: input.total_backoff_ms,
        sticky_hit: input.sticky_hit,
        selected_provider_reason: input.selected_provider_reason,
        attempts: input.attempts,
        upstream_path: None,
    });

    // Best-effort audit logging: request handling should not fail if persistence is unavailable.
    if let Err(err) = audit::record_llm_request(
        input.state,
        error_log_entry(ErrorLogInput {
            request_id: &request_id,
            api_key_id: input.api_key_id,
            session_hash: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.hash.as_str()),
            provider_name: input.provider_name,
            protocol: input.protocol,
            model: input.provider_model,
            operation: &operation,
            status_code: input.error.status_code().as_u16() as i64,
            success: false,
            metadata_json: &metadata,
            latency_ms: input.latency_ms,
            request_body: input.request_body,
        }),
    )
    .await
    {
        tracing::warn!(
            request_id = %input.ctx.request_id,
            error = %err,
            "Failed to persist error request audit"
        );
    }
}

async fn execute_stream_attempt(
    input: AttemptContext<'_>,
    mut prepared: PreparedAttempt,
    attempt: u32,
    provider_attempt: u32,
    retry_attempts: &mut Vec<RetryAttemptLog>,
) -> AttemptExecution {
    let attempt_number = attempt + 1;

    match prepared
        .provider
        .proxy_stream(prepared.modified_req.clone())
        .await
    {
        Ok(stream_resp) => {
            let first_byte_latency_ms = stream_resp.first_byte_latency_ms as i64;
            let mut attempts = std::mem::take(retry_attempts);
            attempts.push(build_retry_attempt_log(RetryAttemptInput {
                attempt: attempt_number,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: stream_resp.status as i64,
                error_code: None,
                error_message: None,
                retryable: false,
                backoff_ms_before_next: None,
                selected_via: prepared.selected_via,
                sticky_hit: prepared.sticky_hit,
                provider_healthy_before_attempt: prepared.provider_healthy_before_attempt,
            }));
            let audit = StreamAudit::new(StreamAuditInit {
                state: input.state.clone(),
                ctx: input.ctx,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                upstream_base_url: &prepared.upstream_base_url,
                upstream_operation: &prepared.operation,
                actual_model: &prepared.actual_model,
                status_code: stream_resp.status,
                start: input.start,
                first_byte_latency_ms,
                request_body: prepared.request_body_bytes,
                attempt_count: attempt_number,
                retry_count: attempt,
                total_backoff_ms: input.total_backoff_ms,
                routing_selected_reason: prepared.selected_via,
                routing_sticky_hit: prepared.sticky_hit,
                attempts,
            });

            record_sticky_session_success(input.state, input.ctx, &prepared.provider_name);

            tracing::info!(
                request_id = %input.ctx.request_id,
                model = %prepared.actual_model,
                public_model = %input.ctx.model,
                provider = %prepared.provider_name,
                status = stream_resp.status,
                first_byte_latency_ms = first_byte_latency_ms,
                stream = true,
                "Streaming request started"
            );

            AttemptExecution::Response(stream_response(
                stream_resp,
                Some(input.ctx.model.clone()),
                audit,
                prepared.stream_transform.take(),
            ))
        }
        Err(error) => {
            record_provider_failure(input.state, &prepared.provider_name);
            let should_retry = should_retry_provider_error(&error, input.retry_policy)
                && provider_attempt + 1 < input.max_attempts;
            let error_message = error.to_string();
            retry_attempts.push(build_retry_attempt_log(RetryAttemptInput {
                attempt: attempt_number,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: error.status_code().as_u16() as i64,
                error_code: Some(error.error_code().into_owned()),
                error_message: Some(error_message.clone()),
                retryable: should_retry,
                backoff_ms_before_next: should_retry
                    .then(|| {
                        next_retry_backoff_ms(
                            input.retry_policy,
                            provider_attempt,
                            input.max_attempts,
                        )
                    })
                    .flatten(),
                selected_via: prepared.selected_via,
                sticky_hit: prepared.sticky_hit,
                provider_healthy_before_attempt: prepared.provider_healthy_before_attempt,
            }));

            if should_retry {
                return AttemptExecution::Retry(error);
            }

            input
                .state
                .stats
                .record_error(&prepared.actual_model, &prepared.provider_name);
            let latency_ms = input.start.elapsed().as_millis() as i64;
            persist_error_request_log(PersistErrorRequestLogInput {
                state: input.state,
                ctx: input.ctx,
                api_key_id: input.api_key_id,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                error: &error,
                attempt_count: attempt_number,
                retry_count: attempt,
                total_backoff_ms: input.total_backoff_ms,
                sticky_hit: Some(prepared.sticky_hit),
                selected_provider_reason: Some(prepared.selected_via),
                attempts: retry_attempts,
                upstream_base_url: Some(&prepared.upstream_base_url),
                upstream_operation: Some(&prepared.operation),
                request_body: prepared.request_body_bytes.as_deref(),
                latency_ms,
            })
            .await;

            AttemptExecution::Fail(error)
        }
    }
}

async fn execute_non_stream_attempt(
    input: AttemptContext<'_>,
    prepared: PreparedAttempt,
    attempt: u32,
    provider_attempt: u32,
    retry_attempts: &mut Vec<RetryAttemptLog>,
) -> AttemptExecution {
    let attempt_number = attempt + 1;

    match prepared.provider.proxy(prepared.modified_req.clone()).await {
        Ok(response) => {
            let latency = input.start.elapsed();
            let latency_ms = latency.as_millis() as i64;
            let first_byte_latency_ms = response.first_byte_latency_ms as i64;
            let response_body_bytes = serde_json::to_vec(&response.body).ok();
            let downstream_body = if response.status < 400 {
                match translation::translate_response_body(
                    prepared.upstream_protocol,
                    input.ctx.operation,
                    response.body.clone(),
                    &input.ctx.model,
                ) {
                    Ok(body) => body,
                    Err(error) => {
                        record_provider_failure(input.state, &prepared.provider_name);
                        let error_message = error.to_string();
                        retry_attempts.push(build_retry_attempt_log(RetryAttemptInput {
                            attempt: attempt_number,
                            provider_name: &prepared.provider_name,
                            protocol: &prepared.protocol,
                            provider_model: &prepared.actual_model,
                            status_code: error.status_code().as_u16() as i64,
                            error_code: Some(error.error_code().into_owned()),
                            error_message: Some(error_message),
                            retryable: false,
                            backoff_ms_before_next: None,
                            selected_via: prepared.selected_via,
                            sticky_hit: prepared.sticky_hit,
                            provider_healthy_before_attempt: prepared
                                .provider_healthy_before_attempt,
                        }));
                        persist_error_request_log(PersistErrorRequestLogInput {
                            state: input.state,
                            ctx: input.ctx,
                            api_key_id: input.api_key_id,
                            provider_name: &prepared.provider_name,
                            protocol: &prepared.protocol,
                            provider_model: &prepared.actual_model,
                            error: &error,
                            attempt_count: attempt_number,
                            retry_count: attempt,
                            total_backoff_ms: input.total_backoff_ms,
                            sticky_hit: Some(prepared.sticky_hit),
                            selected_provider_reason: Some(prepared.selected_via),
                            attempts: retry_attempts,
                            upstream_base_url: Some(&prepared.upstream_base_url),
                            upstream_operation: Some(&prepared.operation),
                            request_body: prepared.request_body_bytes.as_deref(),
                            latency_ms,
                        })
                        .await;
                        return AttemptExecution::Fail(error);
                    }
                }
            } else {
                response.body.clone()
            };
            let cost_cents = calculate_cost_cents(
                &input.ctx.config_snapshot,
                &prepared.actual_model,
                &prepared.provider_name,
                response.tokens.as_ref(),
            );
            let (provider_error_code, provider_error_message) = if response.status >= 400 {
                audit::extract_provider_error(&response.body)
            } else {
                (None, None)
            };
            let should_retry_response = input.retry_policy.should_retry(response.status)
                && provider_attempt + 1 < input.max_attempts;
            retry_attempts.push(build_retry_attempt_log(RetryAttemptInput {
                attempt: attempt_number,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: response.status as i64,
                error_code: provider_error_code.clone(),
                error_message: provider_error_message.clone(),
                retryable: should_retry_response,
                backoff_ms_before_next: should_retry_response
                    .then(|| {
                        next_retry_backoff_ms(
                            input.retry_policy,
                            provider_attempt,
                            input.max_attempts,
                        )
                    })
                    .flatten(),
                selected_via: prepared.selected_via,
                sticky_hit: prepared.sticky_hit,
                provider_healthy_before_attempt: prepared.provider_healthy_before_attempt,
            }));

            if should_retry_response {
                record_provider_failure(input.state, &prepared.provider_name);
                return AttemptExecution::Retry(AppError::ProviderError {
                    provider_name: prepared.provider_name.clone(),
                    status_code: StatusCode::from_u16(response.status).ok(),
                    error_code: provider_error_code,
                    message: provider_error_message
                        .unwrap_or_else(|| format!("Provider returned {}", response.status)),
                });
            }

            if input.retry_policy.should_retry(response.status) {
                record_provider_failure(input.state, &prepared.provider_name);
                return AttemptExecution::RetryNextProvider(AppError::ProviderError {
                    provider_name: prepared.provider_name.clone(),
                    status_code: StatusCode::from_u16(response.status).ok(),
                    error_code: provider_error_code,
                    message: provider_error_message
                        .unwrap_or_else(|| format!("Provider returned {}", response.status)),
                });
            }

            record_completed_request_outcome(RequestOutcome {
                state: input.state.as_ref(),
                provider_name: &prepared.provider_name,
                actual_model: &prepared.actual_model,
                latency,
                tokens: response.tokens.as_ref(),
                cost_cents,
                api_key_id: input.api_key_id,
                success: response.status < 400,
            });

            let (input_tokens, output_tokens, total_tokens, cached_tokens, cache_write_tokens) =
                token_log_fields(response.tokens.as_ref());
            let metadata = log_metadata_json(LogMetadataInput {
                ctx: input.ctx,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                upstream_base_url: Some(&prepared.upstream_base_url),
                upstream_operation: Some(&prepared.operation),
                status_code: response.status as i64,
                error_code: provider_error_code.as_deref(),
                error_message: provider_error_message.as_deref(),
                attempt_count: attempt_number,
                retry_count: attempt,
                total_backoff_ms: input.total_backoff_ms,
                sticky_hit: Some(prepared.sticky_hit),
                selected_provider_reason: Some(prepared.selected_via),
                attempts: retry_attempts,
                upstream_path: None,
            });
            // Best-effort audit logging: a completed response should still be returned if persistence fails.
            if let Err(err) = audit::record_llm_request(
                input.state,
                NewRequestLog {
                    request_id: &input.ctx.request_id.to_string(),
                    api_key_id: input.api_key_id,
                    session_hash: input
                        .ctx
                        .session_affinity
                        .as_ref()
                        .map(|affinity| affinity.hash.as_str()),
                    provider_name: &prepared.provider_name,
                    protocol: &prepared.protocol,
                    model: &prepared.actual_model,
                    operation: &input.ctx.operation.to_string(),
                    status_code: response.status as i64,
                    success: response.status < 400 && provider_error_message.is_none(),
                    input_tokens,
                    output_tokens,
                    total_tokens,
                    cached_tokens,
                    cache_write_tokens,
                    cost_cents: cost_cents as i64,
                    latency_ms,
                    first_byte_latency_ms,
                    metadata_json: &metadata,
                    request_body: prepared.request_body_bytes.as_deref(),
                    response_body: response_body_bytes.as_deref(),
                },
            )
            .await
            {
                tracing::warn!(
                    request_id = %input.ctx.request_id,
                    error = %err,
                    "Failed to persist completed request audit"
                );
            }

            record_sticky_session_success(input.state, input.ctx, &prepared.provider_name);

            tracing::info!(
                request_id = %input.ctx.request_id,
                model = %prepared.actual_model,
                public_model = %input.ctx.resolved_model,
                provider = %prepared.provider_name,
                status = response.status,
                latency_ms = latency_ms,
                tokens = ?response.tokens,
                cost_cents = cost_cents,
                "Request completed"
            );

            let status =
                StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut body = downstream_body;
            if let Some(obj) = body.as_object_mut() {
                obj.insert(
                    "model".to_string(),
                    serde_json::Value::String(input.ctx.model.clone()),
                );
            }

            AttemptExecution::Response((status, Json(body)).into_response())
        }
        Err(error) => {
            record_provider_failure(input.state, &prepared.provider_name);
            let should_retry = should_retry_provider_error(&error, input.retry_policy)
                && provider_attempt + 1 < input.max_attempts;
            let error_message = error.to_string();
            retry_attempts.push(build_retry_attempt_log(RetryAttemptInput {
                attempt: attempt_number,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                status_code: error.status_code().as_u16() as i64,
                error_code: Some(error.error_code().into_owned()),
                error_message: Some(error_message.clone()),
                retryable: should_retry,
                backoff_ms_before_next: should_retry
                    .then(|| {
                        next_retry_backoff_ms(
                            input.retry_policy,
                            provider_attempt,
                            input.max_attempts,
                        )
                    })
                    .flatten(),
                selected_via: prepared.selected_via,
                sticky_hit: prepared.sticky_hit,
                provider_healthy_before_attempt: prepared.provider_healthy_before_attempt,
            }));

            if should_retry {
                return AttemptExecution::Retry(error);
            }

            if should_retry_provider_error(&error, input.retry_policy) {
                return AttemptExecution::RetryNextProvider(error);
            }

            input
                .state
                .stats
                .record_error(&prepared.actual_model, &prepared.provider_name);
            let latency_ms = input.start.elapsed().as_millis() as i64;
            persist_error_request_log(PersistErrorRequestLogInput {
                state: input.state,
                ctx: input.ctx,
                api_key_id: input.api_key_id,
                provider_name: &prepared.provider_name,
                protocol: &prepared.protocol,
                provider_model: &prepared.actual_model,
                error: &error,
                attempt_count: attempt_number,
                retry_count: attempt,
                total_backoff_ms: input.total_backoff_ms,
                sticky_hit: Some(prepared.sticky_hit),
                selected_provider_reason: Some(prepared.selected_via),
                attempts: retry_attempts,
                upstream_base_url: Some(&prepared.upstream_base_url),
                upstream_operation: Some(&prepared.operation),
                request_body: prepared.request_body_bytes.as_deref(),
                latency_ms,
            })
            .await;

            AttemptExecution::Fail(error)
        }
    }
}

async fn finalize_proxy_failure(input: ProxyFailureInput<'_>) -> AppError {
    let provider_model = input.last_actual_model.unwrap_or(&input.ctx.resolved_model);
    let provider_name = input.last_provider_name.unwrap_or("unrouted");
    let protocol = input
        .last_protocol
        .map(str::to_string)
        .unwrap_or_else(|| input.ctx.protocol.to_string());
    let had_last_error = input.last_error.is_some();
    let err = input
        .last_error
        .unwrap_or_else(|| AppError::NoProviderAvailable(input.ctx.model.clone()));

    if had_last_error {
        input
            .state
            .stats
            .record_error(provider_model, "retry_exhausted");
    }

    let latency_ms = input.start.elapsed().as_millis() as i64;
    persist_error_request_log(PersistErrorRequestLogInput {
        state: input.state,
        ctx: input.ctx,
        api_key_id: input.api_key_id,
        provider_name,
        protocol: &protocol,
        provider_model,
        error: &err,
        attempt_count: input.max_attempts,
        retry_count: input.max_attempts.saturating_sub(1),
        total_backoff_ms: input.total_backoff_ms,
        sticky_hit: None,
        selected_provider_reason: None,
        attempts: input.retry_attempts,
        upstream_base_url: input.last_upstream_base_url,
        upstream_operation: input.last_upstream_operation,
        request_body: input.request_body,
        latency_ms,
    })
    .await;

    err
}

struct StreamAudit {
    state: Arc<AppState>,
    config_snapshot: std::sync::Arc<crate::config_service::ConfigSnapshot>,
    request_id: String,
    request_headers: BTreeMap<String, String>,
    request_protocol: String,
    request_operation: String,
    api_key_id: String,
    provider_name: String,
    protocol: String,
    upstream_base_url: String,
    upstream_operation: String,
    requested_model: String,
    resolved_model: String,
    session_id: Option<String>,
    session_source: Option<String>,
    session_hash: Option<String>,
    session_affinity_key: Option<String>,
    sticky_enabled: bool,
    actual_model: String,
    operation: String,
    status_code: u16,
    start: Instant,
    first_byte_latency_ms: i64,
    first_chunk_seen: bool,
    request_body: Option<Vec<u8>>,
    attempt_count: u32,
    retry_count: u32,
    total_backoff_ms: u64,
    routing_selected_reason: &'static str,
    routing_sticky_hit: bool,
    attempts: Vec<RetryAttemptLog>,
    tokens: Option<TokenUsage>,
    error_code: Option<String>,
    error_message: Option<String>,
    sse_buffer: Vec<u8>,
    finished: bool,
}

impl StreamAudit {
    fn new(input: StreamAuditInit<'_>) -> Self {
        Self {
            state: input.state,
            config_snapshot: input.ctx.config_snapshot.clone(),
            request_id: input.ctx.request_id.to_string(),
            request_headers: input.ctx.request_headers.clone(),
            request_protocol: input.ctx.protocol.to_string(),
            request_operation: input.ctx.operation.to_string(),
            api_key_id: auth::persisted_api_key_id(&input.ctx.auth_key).to_string(),
            provider_name: input.provider_name.to_string(),
            protocol: input.protocol.to_string(),
            upstream_base_url: input.upstream_base_url.to_string(),
            upstream_operation: input.upstream_operation.to_string(),
            requested_model: input.ctx.model.clone(),
            resolved_model: input.ctx.resolved_model.clone(),
            session_id: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.id.clone()),
            session_source: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.source.clone()),
            session_hash: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.hash.clone()),
            session_affinity_key: input
                .ctx
                .session_affinity
                .as_ref()
                .map(|affinity| affinity.key.as_str().to_string()),
            sticky_enabled: input.ctx.config_snapshot.config.routing.sticky.enabled,
            actual_model: input.actual_model.to_string(),
            operation: input.ctx.operation.to_string(),
            status_code: input.status_code,
            start: input.start,
            first_byte_latency_ms: input.first_byte_latency_ms,
            first_chunk_seen: false,
            request_body: input.request_body,
            attempt_count: input.attempt_count,
            retry_count: input.retry_count,
            total_backoff_ms: input.total_backoff_ms,
            routing_selected_reason: input.routing_selected_reason,
            routing_sticky_hit: input.routing_sticky_hit,
            attempts: input.attempts,
            tokens: None,
            error_code: None,
            error_message: None,
            sse_buffer: Vec::new(),
            finished: false,
        }
    }

    fn observe_chunk(&mut self, chunk: &[u8]) {
        if !self.first_chunk_seen {
            self.first_chunk_seen = true;
            self.first_byte_latency_ms = self.start.elapsed().as_millis() as i64;
        }

        self.sse_buffer.extend_from_slice(chunk);

        while let Some(pos) = self.sse_buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.sse_buffer[..pos].to_vec();
            self.sse_buffer.drain(..=pos);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.observe_sse_line(&line);
        }

        if self.sse_buffer.len() > 256 * 1024 {
            self.sse_buffer.clear();
        }
    }

    fn observe_sse_line(&mut self, line: &[u8]) {
        let Some(data) = line.strip_prefix(b"data:") else {
            return;
        };
        let data = trim_ascii_whitespace(data);
        if data.is_empty() || data == b"[DONE]" {
            return;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
            return;
        };
        self.observe_json_event(&value);
    }

    fn observe_json_event(&mut self, value: &serde_json::Value) {
        if self.protocol == "messages" {
            if let Some(usage) = value
                .get("message")
                .and_then(|message| message.get("usage"))
            {
                let mut tokens = self.tokens.take().unwrap_or_default();
                tokens.apply_anthropic_stream_usage(usage);
                self.tokens = Some(tokens);
            }
            if let Some(usage) = value.get("usage") {
                let mut tokens = self.tokens.take().unwrap_or_default();
                tokens.apply_anthropic_stream_usage(usage);
                self.tokens = Some(tokens);
            }
        } else {
            let usage = value.get("usage").or_else(|| {
                value
                    .get("response")
                    .and_then(|response| response.get("usage"))
            });
            if let Some(usage) = usage.and_then(TokenUsage::from_openai_usage) {
                self.tokens = Some(usage);
            }
        }

        let (error_code, error_message) = audit::extract_provider_error(value);
        if error_code.is_some() || error_message.is_some() {
            self.error_code = error_code;
            self.error_message = error_message;
        }
    }

    fn completion_info(&self, stream_error: Option<(&str, String)>) -> StreamAuditCompletion {
        let (forced_code, forced_message) = stream_error
            .map(|(code, message)| (Some(code.to_string()), Some(message)))
            .unwrap_or((None, None));
        let error_code = forced_code.or_else(|| {
            self.error_code
                .clone()
                .or_else(|| (self.status_code >= 400).then(|| "stream_response_error".to_string()))
        });
        let error_message = forced_message.or_else(|| {
            self.error_message.clone().or_else(|| {
                (self.status_code >= 400)
                    .then(|| format!("Streaming response returned {}", self.status_code))
            })
        });
        let success = self.status_code < 400 && error_message.is_none();
        let latency = self.start.elapsed();
        let cost_cents = if success {
            calculate_cost_cents(
                &self.config_snapshot,
                &self.actual_model,
                &self.provider_name,
                self.tokens.as_ref(),
            )
        } else {
            0
        };

        StreamAuditCompletion {
            success,
            latency,
            latency_ms: latency.as_millis() as i64,
            cost_cents,
            error_code,
            error_message,
        }
    }

    fn record_completion_stats(&self, completion: &StreamAuditCompletion) {
        record_completed_request_outcome(RequestOutcome {
            state: &self.state,
            provider_name: &self.provider_name,
            actual_model: &self.actual_model,
            latency: completion.latency,
            tokens: self.tokens.as_ref(),
            cost_cents: completion.cost_cents,
            api_key_id: &self.api_key_id,
            success: completion.success,
        });
    }

    fn spawn_persist_completion_log(
        &self,
        completion: StreamAuditCompletion,
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
        cached_tokens: i64,
        cache_write_tokens: i64,
    ) {
        let state = self.state.clone();
        let request_id = self.request_id.clone();
        let request_headers = self.request_headers.clone();
        let request_protocol = self.request_protocol.clone();
        let request_operation = self.request_operation.clone();
        let api_key_id = self.api_key_id.clone();
        let provider_name = self.provider_name.clone();
        let protocol = self.protocol.clone();
        let upstream_base_url = self.upstream_base_url.clone();
        let upstream_operation = self.upstream_operation.clone();
        let actual_model = self.actual_model.clone();
        let operation = self.operation.clone();
        let status_code = self.status_code;
        let first_byte_latency_ms = self.first_byte_latency_ms;
        let request_body = self.request_body.clone();
        let session_hash = self.session_hash.clone();
        let error_code = completion.error_code.clone();
        let error_message = completion.error_message.clone();
        let session_id = self.session_id.clone();
        let session_source = self.session_source.clone();
        let session_affinity_key = self.session_affinity_key.clone();
        let requested_model = self.requested_model.clone();
        let resolved_model = self.resolved_model.clone();
        let sticky_enabled = self.sticky_enabled;
        let currency = self.config_snapshot.config.cost.currency.clone();
        let metadata = build_log_metadata_json(LogMetadata {
            request_id: &request_id,
            request_headers: &request_headers,
            request_protocol: &request_protocol,
            request_operation: &request_operation,
            request_stream: true,
            session: session_id.as_deref().map(|id| LogSessionMetadata {
                id,
                source: session_source.as_deref(),
                hash: session_hash.as_deref(),
                affinity_key: session_affinity_key.as_deref(),
            }),
            requested_model: &requested_model,
            resolved_model: &resolved_model,
            provider_model: &actual_model,
            sticky_enabled,
            provider_name: &provider_name,
            protocol: &protocol,
            status_code: status_code as i64,
            error_code: error_code.as_deref(),
            error_message: error_message.as_deref(),
            attempt_count: self.attempt_count,
            retry_count: self.retry_count,
            total_backoff_ms: self.total_backoff_ms,
            sticky_hit: Some(self.routing_sticky_hit),
            selected_provider_reason: Some(self.routing_selected_reason),
            attempts: &self.attempts,
            upstream_base_url: Some(&upstream_base_url),
            upstream_operation: Some(&upstream_operation),
            upstream_path: None,
            currency: &currency,
        });

        tokio::spawn(async move {
            // Best-effort audit logging: streaming completion should not block socket teardown.
            if let Err(err) = audit::record_llm_request(
                &state,
                NewRequestLog {
                    request_id: &request_id,
                    api_key_id: &api_key_id,
                    session_hash: session_hash.as_deref(),
                    provider_name: &provider_name,
                    protocol: &protocol,
                    model: &actual_model,
                    operation: &operation,
                    status_code: status_code as i64,
                    success: completion.success,
                    input_tokens,
                    output_tokens,
                    total_tokens,
                    cached_tokens,
                    cache_write_tokens,
                    cost_cents: completion.cost_cents as i64,
                    latency_ms: completion.latency_ms,
                    first_byte_latency_ms,
                    metadata_json: &metadata,
                    request_body: request_body.as_deref(),
                    response_body: None,
                },
            )
            .await
            {
                tracing::warn!(
                    request_id = %request_id,
                    error = %err,
                    "Failed to persist streaming completion audit"
                );
            }
        });
    }

    fn finish(&mut self, stream_error: Option<(&str, String)>) {
        if self.finished {
            return;
        }
        self.finished = true;

        let completion = self.completion_info(stream_error);
        self.record_completion_stats(&completion);

        let (input_tokens, output_tokens, total_tokens, cached_tokens, cache_write_tokens) =
            token_log_fields(self.tokens.as_ref());
        self.spawn_persist_completion_log(
            completion,
            input_tokens,
            output_tokens,
            total_tokens,
            cached_tokens,
            cache_write_tokens,
        );
    }
}

struct AuditedSseStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    alias: Option<String>,
    audit: StreamAudit,
    transform: Option<SseTransform>,
    pending_output: VecDeque<Bytes>,
    inner_finished: bool,
    terminated: bool,
}

impl Stream for AuditedSseStream {
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if self.terminated {
                return Poll::Ready(None);
            }

            if let Some(chunk) = self.pending_output.pop_front() {
                return Poll::Ready(Some(Ok(chunk)));
            }

            if self
                .transform
                .as_ref()
                .is_some_and(SseTransform::is_finished)
            {
                self.terminated = true;
                self.audit.finish(None);
                return Poll::Ready(None);
            }

            match self.inner.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(chunk))) => {
                    self.audit.observe_chunk(&chunk);
                    let outputs = if let Some(transform) = self.transform.as_mut() {
                        transform.push_chunk(&chunk)
                    } else {
                        let output = if let Some(alias) = self.alias.clone() {
                            Bytes::from(rewrite_sse_model(&chunk, &alias))
                        } else {
                            chunk
                        };
                        vec![output]
                    };
                    self.pending_output.extend(outputs);
                    if let Some(output) = self.pending_output.pop_front() {
                        return Poll::Ready(Some(Ok(output)));
                    }
                    if self
                        .transform
                        .as_ref()
                        .is_some_and(SseTransform::is_finished)
                    {
                        self.terminated = true;
                        self.audit.finish(None);
                        return Poll::Ready(None);
                    }

                    // A transformed chunk can contain only a partial SSE event or an event
                    // that intentionally produces no downstream frame. Poll the inner stream
                    // again so it registers the waker (or yields an already-buffered chunk)
                    // instead of leaving the response suspended until the request timeout.
                    continue;
                }
                Poll::Ready(Some(Err(err))) => {
                    let message = err.to_string();
                    self.terminated = true;
                    self.audit.finish(Some(("stream_error", message)));
                    return Poll::Ready(Some(Err(err)));
                }
                Poll::Ready(None) => {
                    if self.inner_finished {
                        self.terminated = true;
                        self.audit.finish(None);
                        return Poll::Ready(None);
                    }

                    self.inner_finished = true;
                    if let Some(transform) = self.transform.as_mut() {
                        let outputs = transform.finish();
                        self.pending_output.extend(outputs);
                    }

                    if let Some(output) = self.pending_output.pop_front() {
                        return Poll::Ready(Some(Ok(output)));
                    }
                    self.terminated = true;
                    self.audit.finish(None);
                    return Poll::Ready(None);
                }
            }
        }
    }
}

impl Drop for AuditedSseStream {
    fn drop(&mut self) {
        if !self.audit.finished {
            self.audit.finish(Some((
                "stream_closed",
                "Streaming response closed before completion".to_string(),
            )));
        }
    }
}

/// Shared proxy logic used by all protocol handlers
pub async fn proxy_to_provider(
    state: Arc<AppState>,
    req: ProxyRequest,
    ctx: ProxyContext,
) -> AppResult<Response> {
    let start = std::time::Instant::now();
    let original_request_body_bytes = serde_json::to_vec(&req.body).ok();
    let api_key_id = auth::persisted_api_key_id(&ctx.auth_key);
    let retry_policy = RetryPolicy::from_config(&ctx.config_snapshot.config.retry);
    let max_attempts = retry_policy.max_attempts();
    let mut last_error: Option<AppError> = None;
    let mut last_provider_name: Option<String> = None;
    let mut last_protocol: Option<String> = None;
    let mut last_upstream_base_url: Option<String> = None;
    let mut last_upstream_operation: Option<String> = None;
    let mut last_actual_model: Option<String> = None;
    let mut total_backoff_ms = 0u64;
    let mut exhausted_providers = HashSet::new();
    let mut retry_attempts = Vec::new();
    let mut request_attempt = 0u32;
    let mut pending_backoff: Option<std::time::Duration> = None;

    loop {
        let prepared = match prepare_attempt(&state, &req, &ctx, &mut exhausted_providers) {
            Ok(prepared) => prepared,
            Err(err) => {
                return Err(finalize_proxy_failure(ProxyFailureInput {
                    state: &state,
                    ctx: &ctx,
                    api_key_id,
                    start,
                    max_attempts: request_attempt.max(1),
                    total_backoff_ms,
                    last_error: last_error.or(Some(err)),
                    last_provider_name: last_provider_name.as_deref(),
                    last_protocol: last_protocol.as_deref(),
                    last_upstream_base_url: last_upstream_base_url.as_deref(),
                    last_upstream_operation: last_upstream_operation.as_deref(),
                    last_actual_model: last_actual_model.as_deref(),
                    retry_attempts: &retry_attempts,
                    request_body: original_request_body_bytes.as_deref(),
                })
                .await);
            }
        };

        let provider_name = prepared.provider_name.clone();
        last_provider_name = Some(prepared.provider_name.clone());
        last_protocol = Some(prepared.protocol.clone());
        last_upstream_base_url = Some(prepared.upstream_base_url.clone());
        last_upstream_operation = Some(prepared.operation.clone());

        for provider_attempt in 0..max_attempts {
            if let Some(backoff) = pending_backoff.take() {
                let backoff_ms = backoff.as_millis() as u64;
                total_backoff_ms = total_backoff_ms.saturating_add(backoff_ms);
                tracing::warn!(
                    request_id = %ctx.request_id,
                    retry_count = request_attempt,
                    next_attempt = request_attempt + 1,
                    provider_attempt = provider_attempt + 1,
                    max_attempts = max_attempts,
                    provider = %provider_name,
                    backoff_ms,
                    "Retrying request"
                );
                tokio::time::sleep(backoff).await;
                state.router.check_recovery();
            }

            last_actual_model = Some(prepared.actual_model.clone());
            let attempt = request_attempt;
            request_attempt = request_attempt.saturating_add(1);

            let execution = if ctx.stream {
                execute_stream_attempt(
                    AttemptContext {
                        state: &state,
                        ctx: &ctx,
                        retry_policy: &retry_policy,
                        api_key_id,
                        start,
                        max_attempts,
                        total_backoff_ms,
                    },
                    prepared.clone(),
                    attempt,
                    provider_attempt,
                    &mut retry_attempts,
                )
                .await
            } else {
                execute_non_stream_attempt(
                    AttemptContext {
                        state: &state,
                        ctx: &ctx,
                        retry_policy: &retry_policy,
                        api_key_id,
                        start,
                        max_attempts,
                        total_backoff_ms,
                    },
                    prepared.clone(),
                    attempt,
                    provider_attempt,
                    &mut retry_attempts,
                )
                .await
            };

            match execution {
                AttemptExecution::Response(response) => return Ok(response),
                AttemptExecution::Retry(err) => {
                    last_error = Some(err);
                    pending_backoff = Some(retry_policy.backoff_for(provider_attempt));
                    if provider_attempt + 1 >= max_attempts {
                        exhausted_providers.insert(provider_name.clone());
                        break;
                    }
                }
                AttemptExecution::RetryNextProvider(err) => {
                    last_error = Some(err);
                    exhausted_providers.insert(provider_name.clone());
                    break;
                }
                AttemptExecution::Fail(err) => return Err(err),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_sse_model_single_event() {
        let chunk = "data: {\"id\":\"chatcmpl-123\",\"model\":\"gpt-4o\",\"choices\":[]}\n\n";
        let result = rewrite_sse_model(chunk.as_bytes(), "my-gpt");
        let text = String::from_utf8(result).unwrap();
        assert!(text.contains("\"model\":\"my-gpt\""));
        assert!(!text.contains("\"model\":\"gpt-4o\""));
    }

    #[test]
    fn test_rewrite_sse_model_done() {
        let chunk = "data: [DONE]\n\n";
        let result = rewrite_sse_model(chunk.as_bytes(), "my-gpt");
        assert_eq!(String::from_utf8(result).unwrap(), "data: [DONE]\n\n");
    }

    #[test]
    fn test_rewrite_sse_model_non_data_line() {
        let chunk = "event: message\n";
        let result = rewrite_sse_model(chunk.as_bytes(), "my-gpt");
        assert_eq!(String::from_utf8(result).unwrap(), "event: message\n");
    }

    #[test]
    fn test_rewrite_sse_model_multiple_events() {
        let chunk = "data: {\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: {\"model\":\"gpt-4o\",\"choices\":[{\"delta\":{\"content\":\"!\"}}]}\n\ndata: [DONE]\n\n";
        let result = rewrite_sse_model(chunk.as_bytes(), "my-alias");
        let text = String::from_utf8(result).unwrap();
        assert!(!text.contains("gpt-4o"));
        assert_eq!(text.matches("my-alias").count(), 2);
        assert!(text.contains("[DONE]"));
    }

    #[test]
    fn openai_to_anthropic_handles_fragmented_single_newline_sse() {
        let mut transform = OpenAiToAnthropicStream::new("public-model");

        let first = transform.push_chunk(
            b"data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"index\":0,\"delta\":{\"cont",
        );
        assert!(first.is_empty());

        let output = transform.push_chunk(b"ent\":\"hello\"},\"finish_reason\":null}]}\n");
        let output = String::from_utf8(
            output
                .into_iter()
                .flat_map(|chunk| chunk.to_vec())
                .collect(),
        )
        .unwrap();
        assert!(output.contains("event: message_start"));
        assert!(output.contains("\"type\":\"text_delta\""));
        assert!(output.contains("\"text\":\"hello\""));

        assert!(transform
            .push_chunk(
                b"data: {\"id\":\"chatcmpl-test\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n"
            )
            .is_empty());
        let final_output = transform.push_chunk(b"data: [DONE]\n");
        let final_output = String::from_utf8(
            final_output
                .into_iter()
                .flat_map(|chunk| chunk.to_vec())
                .collect(),
        )
        .unwrap();
        assert!(final_output.contains("event: message_delta"));
        assert!(final_output.contains("event: message_stop"));
    }

    #[test]
    fn openai_to_anthropic_preserves_multibyte_text_across_byte_chunks() {
        let mut transform = OpenAiToAnthropicStream::new("public-model");
        let frame = "data: {\"id\":\"chatcmpl-unicode\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"你好😀🎉\"},\"finish_reason\":null}]}\n";
        let mut output = Vec::new();

        for byte in frame.as_bytes() {
            output.extend(transform.push_chunk(std::slice::from_ref(byte)));
        }

        let output = String::from_utf8(
            output
                .into_iter()
                .flat_map(|chunk| chunk.to_vec())
                .collect(),
        )
        .unwrap();
        assert!(output.contains("\"text\":\"你好😀🎉\""));
        assert!(!output.contains('�'));
    }

    #[test]
    fn anthropic_to_openai_preserves_multibyte_text_across_byte_chunks() {
        let mut transform = AnthropicToOpenAiStream::new("public-model");
        let frame = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"你好😀🎉\"}}\n\n"
        );
        let mut output = Vec::new();

        for byte in frame.as_bytes() {
            output.extend(transform.push_chunk(std::slice::from_ref(byte)));
        }

        let output = String::from_utf8(
            output
                .into_iter()
                .flat_map(|chunk| chunk.to_vec())
                .collect(),
        )
        .unwrap();
        assert!(output.contains("\"content\":\"你好😀🎉\""));
        assert!(!output.contains('�'));
    }
}

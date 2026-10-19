//! 两种端点适配器：OpenAI chat completions 与 Anthropic messages。
//!
//! 非流式是默认。`stream = true` 时把 SSE 组装成同一种 [`ModelMessage`]。
//! 明文密钥只放进请求头，不进 Debug，也不进错误文本。

use crate::error::{redact, Error, Result};
use crate::http::HttpClient;
use crate::spec::{EffectClass, EndpointSpec, Protocol, ReasoningTier, ToolDecl};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;

/// Anthropic 要求的版本头。
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

const ANTHROPIC_MAX_TOKENS_PLAIN: u32 = 1024;
const THINKING_LOW: u32 = 1024;
const THINKING_MEDIUM: u32 = 2048;
const THINKING_HIGH: u32 = 4096;

/// 调用方注入的明文密钥。Debug / Display 只显示占位。
#[derive(Clone)]
pub struct EndpointKey {
    value: String,
}

impl EndpointKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for EndpointKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EndpointKey(***)")
    }
}

impl fmt::Display for EndpointKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EndpointKey(***)")
    }
}

/// 模型一轮的归一化结果。两种协议都落到这里。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelMessage {
    pub text: String,
    pub tool_calls: Vec<ParsedToolCall>,
    pub finish: String,
}

/// 一次工具调用。`arguments` 已解析成 JSON。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// 协议无关的对话快照。续跑时整段落库。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case", deny_unknown_fields)]
pub enum TranscriptItem {
    System {
        text: String,
    },
    User {
        text: String,
    },
    Assistant {
        text: String,
        tool_calls: Vec<ParsedToolCall>,
    },
    Tool {
        call_id: String,
        name: String,
        output: Value,
        is_error: bool,
    },
}

/// 发一轮请求并解析。`tools` 是已经求交之后的可见工具，不是规格全文。
pub fn complete_once(
    client: &dyn HttpClient,
    endpoint: &EndpointSpec,
    key: &EndpointKey,
    transcript: &[TranscriptItem],
    tools: &[ToolDecl],
) -> Result<ModelMessage> {
    let url = endpoint_url(endpoint)?;
    let body = request_body(endpoint, transcript, tools)?;
    let body_text = serde_json::to_string(&body)?;
    let secret = key.expose();
    let headers = request_headers(endpoint, secret);
    let header_refs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let response = client.post_json(&url, &header_refs, &body_text, secret)?;
    if !(200..300).contains(&response.status) {
        return Err(Error::Endpoint {
            status: response.status,
            message: redact(&truncate(&response.body), secret),
        });
    }
    let message = if endpoint.stream {
        parse_stream(endpoint.protocol, &response.body)?
    } else {
        parse_json(endpoint.protocol, &response.body)?
    };
    Ok(message)
}

pub(crate) fn endpoint_url(endpoint: &EndpointSpec) -> Result<String> {
    let base = endpoint.base_url.trim_end_matches('/');
    let path = match endpoint.protocol {
        Protocol::OpenaiChatCompletions => "chat/completions",
        Protocol::AnthropicMessages => "v1/messages",
    };
    Ok(format!("{base}/{path}"))
}

fn request_headers(endpoint: &EndpointSpec, secret: &str) -> Vec<(String, String)> {
    let mut headers = vec![("content-type".into(), "application/json".into())];
    match endpoint.protocol {
        Protocol::OpenaiChatCompletions => {
            headers.push(("authorization".into(), format!("Bearer {secret}")));
        }
        Protocol::AnthropicMessages => {
            headers.push(("x-api-key".into(), secret.to_string()));
            headers.push(("anthropic-version".into(), ANTHROPIC_VERSION.into()));
        }
    }
    headers
}

fn request_body(
    endpoint: &EndpointSpec,
    transcript: &[TranscriptItem],
    tools: &[ToolDecl],
) -> Result<Value> {
    match endpoint.protocol {
        Protocol::OpenaiChatCompletions => Ok(openai_body(endpoint, transcript, tools)),
        Protocol::AnthropicMessages => Ok(anthropic_body(endpoint, transcript, tools)),
    }
}

fn openai_body(
    endpoint: &EndpointSpec,
    transcript: &[TranscriptItem],
    tools: &[ToolDecl],
) -> Value {
    let messages: Vec<Value> = transcript.iter().map(openai_message).collect();
    let mut body = json!({
        "model": endpoint.model,
        "messages": messages,
        "stream": endpoint.stream,
    });
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(openai_tool).collect::<Vec<_>>());
    }
    if let Some(effort) = reasoning_effort(endpoint.reasoning_tier) {
        body["reasoning_effort"] = json!(effort);
    }
    body
}

fn openai_message(item: &TranscriptItem) -> Value {
    match item {
        TranscriptItem::System { text } => json!({"role": "system", "content": text}),
        TranscriptItem::User { text } => json!({"role": "user", "content": text}),
        TranscriptItem::Assistant { text, tool_calls } => {
            let mut message = json!({"role": "assistant"});
            if text.is_empty() && !tool_calls.is_empty() {
                message["content"] = Value::Null;
            } else {
                message["content"] = json!(text);
            }
            if !tool_calls.is_empty() {
                message["tool_calls"] = json!(tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments.to_string(),
                            }
                        })
                    })
                    .collect::<Vec<_>>());
            }
            message
        }
        TranscriptItem::Tool {
            call_id, output, ..
        } => json!({
            "role": "tool",
            "tool_call_id": call_id,
            "content": output.to_string(),
        }),
    }
}

fn openai_tool(tool: &ToolDecl) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }
    })
}

fn anthropic_body(
    endpoint: &EndpointSpec,
    transcript: &[TranscriptItem],
    tools: &[ToolDecl],
) -> Value {
    let system = transcript.iter().rev().find_map(|item| match item {
        TranscriptItem::System { text } => Some(text.clone()),
        _ => None,
    });
    let messages = anthropic_messages(transcript);
    let max_tokens = anthropic_max_tokens(endpoint.reasoning_tier);
    let mut body = json!({
        "model": endpoint.model,
        "max_tokens": max_tokens,
        "messages": messages,
        "stream": endpoint.stream,
    });
    if let Some(system) = system {
        body["system"] = json!(system);
    }
    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(anthropic_tool).collect::<Vec<_>>());
    }
    if let Some(budget) = thinking_budget(endpoint.reasoning_tier) {
        body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
    }
    body
}

fn anthropic_messages(transcript: &[TranscriptItem]) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut index = 0;
    while index < transcript.len() {
        match &transcript[index] {
            TranscriptItem::System { .. } => index += 1,
            TranscriptItem::User { text } => {
                messages.push(json!({"role": "user", "content": text}));
                index += 1;
            }
            TranscriptItem::Assistant { text, tool_calls } => {
                let mut blocks = Vec::new();
                if !text.is_empty() {
                    blocks.push(json!({"type": "text", "text": text}));
                }
                for call in tool_calls {
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.name,
                        "input": call.arguments,
                    }));
                }
                messages.push(json!({"role": "assistant", "content": blocks}));
                index += 1;
            }
            TranscriptItem::Tool { .. } => {
                let mut blocks = Vec::new();
                while index < transcript.len() {
                    let TranscriptItem::Tool {
                        call_id,
                        output,
                        is_error,
                        ..
                    } = &transcript[index]
                    else {
                        break;
                    };
                    blocks.push(json!({
                        "type": "tool_result",
                        "tool_use_id": call_id,
                        "content": output.to_string(),
                        "is_error": is_error,
                    }));
                    index += 1;
                }
                messages.push(json!({"role": "user", "content": blocks}));
            }
        }
    }
    messages
}

fn anthropic_tool(tool: &ToolDecl) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.parameters,
    })
}

fn reasoning_effort(tier: ReasoningTier) -> Option<&'static str> {
    match tier {
        ReasoningTier::Off => None,
        ReasoningTier::Low => Some("low"),
        ReasoningTier::Medium => Some("medium"),
        ReasoningTier::High => Some("high"),
    }
}

fn thinking_budget(tier: ReasoningTier) -> Option<u32> {
    match tier {
        ReasoningTier::Off => None,
        ReasoningTier::Low => Some(THINKING_LOW),
        ReasoningTier::Medium => Some(THINKING_MEDIUM),
        ReasoningTier::High => Some(THINKING_HIGH),
    }
}

fn anthropic_max_tokens(tier: ReasoningTier) -> u32 {
    match thinking_budget(tier) {
        Some(budget) => budget.saturating_add(ANTHROPIC_MAX_TOKENS_PLAIN),
        None => ANTHROPIC_MAX_TOKENS_PLAIN,
    }
}

fn parse_json(protocol: Protocol, body: &str) -> Result<ModelMessage> {
    let value: Value = serde_json::from_str(body)
        .map_err(|err| Error::Protocol(format!("响应不是 JSON: {err}")))?;
    match protocol {
        Protocol::OpenaiChatCompletions => parse_openai_json(&value),
        Protocol::AnthropicMessages => parse_anthropic_json(&value),
    }
}

fn parse_stream(protocol: Protocol, body: &str) -> Result<ModelMessage> {
    match protocol {
        Protocol::OpenaiChatCompletions => parse_openai_sse(body),
        Protocol::AnthropicMessages => parse_anthropic_sse(body),
    }
}

fn parse_openai_json(value: &Value) -> Result<ModelMessage> {
    let message = value
        .pointer("/choices/0/message")
        .ok_or_else(|| Error::Protocol("openai 响应缺少 choices[0].message".into()))?;
    let text = match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => {
            return Err(Error::Protocol(format!(
                "openai content 不是字符串: {other}"
            )))
        }
    };
    let mut tool_calls = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            tool_calls.push(parse_openai_tool_call(call, index)?);
        }
    }
    let finish = value
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop")
        .to_string();
    Ok(ModelMessage {
        text,
        tool_calls,
        finish,
    })
}

fn parse_openai_tool_call(call: &Value, index: usize) -> Result<ParsedToolCall> {
    let id = call
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("call-{index}"));
    let function = call
        .get("function")
        .ok_or_else(|| Error::Protocol("openai tool_call 缺少 function".into()))?;
    let name = function
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Protocol("openai tool_call 缺少函数名".into()))?
        .to_string();
    let arguments = json_arguments(function.get("arguments"))?;
    Ok(ParsedToolCall {
        id,
        name,
        arguments,
    })
}

fn parse_openai_sse(body: &str) -> Result<ModelMessage> {
    let mut text = String::new();
    let mut calls: Vec<CallAcc> = Vec::new();
    let mut finish = String::from("stop");
    for data in sse_data(body) {
        if data == "[DONE]" {
            continue;
        }
        let value: Value = serde_json::from_str(data)
            .map_err(|err| Error::Protocol(format!("openai SSE 不是 JSON: {err}")))?;
        if let Some(reason) = value
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
        {
            finish = reason.to_string();
        }
        let Some(delta) = value.pointer("/choices/0/delta") else {
            continue;
        };
        if let Some(piece) = delta.get("content").and_then(Value::as_str) {
            text.push_str(piece);
        }
        if let Some(deltas) = delta.get("tool_calls").and_then(Value::as_array) {
            for delta_call in deltas {
                let index = delta_call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if calls.len() <= index {
                    calls.resize(index + 1, CallAcc::default());
                }
                let acc = &mut calls[index];
                if let Some(id) = delta_call.get("id").and_then(Value::as_str) {
                    acc.id = id.to_string();
                }
                if let Some(name) = delta_call.pointer("/function/name").and_then(Value::as_str) {
                    acc.name.push_str(name);
                }
                if let Some(arguments) = delta_call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                {
                    acc.arguments.push_str(arguments);
                }
            }
        }
    }
    let tool_calls = finish_calls(&calls)?;
    Ok(ModelMessage {
        text,
        tool_calls,
        finish,
    })
}

fn parse_anthropic_json(value: &Value) -> Result<ModelMessage> {
    let blocks = value
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Protocol("anthropic 响应缺少 content 数组".into()))?;
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(piece) = block.get("text").and_then(Value::as_str) {
                    text.push_str(piece);
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("toolu")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::Protocol("anthropic tool_use 缺少 name".into()))?
                    .to_string();
                let arguments = block.get("input").cloned().unwrap_or_else(|| json!({}));
                tool_calls.push(ParsedToolCall {
                    id,
                    name,
                    arguments,
                });
            }
            _ => {}
        }
    }
    let finish = value
        .get("stop_reason")
        .and_then(Value::as_str)
        .unwrap_or("end_turn")
        .to_string();
    Ok(ModelMessage {
        text,
        tool_calls,
        finish,
    })
}

fn parse_anthropic_sse(body: &str) -> Result<ModelMessage> {
    let mut text = String::new();
    let mut calls: Vec<CallAcc> = Vec::new();
    let mut finish = String::from("end_turn");
    let mut event_name = String::new();
    for line in body.split('\n') {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = rest.trim().to_string();
            continue;
        }
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(data)
            .map_err(|err| Error::Protocol(format!("anthropic SSE 不是 JSON: {err}")))?;
        if event_name == "message_delta" {
            if let Some(reason) = value.pointer("/delta/stop_reason").and_then(Value::as_str) {
                finish = reason.to_string();
            }
        }
        if event_name == "content_block_start" {
            let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let Some(block) = value.get("content_block") else {
                continue;
            };
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                if calls.len() <= index {
                    calls.resize(index + 1, CallAcc::default());
                }
                calls[index].id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("toolu")
                    .to_string();
                calls[index].name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
        }
        if event_name == "content_block_delta" {
            let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let Some(delta) = value.get("delta") else {
                continue;
            };
            match delta.get("type").and_then(Value::as_str) {
                Some("text_delta") => {
                    if let Some(piece) = delta.get("text").and_then(Value::as_str) {
                        text.push_str(piece);
                    }
                }
                Some("input_json_delta") => {
                    if calls.len() <= index {
                        calls.resize(index + 1, CallAcc::default());
                    }
                    if let Some(partial) = delta.get("partial_json").and_then(Value::as_str) {
                        calls[index].arguments.push_str(partial);
                    }
                }
                _ => {}
            }
        }
    }
    let tool_calls = finish_calls(&calls)?;
    Ok(ModelMessage {
        text,
        tool_calls,
        finish,
    })
}

#[derive(Clone, Default)]
struct CallAcc {
    id: String,
    name: String,
    arguments: String,
}

fn finish_calls(calls: &[CallAcc]) -> Result<Vec<ParsedToolCall>> {
    let mut out = Vec::new();
    for (index, call) in calls.iter().enumerate() {
        if call.name.is_empty() && call.arguments.is_empty() && call.id.is_empty() {
            continue;
        }
        if call.name.is_empty() {
            return Err(Error::Protocol(format!("流式工具调用 {index} 缺少名称")));
        }
        let arguments = if call.arguments.is_empty() {
            json!({})
        } else {
            serde_json::from_str(&call.arguments)
                .map_err(|err| Error::Protocol(format!("流式工具参数不是 JSON: {err}")))?
        };
        let id = if call.id.is_empty() {
            format!("call-{index}")
        } else {
            call.id.clone()
        };
        out.push(ParsedToolCall {
            id,
            name: call.name.clone(),
            arguments,
        });
    }
    Ok(out)
}

fn json_arguments(value: Option<&Value>) -> Result<Value> {
    match value {
        None | Some(Value::Null) => Ok(json!({})),
        Some(Value::String(text)) if text.is_empty() => Ok(json!({})),
        Some(Value::String(text)) => serde_json::from_str(text)
            .map_err(|err| Error::Protocol(format!("工具参数不是 JSON: {err}"))),
        Some(other) if other.is_object() || other.is_array() => Ok(other.clone()),
        Some(other) => Err(Error::Protocol(format!("工具参数形态无法识别: {other}"))),
    }
}

fn sse_data(body: &str) -> Vec<&str> {
    body.lines()
        .filter_map(|line| {
            let line = line.trim_end_matches('\r');
            line.strip_prefix("data:").map(str::trim)
        })
        .filter(|data| !data.is_empty())
        .collect()
}

fn truncate(text: &str) -> String {
    let mut out: String = text.chars().take(512).collect();
    if text.chars().count() > 512 {
        out.push('…');
    }
    out
}

/// 供测试观察副作用分类没有被适配器改写。
pub(crate) fn effect_label(effect: EffectClass) -> &'static str {
    match effect {
        EffectClass::Pure => "pure",
        EffectClass::Persist => "persist",
        EffectClass::ExternalOnce => "external_once",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::UreqClient;
    use crate::spec::{decode_spec, EffectClass};
    use crate::stub::{StubResponse, StubServer};

    fn key() -> EndpointKey {
        EndpointKey::new("sk-test-secret")
    }

    #[test]
    fn key_debug_and_display_hide_the_secret() {
        let key = key();
        let debug = format!("{key:?}");
        let display = format!("{key}");
        assert!(!debug.contains("sk-test-secret"));
        assert!(!display.contains("sk-test-secret"));
        assert!(debug.contains("***"));
    }

    #[test]
    fn openai_json_and_sse_parse_tool_calls() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"看一眼","tool_calls":[{"id":"call_1","type":"function","function":{"name":"git","arguments":"{\"args\":\"status\"}"}}]},"finish_reason":"tool_calls"}]}"#;
        let message = parse_json(Protocol::OpenaiChatCompletions, body).unwrap();
        assert_eq!(message.text, "看一眼");
        assert_eq!(message.tool_calls[0].name, "git");
        assert_eq!(message.tool_calls[0].arguments["args"], "status");

        let chunk1 = json!({"choices":[{"delta":{"content":"he"}}]}).to_string();
        let chunk2 = json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_9",
                        "function": {
                            "name": "git",
                            "arguments": "{\"args\":\"status\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        })
        .to_string();
        let sse = format!("data: {chunk1}\n\ndata: {chunk2}\n\ndata: [DONE]\n");
        let streamed = parse_stream(Protocol::OpenaiChatCompletions, &sse).unwrap();
        assert_eq!(streamed.text, "he");
        assert_eq!(streamed.tool_calls[0].id, "call_9");
        assert_eq!(streamed.tool_calls[0].arguments["args"], "status");
        assert_eq!(streamed.finish, "tool_calls");
    }

    #[test]
    fn anthropic_json_and_sse_parse_tool_use() {
        let body = r#"{"stop_reason":"tool_use","content":[{"type":"text","text":"付款"},{"type":"tool_use","id":"toolu_1","name":"pay","input":{"cents":1}}]}"#;
        let message = parse_json(Protocol::AnthropicMessages, body).unwrap();
        assert_eq!(message.text, "付款");
        assert_eq!(message.tool_calls[0].name, "pay");
        assert_eq!(message.tool_calls[0].arguments["cents"], 1);

        let sse = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_2\",\"name\":\"pay\",\"input\":{}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"cents\\\":1}\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\n",
        );
        let streamed = parse_stream(Protocol::AnthropicMessages, sse).unwrap();
        assert_eq!(streamed.text, "hi");
        assert_eq!(streamed.tool_calls.len(), 1);
        assert_eq!(streamed.tool_calls[0].name, "pay");
        assert_eq!(streamed.tool_calls[0].arguments["cents"], 1);
        assert_eq!(streamed.finish, "tool_use");
    }

    #[test]
    fn stub_sees_openai_and_anthropic_shapes_without_logging_the_key() {
        let stub = StubServer::spawn().unwrap();
        let client = UreqClient::new();
        let mut spec = decode_spec(crate::EXAMPLE_SPEC_JSON).unwrap();
        spec.endpoint.base_url = stub.url();
        spec.endpoint.reasoning_tier = ReasoningTier::Low;
        let transcript = vec![
            TranscriptItem::System { text: "sys".into() },
            TranscriptItem::User { text: "go".into() },
        ];
        stub.push(StubResponse::json(
            200,
            r#"{"choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#,
        ));
        let message =
            complete_once(&client, &spec.endpoint, &key(), &transcript, &spec.toolset).unwrap();
        assert_eq!(message.text, "ok");
        let recorded = stub.requests();
        assert_eq!(recorded[0].path, "/chat/completions");
        let body: Value = serde_json::from_str(&recorded[0].body).unwrap();
        assert_eq!(body["model"], "kimi-k2");
        assert_eq!(body["reasoning_effort"], "low");
        assert_eq!(body["stream"], false);
        assert_eq!(body["tools"][0]["function"]["name"], "git");
        assert_eq!(body["tools"][1]["function"]["name"], "pay");
        assert!(recorded[0]
            .headers
            .iter()
            .any(|(name, value)| { name == "authorization" && value == "Bearer sk-test-secret" }));
        let debug = format!("{message:?}");
        assert!(!debug.contains("sk-test-secret"));
    }

    #[test]
    fn stub_anthropic_request_carries_version_and_thinking() {
        let stub = StubServer::spawn().unwrap();
        let client = UreqClient::new();
        let mut spec = decode_spec(crate::EXAMPLE_SPEC_JSON).unwrap();
        spec.endpoint.protocol = Protocol::AnthropicMessages;
        spec.endpoint.base_url = stub.url();
        spec.endpoint.reasoning_tier = ReasoningTier::Medium;
        spec.endpoint.model = "claude-sonnet".into();
        stub.push(StubResponse::json(
            200,
            r#"{"stop_reason":"end_turn","content":[{"type":"text","text":"好"}]}"#,
        ));
        let transcript = vec![TranscriptItem::User { text: "go".into() }];
        let message = complete_once(&client, &spec.endpoint, &key(), &transcript, &[]).unwrap();
        assert_eq!(message.text, "好");
        let recorded = &stub.requests()[0];
        assert_eq!(recorded.path, "/v1/messages");
        assert!(recorded
            .headers
            .iter()
            .any(|(name, value)| { name == "x-api-key" && value == "sk-test-secret" }));
        assert!(recorded
            .headers
            .iter()
            .any(|(name, value)| name == "anthropic-version" && value == ANTHROPIC_VERSION));
        let body: Value = serde_json::from_str(&recorded.body).unwrap();
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(
            body["thinking"]["budget_tokens"].as_u64(),
            Some(u64::from(THINKING_MEDIUM))
        );
        assert!(body.get("tools").is_none());
        assert_eq!(effect_label(EffectClass::Persist), "persist");
    }

    #[test]
    fn endpoint_error_redacts_the_secret_if_the_body_echoes_it() {
        let stub = StubServer::spawn().unwrap();
        let client = UreqClient::new();
        let mut spec = decode_spec(crate::EXAMPLE_SPEC_JSON).unwrap();
        spec.endpoint.base_url = stub.url();
        stub.push(StubResponse::json(401, "bad key sk-test-secret"));
        let err = complete_once(
            &client,
            &spec.endpoint,
            &key(),
            &[TranscriptItem::User { text: "go".into() }],
            &[],
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(!text.contains("sk-test-secret"), "{text}");
        assert!(matches!(err, Error::Endpoint { status: 401, .. }));
    }
}

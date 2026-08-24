use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use thiserror::Error;

use crate::agent::tools::ToolSpec;
use crate::config::{Config, Provider};

const FALLBACK_TAG: &str = "If you cannot call tools natively, reply with one or more tags of the form:\n\
<tool name=\"TOOL_NAME\">{...json arguments...}</tool>\n\
Do not wrap them in markdown. After tools run you will get results. When finished, reply in plain language with no <tool> tags.";

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        id: String,
        name: String,
        content: String,
    },
}

#[derive(Debug, Clone)]
pub struct Completion {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl LlmError {
    fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

pub async fn complete(
    config: &Config,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    allow_native_tools: bool,
) -> Result<Completion, LlmError> {
    match config.provider {
        Provider::Anthropic => anthropic(config, messages, tools).await,
        _ => openai_compatible(config, messages, tools, allow_native_tools).await,
    }
}

async fn openai_compatible(
    config: &Config,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    allow_native_tools: bool,
) -> Result<Completion, LlmError> {
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let mut body = json!({
        "model": config.model,
        "messages": openai_messages(messages),
        "temperature": 0.2,
    });
    if allow_native_tools && !tools.is_empty() {
        body["tools"] = openai_tools(tools);
        body["tool_choice"] = json!("auto");
    }

    let response = client()
        .post(url)
        .headers(openai_headers(config)?)
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    let payload: Value = response.json().await?;
    if !status.is_success() {
        let message = error_message(&payload, status.as_u16());
        if allow_native_tools && looks_like_missing_tools(&message) {
            return Box::pin(openai_compatible(config, messages, tools, false)).await;
        }
        return Err(provider_error(config, message));
    }
    parse_openai(&payload)
}

async fn anthropic(
    config: &Config,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
) -> Result<Completion, LlmError> {
    let url = format!("{}/v1/messages", config.base_url.trim_end_matches('/'));
    let (system, chat) = anthropic_messages(messages);
    let mut body = json!({
        "model": config.model,
        "max_tokens": 8192,
        "messages": chat,
        "temperature": 0.2,
    });
    if let Some(system) = system {
        body["system"] = json!(system);
    }
    if !tools.is_empty() {
        body["tools"] = anthropic_tools(tools);
    }
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    if !config.api_key.is_empty() {
        headers.insert(
            "x-api-key",
            header_value(&config.api_key, "API key")?,
        );
    }
    let response = client().post(url).headers(headers).json(&body).send().await?;
    let status = response.status();
    let payload: Value = response.json().await?;
    if !status.is_success() {
        return Err(provider_error(config, error_message(&payload, status.as_u16())));
    }
    parse_anthropic(&payload)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .expect("reqwest client")
}

fn openai_headers(config: &Config) -> Result<HeaderMap, LlmError> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if !config.api_key.is_empty() {
        let value = format!("Bearer {}", config.api_key.trim());
        headers.insert(AUTHORIZATION, header_value(&value, "API key")?);
    }
    if config.provider == Provider::OpenRouter {
        headers.insert(
            "HTTP-Referer",
            HeaderValue::from_static("https://github.com/commando"),
        );
        headers.insert("X-Title", HeaderValue::from_static("Commando"));
    }
    Ok(headers)
}

fn header_value(value: &str, label: &str) -> Result<HeaderValue, LlmError> {
    HeaderValue::from_str(value).map_err(|_| LlmError::msg(format!("Invalid {label}")))
}

fn openai_messages(messages: &[ChatMessage]) -> Value {
    let rows: Vec<Value> = messages
        .iter()
        .map(|message| match message {
            ChatMessage::System(text) => json!({"role": "system", "content": text}),
            ChatMessage::User(text) => json!({"role": "user", "content": text}),
            ChatMessage::Assistant { text, tool_calls } if tool_calls.is_empty() => {
                json!({"role": "assistant", "content": text})
            }
            ChatMessage::Assistant { text, tool_calls } => {
                let calls: Vec<Value> = tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": arguments_string(&call.arguments)
                            }
                        })
                    })
                    .collect();
                let content = if text.is_empty() {
                    Value::Null
                } else {
                    json!(text)
                };
                json!({"role": "assistant", "content": content, "tool_calls": calls})
            }
            ChatMessage::Tool { id, name, content } => json!({
                "role": "tool",
                "tool_call_id": id,
                "name": name,
                "content": content
            }),
        })
        .collect();
    Value::Array(rows)
}

fn openai_tools(tools: &[ToolSpec]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters
                    }
                })
            })
            .collect(),
    )
}

fn anthropic_messages(messages: &[ChatMessage]) -> (Option<String>, Value) {
    let mut system = None;
    let mut rows = Vec::new();
    for message in messages {
        match message {
            ChatMessage::System(text) => {
                system = Some(if let Some(existing) = system {
                    format!("{existing}\n\n{text}")
                } else {
                    text.clone()
                });
            }
            ChatMessage::User(text) => rows.push(json!({
                "role": "user",
                "content": [{"type": "text", "text": text}]
            })),
            ChatMessage::Assistant { text, tool_calls } => {
                let mut content = Vec::new();
                if !text.is_empty() {
                    content.push(json!({"type": "text", "text": text}));
                }
                for call in tool_calls {
                    content.push(json!({
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.name,
                        "input": call.arguments
                    }));
                }
                rows.push(json!({"role": "assistant", "content": content}));
            }
            ChatMessage::Tool { id, content, .. } => {
                if let Some(Value::String(role)) = rows.last().map(|row| &row["role"]) {
                    if role == "user" {
                        if let Some(Value::Array(content_arr)) =
                            rows.last_mut().and_then(|row| row.get_mut("content"))
                        {
                            content_arr.push(json!({
                                "type": "tool_result",
                                "tool_use_id": id,
                                "content": content
                            }));
                            continue;
                        }
                    }
                }
                rows.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": content
                    }]
                }));
            }
        }
    }
    (system, Value::Array(rows))
}

fn anthropic_tools(tools: &[ToolSpec]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters
                })
            })
            .collect(),
    )
}

fn parse_openai(payload: &Value) -> Result<Completion, LlmError> {
    let choice = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| LlmError::msg("Model returned no choices"))?;
    let message = &choice["message"];
    let text = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut tool_calls = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            let name = call["function"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let arguments = parse_arguments(&call["function"]["arguments"]);
            tool_calls.push(ToolCall {
                id: call["id"]
                    .as_str()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("call_{index}")),
                name,
                arguments,
            });
        }
    }
    if tool_calls.is_empty() {
        tool_calls = parse_fallback_tools(&text);
    }
    let text = if tool_calls.is_empty() {
        text
    } else {
        strip_tool_tags(&text)
    };
    Ok(Completion { text, tool_calls })
}

fn parse_anthropic(payload: &Value) -> Result<Completion, LlmError> {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    if let Some(blocks) = payload.get("content").and_then(Value::as_array) {
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(piece) = block.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(piece);
                    }
                }
                Some("tool_use") => {
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    tool_calls.push(ToolCall {
                        id: block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string(),
                        name,
                        arguments: block.get("input").cloned().unwrap_or_else(|| json!({})),
                    });
                }
                _ => {}
            }
        }
    }
    if tool_calls.is_empty() {
        tool_calls = parse_fallback_tools(&text);
    }
    Ok(Completion { text, tool_calls })
}

fn parse_arguments(value: &Value) -> Value {
    match value {
        Value::String(raw) => serde_json::from_str(raw).unwrap_or_else(|_| json!({})),
        other => other.clone(),
    }
}

pub fn parse_fallback_tools(text: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut rest = text;
    let mut index = 0;
    while let Some(start) = rest.find("<tool") {
        let after = &rest[start..];
        let Some(name_at) = after.find("name=\"") else {
            break;
        };
        let name_start = name_at + 6;
        let name_src = &after[name_start..];
        let Some(name_end) = name_src.find('"') else {
            break;
        };
        let name = name_src[..name_end].to_string();
        let Some(body_at) = after.find('>') else {
            break;
        };
        let body = &after[body_at + 1..];
        let Some(end) = body.find("</tool>") else {
            break;
        };
        let raw_args = body[..end].trim();
        let arguments = serde_json::from_str(raw_args).unwrap_or_else(|_| json!({}));
        calls.push(ToolCall {
            id: format!("fallback_{index}"),
            name,
            arguments,
        });
        index += 1;
        rest = &body[end + 7..];
    }
    calls
}

fn strip_tool_tags(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<tool") {
        out.push_str(rest[..start].trim());
        if let Some(end) = rest[start..].find("</tool>") {
            rest = rest[start + end + 7..].trim_start();
        } else {
            rest = "";
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str(rest.trim());
    out.trim().to_string()
}

fn arguments_string(value: &Value) -> String {
    if value.is_string() {
        value.as_str().unwrap_or_default().to_string()
    } else {
        value.to_string()
    }
}

fn error_message(payload: &Value, status: u16) -> String {
    payload
        .pointer("/error/message")
        .or_else(|| payload.pointer("/error"))
        .and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            other => Some(other.to_string()),
        })
        .unwrap_or_else(|| format!("HTTP {status}"))
}

fn looks_like_missing_tools(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("tool")
        && (lower.contains("not support")
            || lower.contains("unknown")
            || lower.contains("unexpected")
            || lower.contains("does not have"))
}

fn provider_error(config: &Config, message: String) -> LlmError {
    if config.provider == Provider::Ollama
        && (message.contains("error sending request")
            || message.contains("Connection refused")
            || message.contains("ConnectError"))
    {
        return LlmError::msg(format!(
            "Ollama is not reachable at {}. Start it with `ollama serve` or pick another provider in Settings.",
            config.base_url
        ));
    }
    LlmError::msg(message)
}

pub fn with_fallback_instructions(system: &str) -> String {
    format!("{system}\n\n{FALLBACK_TAG}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fallback_tool_tags() {
        let text = "Working\n<tool name=\"list_directory\">{\"path\": \"~/Downloads\"}</tool>";
        let calls = parse_fallback_tools(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_directory");
        assert_eq!(calls[0].arguments["path"], "~/Downloads");
    }

    #[test]
    fn parses_openai_tool_payload() {
        let payload = json!({
            "choices": [{
                "message": {
                    "content": "",
                    "tool_calls": [{
                        "id": "1",
                        "function": {
                            "name": "run_command",
                            "arguments": "{\"command\":\"ls\"}"
                        }
                    }]
                }
            }]
        });
        let completion = parse_openai(&payload).unwrap();
        assert_eq!(completion.tool_calls[0].name, "run_command");
        assert_eq!(completion.tool_calls[0].arguments["command"], "ls");
    }
}

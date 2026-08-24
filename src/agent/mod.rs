pub mod library;
pub mod llm;
pub mod safety;
pub mod tools;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agent::llm::{with_fallback_instructions, ChatMessage};
use crate::agent::tools::ToolOutcome;
use crate::config::{display_path, Config};

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Status(String),
    ToolStart { name: String, summary: String },
    ToolResult { ok: bool, summary: String, detail: String },
    Assistant(String),
    Done { summary: String, elapsed: Duration },
    Failed(String),
}

pub struct AgentRequest {
    pub prompt: String,
    pub image: Option<Vec<u8>>,
    pub knowledge: Vec<String>,
    pub config: Config,
    pub workspace: PathBuf,
    pub history: Vec<ChatMessage>,
    pub stop: Arc<AtomicBool>,
}

pub async fn run(request: AgentRequest, emit: impl Fn(AgentEvent)) {
    let started = Instant::now();
    let tools = tools::catalog();
    let mut messages = request.history;
    if messages.is_empty() {
        messages.push(ChatMessage::System(with_fallback_instructions(
            &library::system_prompt(&display_path(&request.workspace), &request.knowledge),
        )));
    }
    messages.push(match request.image {
        Some(png) => ChatMessage::UserImage {
            text: request.prompt,
            png,
        },
        None => ChatMessage::User(request.prompt),
    });

    let timeout = Duration::from_secs(request.config.timeout_secs.max(5));
    let max_iterations = request.config.max_iterations.max(1);

    for iteration in 0..max_iterations {
        if request.stop.load(Ordering::Relaxed) {
            emit(AgentEvent::Done {
                summary: "Stopped".into(),
                elapsed: started.elapsed(),
            });
            return;
        }
        emit(AgentEvent::Status(if iteration == 0 {
            "Thinking…".into()
        } else {
            format!("Planning next step ({})…", iteration + 1)
        }));

        let completion = match llm::complete(&request.config, &messages, &tools, true).await {
            Ok(completion) => completion,
            Err(error) => {
                emit(AgentEvent::Failed(error.to_string()));
                return;
            }
        };

        if completion.tool_calls.is_empty() {
            let text = completion.text.trim();
            if !text.is_empty() {
                emit(AgentEvent::Assistant(text.to_string()));
            }
            emit(AgentEvent::Done {
                summary: if text.is_empty() {
                    "Done".into()
                } else {
                    first_line(text)
                },
                elapsed: started.elapsed(),
            });
            return;
        }

        messages.push(ChatMessage::Assistant {
            text: completion.text.clone(),
            tool_calls: completion.tool_calls.clone(),
        });

        for call in completion.tool_calls {
            if request.stop.load(Ordering::Relaxed) {
                emit(AgentEvent::Done {
                    summary: "Stopped".into(),
                    elapsed: started.elapsed(),
                });
                return;
            }
            emit(AgentEvent::ToolStart {
                name: call.name.clone(),
                summary: tool_preview(&call.name, &call.arguments),
            });
            let outcome =
                tools::execute(&call.name, &call.arguments, &request.workspace, timeout).await;
            emit(AgentEvent::ToolResult {
                ok: outcome.ok,
                summary: outcome.summary.clone(),
                detail: outcome.detail.clone(),
            });
            messages.push(ChatMessage::Tool {
                id: call.id,
                name: call.name,
                content: format_tool_content(&outcome),
            });
        }
    }

    emit(AgentEvent::Failed(format!(
        "Stopped after {max_iterations} tool steps. Ask again to continue."
    )));
}

fn format_tool_content(outcome: &ToolOutcome) -> String {
    if outcome.ok {
        outcome.detail.clone()
    } else {
        format!("ERROR: {}\n{}", outcome.summary, outcome.detail)
    }
}

fn tool_preview(name: &str, args: &serde_json::Value) -> String {
    let compact = match name {
        "run_command" => args
            .get("command")
            .and_then(|value| value.as_str())
            .map(|value| format!("`{value}`"))
            .unwrap_or_else(|| args.to_string()),
        "start_process" => args
            .get("command")
            .and_then(|value| value.as_str())
            .map(|value| format!("`{value}`"))
            .unwrap_or_else(|| args.to_string()),
        "interact_with_process" | "kill_process" => format!(
            "session {}",
            args.get("session_id").and_then(|value| value.as_u64()).map(|id| id.to_string()).unwrap_or_else(|| "?".into())
        ),
        "list_processes" => String::new(),
        "read_file" | "write_file" | "edit_file" | "file_info" | "delete_path"
        | "create_directory" | "list_directory" => args
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("workspace")
            .to_string(),
        "move_path" => format!(
            "{} → {}",
            args.get("from").and_then(|value| value.as_str()).unwrap_or("?"),
            args.get("to").and_then(|value| value.as_str()).unwrap_or("?")
        ),
        "search_files" => args
            .get("query")
            .or_else(|| args.get("glob"))
            .and_then(|value| value.as_str())
            .unwrap_or("workspace")
            .to_string(),
        _ => args.to_string(),
    };
    format!("{name} {compact}")
}

fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or(text)
        .chars()
        .take(120)
        .collect()
}

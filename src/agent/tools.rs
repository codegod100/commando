use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use globset::GlobBuilder;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;
use walkdir::WalkDir;

use crate::agent::safety;
use crate::config::expand_user_path;

const MAX_TEXT_BYTES: u64 = 256 * 1024;
const MAX_SEARCH_HITS: usize = 80;
const MAX_LIST_ENTRIES: usize = 400;
const MAX_COMMAND_CHARS: usize = 24_000;
const SESSION_READ_DELAY_MS: u64 = 150;

struct ProcessSession {
    command: String,
    child: Child,
    stdin: Option<ChildStdin>,
    output: Arc<Mutex<Vec<u8>>>,
    read_offset: usize,
}

#[derive(Default)]
struct ProcessManager {
    next_id: u64,
    sessions: HashMap<u64, ProcessSession>,
}

fn process_manager() -> &'static Mutex<ProcessManager> {
    static MANAGER: OnceLock<Mutex<ProcessManager>> = OnceLock::new();
    MANAGER.get_or_init(|| Mutex::new(ProcessManager::default()))
}

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

pub fn catalog() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "list_directory",
            description: "List files and folders. Directories first. Use depth > 1 for a shallow tree.",
            parameters: object_schema(
                &[(
                    "path",
                    json!({"type": "string", "description": "Directory to list. Defaults to the workspace."}),
                    false,
                ),
                (
                    "depth",
                    json!({"type": "integer", "description": "How many levels to recurse. Default 1, max 4."}),
                    false,
                )],
            ),
        },
        ToolSpec {
            name: "read_file",
            description: "Read a text file. Use offset/length for large files. Negative offset reads from the end.",
            parameters: object_schema(
                &[(
                    "path",
                    json!({"type": "string", "description": "File path"}),
                    true,
                ),
                (
                    "offset",
                    json!({"type": "integer", "description": "Line offset. Negative counts from the end."}),
                    false,
                ),
                (
                    "length",
                    json!({"type": "integer", "description": "Maximum number of lines to return."}),
                    false,
                )],
            ),
        },
        ToolSpec {
            name: "write_file",
            description: "Create or overwrite a text file with the given contents.",
            parameters: object_schema(
                &[(
                    "path",
                    json!({"type": "string"}),
                    true,
                ),
                (
                    "content",
                    json!({"type": "string"}),
                    true,
                )],
            ),
        },
        ToolSpec {
            name: "edit_file",
            description: "Replace one exact occurrence of old_string with new_string in a file.",
            parameters: object_schema(
                &[(
                    "path",
                    json!({"type": "string"}),
                    true,
                ),
                (
                    "old_string",
                    json!({"type": "string"}),
                    true,
                ),
                (
                    "new_string",
                    json!({"type": "string"}),
                    true,
                )],
            ),
        },
        ToolSpec {
            name: "move_path",
            description: "Move or rename a file or directory. Creates the destination parent if needed.",
            parameters: object_schema(
                &[(
                    "from",
                    json!({"type": "string"}),
                    true,
                ),
                (
                    "to",
                    json!({"type": "string"}),
                    true,
                )],
            ),
        },
        ToolSpec {
            name: "create_directory",
            description: "Create a directory, including parents.",
            parameters: object_schema(&[("path", json!({"type": "string"}), true)]),
        },
        ToolSpec {
            name: "delete_path",
            description: "Delete a file or an empty directory. Set recursive=true to delete a folder tree.",
            parameters: object_schema(
                &[(
                    "path",
                    json!({"type": "string"}),
                    true,
                ),
                (
                    "recursive",
                    json!({"type": "boolean"}),
                    false,
                )],
            ),
        },
        ToolSpec {
            name: "search_files",
            description: "Find files by glob and optionally search file contents (case-insensitive).",
            parameters: object_schema(
                &[(
                    "path",
                    json!({"type": "string", "description": "Root directory. Defaults to the workspace."}),
                    false,
                ),
                (
                    "glob",
                    json!({"type": "string", "description": "Filename glob such as *.pdf or **/*.rs"}),
                    false,
                ),
                (
                    "query",
                    json!({"type": "string", "description": "Optional text to search for inside files."}),
                    false,
                )],
            ),
        },
        ToolSpec {
            name: "file_info",
            description: "Get size, type, and modified time for a path.",
            parameters: object_schema(&[("path", json!({"type": "string"}), true)]),
        },
        ToolSpec {
            name: "run_command",
            description: "Run a shell command in bash -lc. Prefer this for conversions, sorting, git, and package tools.",
            parameters: object_schema(
                &[(
                    "command",
                    json!({"type": "string"}),
                    true,
                ),
                (
                    "cwd",
                    json!({"type": "string", "description": "Working directory. Defaults to the workspace."}),
                    false,
                )],
            ),
        },
        ToolSpec {
            name: "start_process",
            description: "Start a persistent shell process and return a session ID. Use for servers, REPLs, debuggers, and commands that need later input.",
            parameters: object_schema(
                &[(
                    "command",
                    json!({"type": "string"}),
                    true,
                ),
                (
                    "cwd",
                    json!({"type": "string", "description": "Working directory. Defaults to the workspace."}),
                    false,
                )],
            ),
        },
        ToolSpec {
            name: "interact_with_process",
            description: "Send input to a persistent process and read output produced since the previous interaction. Omit input to poll output.",
            parameters: object_schema(
                &[(
                    "session_id",
                    json!({"type": "integer"}),
                    true,
                ),
                (
                    "input",
                    json!({"type": "string", "description": "Text sent to stdin. A newline is appended unless input already ends with one."}),
                    false,
                ),
                (
                    "wait_ms",
                    json!({"type": "integer", "description": "Time to wait for output after sending input. Default 150, max 5000."}),
                    false,
                )],
            ),
        },
        ToolSpec {
            name: "list_processes",
            description: "List persistent process sessions started by this app and whether each is running or exited.",
            parameters: object_schema(&[]),
        },
        ToolSpec {
            name: "kill_process",
            description: "Terminate and remove a persistent process session.",
            parameters: object_schema(&[(
                "session_id",
                json!({"type": "integer"}),
                true,
            )]),
        },
    ]
}

fn object_schema(fields: &[(&str, Value, bool)]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, schema, is_required) in fields {
        properties.insert((*name).to_string(), schema.clone());
        if *is_required {
            required.push(Value::String((*name).to_string()));
        }
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub ok: bool,
    pub summary: String,
    pub detail: String,
}

impl ToolOutcome {
    fn ok(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            ok: true,
            summary: summary.into(),
            detail: detail.into(),
        }
    }

    fn err(summary: impl Into<String>) -> Self {
        let summary = summary.into();
        Self {
            ok: false,
            detail: summary.clone(),
            summary,
        }
    }
}

pub async fn execute(name: &str, args: &Value, workspace: &Path, timeout: Duration) -> ToolOutcome {
    match name {
        "list_directory" => list_directory(arg_path(args, "path", workspace), arg_u32(args, "depth", 1)),
        "read_file" => read_file(
            &required_path(args, "path", workspace),
            arg_i64(args, "offset"),
            arg_u32(args, "length", 0),
        ),
        "write_file" => write_file(
            &required_path(args, "path", workspace),
            args.get("content").and_then(Value::as_str).unwrap_or_default(),
        ),
        "edit_file" => edit_file(
            &required_path(args, "path", workspace),
            args.get("old_string").and_then(Value::as_str).unwrap_or_default(),
            args.get("new_string").and_then(Value::as_str).unwrap_or_default(),
        ),
        "move_path" => move_path(
            &required_path(args, "from", workspace),
            &required_path(args, "to", workspace),
        ),
        "create_directory" => create_directory(&required_path(args, "path", workspace)),
        "delete_path" => delete_path(
            &required_path(args, "path", workspace),
            args.get("recursive").and_then(Value::as_bool).unwrap_or(false),
        ),
        "search_files" => search_files(
            &arg_path(args, "path", workspace),
            args.get("glob").and_then(Value::as_str),
            args.get("query").and_then(Value::as_str),
        ),
        "file_info" => file_info(&required_path(args, "path", workspace)),
        "run_command" => {
            let command = args.get("command").and_then(Value::as_str).unwrap_or("").trim();
            if command.is_empty() {
                return ToolOutcome::err("run_command needs a command");
            }
            if let Some(reason) = safety::blocked_command(command) {
                return ToolOutcome::err(format!("Blocked command matching `{reason}`"));
            }
            let cwd = arg_path(args, "cwd", workspace);
            run_command(command, &cwd, timeout).await
        }
        "start_process" => {
            let command = args.get("command").and_then(Value::as_str).unwrap_or("").trim();
            if command.is_empty() {
                return ToolOutcome::err("start_process needs a command");
            }
            if let Some(reason) = safety::blocked_command(command) {
                return ToolOutcome::err(format!("Blocked command matching `{reason}`"));
            }
            start_process(command, &arg_path(args, "cwd", workspace)).await
        }
        "interact_with_process" => interact_with_process(
            args.get("session_id").and_then(Value::as_u64),
            args.get("input").and_then(Value::as_str),
            args.get("wait_ms").and_then(Value::as_u64),
        )
        .await,
        "list_processes" => list_processes().await,
        "kill_process" => kill_process(args.get("session_id").and_then(Value::as_u64)).await,
        other => ToolOutcome::err(format!("Unknown tool `{other}`")),
    }
}

fn arg_path(args: &Value, key: &str, workspace: &Path) -> PathBuf {
    args.get(key)
        .and_then(Value::as_str)
        .map(resolve_path)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| workspace.to_path_buf())
}

fn required_path(args: &Value, key: &str, workspace: &Path) -> PathBuf {
    match args.get(key).and_then(Value::as_str) {
        Some(value) if !value.trim().is_empty() => resolve_path(value),
        _ => workspace.to_path_buf(),
    }
}

fn resolve_path(value: &str) -> PathBuf {
    expand_user_path(value)
}

fn arg_u32(args: &Value, key: &str, default: u32) -> u32 {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or(default)
}

fn arg_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

fn list_directory(path: PathBuf, depth: u32) -> ToolOutcome {
    let depth = depth.clamp(1, 4) as usize;
    if !path.exists() {
        return ToolOutcome::err(format!("Directory not found: {}", path.display()));
    }
    let mut lines = Vec::new();
    for entry in WalkDir::new(&path)
        .min_depth(1)
        .max_depth(depth)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .take(MAX_LIST_ENTRIES)
    {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let rel = entry
            .path()
            .strip_prefix(&path)
            .unwrap_or(entry.path())
            .display();
        if meta.is_dir() {
            lines.push(format!("{rel}/"));
        } else {
            lines.push(format!("{rel}\t{}", format_size(meta.len())));
        }
    }
    lines.sort();
    let count = lines.len();
    let body = if lines.is_empty() {
        "(empty)".to_string()
    } else {
        lines.join("\n")
    };
    ToolOutcome::ok(
        format!("Listed {count} items in {}", path.display()),
        body,
    )
}

fn read_file(path: &Path, offset: Option<i64>, length: u32) -> ToolOutcome {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) => return ToolOutcome::err(format!("Cannot read {}: {error}", path.display())),
    };
    if meta.is_dir() {
        return ToolOutcome::err(format!("{} is a directory", path.display()));
    }
    if meta.len() > MAX_TEXT_BYTES && offset.is_none() && length == 0 {
        return ToolOutcome::err(format!(
            "{} is {} — pass offset/length to read a slice",
            path.display(),
            format_size(meta.len())
        ));
    }
    let contents = match read_text_lossy(path) {
        Ok(contents) => contents,
        Err(error) => return ToolOutcome::err(format!("Cannot read {}: {error}", path.display())),
    };
    let lines: Vec<&str> = contents.lines().collect();
    let (start, end) = slice_lines(lines.len(), offset, length);
    let snippet = lines[start..end].join("\n");
    ToolOutcome::ok(
        format!(
            "Read {} ({}-{} of {} lines)",
            path.display(),
            start + 1,
            end,
            lines.len()
        ),
        snippet,
    )
}

fn slice_lines(total: usize, offset: Option<i64>, length: u32) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let start = match offset {
        Some(value) if value < 0 => total.saturating_sub(value.unsigned_abs() as usize),
        Some(value) => (value as usize).min(total),
        None => 0,
    };
    let end = if length == 0 {
        total
    } else {
        start.saturating_add(length as usize).min(total)
    };
    (start, end)
}

fn write_file(path: &Path, content: &str) -> ToolOutcome {
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return ToolOutcome::err(format!("Cannot create {}: {error}", parent.display()));
        }
    }
    match fs::write(path, content) {
        Ok(()) => ToolOutcome::ok(
            format!("Wrote {} ({} bytes)", path.display(), content.len()),
            format!("Wrote {}", path.display()),
        ),
        Err(error) => ToolOutcome::err(format!("Cannot write {}: {error}", path.display())),
    }
}

fn edit_file(path: &Path, old: &str, new: &str) -> ToolOutcome {
    if old.is_empty() {
        return ToolOutcome::err("old_string must not be empty");
    }
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => return ToolOutcome::err(format!("Cannot read {}: {error}", path.display())),
    };
    let matches = contents.matches(old).count();
    if matches == 0 {
        return ToolOutcome::err(format!("old_string not found in {}", path.display()));
    }
    if matches > 1 {
        return ToolOutcome::err(format!(
            "old_string matched {matches} times in {} — make it unique",
            path.display()
        ));
    }
    match fs::write(path, contents.replacen(old, new, 1)) {
        Ok(()) => ToolOutcome::ok(format!("Edited {}", path.display()), format!("Updated {}", path.display())),
        Err(error) => ToolOutcome::err(format!("Cannot write {}: {error}", path.display())),
    }
}

fn move_path(from: &Path, to: &Path) -> ToolOutcome {
    if let Some(parent) = to.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return ToolOutcome::err(format!("Cannot create {}: {error}", parent.display()));
        }
    }
    match fs::rename(from, to) {
        Ok(()) => ToolOutcome::ok(
            format!("Moved {} → {}", from.display(), to.display()),
            format!("Moved {} to {}", from.display(), to.display()),
        ),
        Err(error) => ToolOutcome::err(format!(
            "Cannot move {} to {}: {error}",
            from.display(),
            to.display()
        )),
    }
}

fn create_directory(path: &Path) -> ToolOutcome {
    match fs::create_dir_all(path) {
        Ok(()) => ToolOutcome::ok(
            format!("Created {}", path.display()),
            format!("Created directory {}", path.display()),
        ),
        Err(error) => ToolOutcome::err(format!("Cannot create {}: {error}", path.display())),
    }
}

fn delete_path(path: &Path, recursive: bool) -> ToolOutcome {
    let result = if path.is_dir() {
        if recursive {
            fs::remove_dir_all(path)
        } else {
            fs::remove_dir(path)
        }
    } else {
        fs::remove_file(path)
    };
    match result {
        Ok(()) => ToolOutcome::ok(format!("Deleted {}", path.display()), format!("Deleted {}", path.display())),
        Err(error) => ToolOutcome::err(format!("Cannot delete {}: {error}", path.display())),
    }
}

fn search_files(root: &Path, glob: Option<&str>, query: Option<&str>) -> ToolOutcome {
    if !root.exists() {
        return ToolOutcome::err(format!("Search root not found: {}", root.display()));
    }
    let matcher = match glob.filter(|value| !value.is_empty()) {
        Some(pattern) => match compile_glob(pattern) {
            Ok(glob) => Some(glob),
            Err(error) => return ToolOutcome::err(error),
        },
        None => None,
    };
    let query = query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let mut hits = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(glob) = &matcher {
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .to_string_lossy();
            if !glob.is_match(rel.as_ref()) && !glob.is_match(entry.file_name().to_string_lossy().as_ref()) {
                continue;
            }
        }
        if let Some(query) = &query {
            let Ok(contents) = fs::read_to_string(entry.path()) else {
                continue;
            };
            if !contents.to_ascii_lowercase().contains(query) {
                continue;
            }
        }
        hits.push(entry.path().display().to_string());
        if hits.len() >= MAX_SEARCH_HITS {
            break;
        }
    }
    let count = hits.len();
    let body = if hits.is_empty() {
        "No matches".to_string()
    } else {
        hits.join("\n")
    };
    ToolOutcome::ok(format!("Found {count} matching files"), body)
}

fn compile_glob(pattern: &str) -> Result<globset::GlobMatcher, String> {
    GlobBuilder::new(pattern)
        .case_insensitive(true)
        .literal_separator(false)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|error| format!("Invalid glob: {error}"))
}

fn file_info(path: &Path) -> ToolOutcome {
    match fs::metadata(path) {
        Ok(meta) => {
            let kind = if meta.is_dir() {
                "directory"
            } else if meta.is_file() {
                "file"
            } else {
                "other"
            };
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| {
                    chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_else(|| "unknown".into());
            ToolOutcome::ok(
                format!("{} ({kind})", path.display()),
                format!(
                    "path: {}\nkind: {kind}\nsize: {}\nmodified: {modified}",
                    path.display(),
                    format_size(meta.len())
                ),
            )
        }
        Err(error) => ToolOutcome::err(format!("Cannot stat {}: {error}", path.display())),
    }
}

async fn run_command(command: &str, cwd: &Path, timeout: Duration) -> ToolOutcome {
    if !cwd.exists() {
        return ToolOutcome::err(format!("Working directory not found: {}", cwd.display()));
    }
    let child = Command::new("bash")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(timeout, child).await {
        Ok(Ok(output)) => {
            let mut text = String::new();
            if !output.stdout.is_empty() {
                text.push_str(&String::from_utf8_lossy(&output.stdout));
            }
            if !output.stderr.is_empty() {
                if !text.is_empty() {
                    text.push_str("\n--- stderr ---\n");
                }
                text.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            if text.is_empty() {
                text = "(no output)".into();
            }
            let truncated = truncate(&text, MAX_COMMAND_CHARS);
            let status = output.status.code().unwrap_or(-1);
            if output.status.success() {
                ToolOutcome::ok(format!("Ran `{command}`"), truncated)
            } else {
                ToolOutcome {
                    ok: false,
                    summary: format!("`{command}` exited {status}"),
                    detail: truncated,
                }
            }
        }
        Ok(Err(error)) => ToolOutcome::err(format!("Failed to start command: {error}")),
        Err(_) => ToolOutcome::err(format!(
            "Command timed out after {}s: {command}",
            timeout.as_secs()
        )),
    }
}

async fn start_process(command: &str, cwd: &Path) -> ToolOutcome {
    if !cwd.exists() {
        return ToolOutcome::err(format!("Working directory not found: {}", cwd.display()));
    }
    let mut child = match Command::new("bash")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return ToolOutcome::err(format!("Failed to start process: {error}")),
    };
    let stdin = child.stdin.take();
    let output = Arc::new(Mutex::new(Vec::new()));
    if let Some(stdout) = child.stdout.take() {
        collect_process_output(stdout, Arc::clone(&output), None);
    }
    if let Some(stderr) = child.stderr.take() {
        collect_process_output(stderr, Arc::clone(&output), Some(b"\n--- stderr ---\n"));
    }

    let mut manager = process_manager().lock().await;
    manager.next_id += 1;
    let id = manager.next_id;
    manager.sessions.insert(
        id,
        ProcessSession {
            command: command.to_string(),
            child,
            stdin,
            output,
            read_offset: 0,
        },
    );
    ToolOutcome::ok(
        format!("Started session {id}: `{command}`"),
        format!("session_id: {id}\nUse interact_with_process to read output or send input."),
    )
}

fn collect_process_output<R>(mut reader: R, output: Arc<Mutex<Vec<u8>>>, prefix: Option<&'static [u8]>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut first = true;
        let mut chunk = [0_u8; 4096];
        loop {
            match reader.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let mut output = output.lock().await;
                    if first {
                        if let Some(prefix) = prefix {
                            output.extend_from_slice(prefix);
                        }
                        first = false;
                    }
                    output.extend_from_slice(&chunk[..count]);
                }
            }
        }
    });
}

async fn interact_with_process(
    id: Option<u64>,
    input: Option<&str>,
    wait_ms: Option<u64>,
) -> ToolOutcome {
    let Some(id) = id else {
        return ToolOutcome::err("interact_with_process needs a session_id");
    };
    let mut manager = process_manager().lock().await;
    let Some(session) = manager.sessions.get_mut(&id) else {
        return ToolOutcome::err(format!("Process session {id} not found"));
    };
    if let Some(input) = input {
        let Some(stdin) = session.stdin.as_mut() else {
            return ToolOutcome::err(format!("Process session {id} has no open stdin"));
        };
        if let Err(error) = stdin.write_all(input.as_bytes()).await {
            return ToolOutcome::err(format!("Cannot write to process session {id}: {error}"));
        }
        if !input.ends_with('\n') {
            if let Err(error) = stdin.write_all(b"\n").await {
                return ToolOutcome::err(format!("Cannot write to process session {id}: {error}"));
            }
        }
        if let Err(error) = stdin.flush().await {
            return ToolOutcome::err(format!("Cannot flush process session {id}: {error}"));
        }
    }
    tokio::time::sleep(Duration::from_millis(
        wait_ms.unwrap_or(SESSION_READ_DELAY_MS).min(5_000),
    ))
    .await;
    let status = match session.child.try_wait() {
        Ok(Some(status)) => format!("exited {}", status.code().unwrap_or(-1)),
        Ok(None) => "running".to_string(),
        Err(error) => return ToolOutcome::err(format!("Cannot inspect process session {id}: {error}")),
    };
    let output = session.output.lock().await;
    let unread = &output[session.read_offset.min(output.len())..];
    let detail = if unread.is_empty() {
        "(no new output)".to_string()
    } else {
        truncate(&String::from_utf8_lossy(unread), MAX_COMMAND_CHARS)
    };
    session.read_offset = output.len();
    ToolOutcome::ok(format!("Session {id} is {status}"), detail)
}

async fn list_processes() -> ToolOutcome {
    let mut manager = process_manager().lock().await;
    if manager.sessions.is_empty() {
        return ToolOutcome::ok("No process sessions", "(none)");
    }
    let mut lines = Vec::new();
    for (id, session) in &mut manager.sessions {
        let status = match session.child.try_wait() {
            Ok(Some(status)) => format!("exited {}", status.code().unwrap_or(-1)),
            Ok(None) => "running".to_string(),
            Err(error) => format!("unknown ({error})"),
        };
        lines.push(format!("{id}\t{status}\t{}", session.command));
    }
    lines.sort();
    ToolOutcome::ok(format!("{} process sessions", lines.len()), lines.join("\n"))
}

async fn kill_process(id: Option<u64>) -> ToolOutcome {
    let Some(id) = id else {
        return ToolOutcome::err("kill_process needs a session_id");
    };
    let mut manager = process_manager().lock().await;
    let Some(mut session) = manager.sessions.remove(&id) else {
        return ToolOutcome::err(format!("Process session {id} not found"));
    };
    match session.child.kill().await {
        Ok(()) => ToolOutcome::ok(
            format!("Killed process session {id}"),
            format!("Killed `{}`", session.command),
        ),
        Err(_) if session.child.try_wait().ok().flatten().is_some() => ToolOutcome::ok(
            format!("Removed exited process session {id}"),
            format!("Removed `{}`", session.command),
        ),
        Err(error) => ToolOutcome::err(format!("Cannot kill process session {id}: {error}")),
    }
}

fn read_text_lossy(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut buf = Vec::new();
    if file.metadata()?.len() > MAX_TEXT_BYTES {
        file.take(MAX_TEXT_BYTES).read_to_end(&mut buf)?;
    } else {
        file.read_to_end(&mut buf)?;
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let clipped: String = text.chars().take(max_chars).collect();
        format!("{clipped}\n… truncated …")
    }
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Used by the file preview dialog.
pub fn preview_text(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut buf = Vec::new();
    if len > MAX_TEXT_BYTES {
        file.seek(SeekFrom::Start(0))?;
        file.take(MAX_TEXT_BYTES).read_to_end(&mut buf)?;
        buf.extend_from_slice("\n… truncated …".as_bytes());
    } else {
        file.read_to_end(&mut buf)?;
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_edit_and_read_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.txt");
        assert!(write_file(&path, "hello world").ok);
        assert!(edit_file(&path, "world", "commando").ok);
        let outcome = read_file(&path, None, 0);
        assert!(outcome.ok);
        assert_eq!(outcome.detail, "hello commando");
    }

    #[test]
    fn search_finds_content() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "invoice-42").unwrap();
        fs::write(dir.path().join("b.txt"), "unrelated").unwrap();
        let outcome = search_files(dir.path(), Some("*.txt"), Some("invoice"));
        assert!(outcome.ok);
        assert!(outcome.detail.contains("a.txt"));
        assert!(!outcome.detail.contains("b.txt"));
    }

    #[test]
    fn unique_edit_required() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dup.txt");
        fs::write(&path, "foo foo").unwrap();
        assert!(!edit_file(&path, "foo", "bar").ok);
    }

    #[tokio::test]
    async fn persistent_process_accepts_input_and_streams_output() {
        let dir = tempdir().unwrap();
        let started = start_process("while read line; do echo reply:$line; done", dir.path()).await;
        assert!(started.ok, "{}", started.detail);
        let id = started
            .detail
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("session_id: "))
            .and_then(|id| id.parse().ok())
            .unwrap();

        let interaction = interact_with_process(Some(id), Some("hello"), Some(500)).await;
        assert!(interaction.ok, "{}", interaction.detail);
        assert!(interaction.detail.contains("reply:hello"));
        assert!(kill_process(Some(id)).await.ok);
    }
}

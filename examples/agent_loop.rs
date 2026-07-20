//! 一个带工具调用的流式 coding agent。
//!
//! 运行方式：
//! `DEEPSEEK_API_KEY=sk-xxx cargo run --example agent_loop -- "为这个项目补充 README"`
//!
//! 默认将当前目录视为工作区；可通过 `CODING_AGENT_ROOT` 限制 agent 可读写的根目录。

use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use futures::StreamExt;
use oven_llm::{
    ContentBlock, Delta, Message, ModelId, OpenAICompatProvider, Provider, Request, SamplingParams,
    StopReason, StreamEvent, Tool,
};
use secrecy::SecretString;
use serde_json::{Value, json};

type ExampleResult<T> = Result<T, Box<dyn Error>>;

const MAX_FILE_BYTES: u64 = 64 * 1024;

#[derive(Debug)]
struct StreamedTurn {
    content: Vec<ContentBlock>,
    stop_reason: Option<StopReason>,
}

#[tokio::main]
async fn main() -> ExampleResult<()> {
    let workspace_root = coding_workspace_root()?;
    let task = coding_task();
    let api_key = env::var("DEEPSEEK_API_KEY").unwrap_or_else(|_| "sk-placeholder".to_string());
    let provider = OpenAICompatProvider::deepseek(SecretString::new(api_key.into()));

    println!("coding workspace: {}", workspace_root.display());
    println!("task: {task}");

    let mut request = Request {
        model: ModelId::from("deepseek-v4-flash"),
        system: Some(format!(
            "You are a careful coding agent. Your workspace root is {}. \
             Use read_file before changing a file. Use write_file only for files inside that root. \
             Make the requested change, then give a concise final summary.",
            workspace_root.display()
        )),
        messages: vec![Message::user(vec![ContentBlock::text(task)])],
        tools: coding_tools(),
        sampling: SamplingParams {
            temperature: Some(0.0),
            ..Default::default()
        },
        ..Default::default()
    };

    loop {
        let turn = collect_streamed_turn(&provider, &request).await?;
        let requests_tools = turn.stop_reason == Some(StopReason::ToolUse)
            && turn
                .content
                .iter()
                .any(|block| matches!(block, ContentBlock::ToolUse { .. }));

        // 关键顺序：先提交完整的 assistant 消息（含 tool_use），再执行工具并追加结果。
        // OpenAI 兼容接口要求 role=tool 紧跟在发起该调用的 assistant 消息之后。
        request
            .messages
            .push(Message::assistant(turn.content.clone()));

        if !requests_tools {
            break;
        }

        let tool_results = execute_requested_tools(&workspace_root, &turn.content);
        request.messages.push(Message::user(tool_results));
    }

    Ok(())
}

/// 消费一次流式响应，并按内容块 index 重建一条完整 assistant 消息。
///
/// 文本和工具参数都可能被拆为多个 delta；工具参数必须在流结束后才可解析为 JSON。
async fn collect_streamed_turn<P: Provider + ?Sized>(
    provider: &P,
    request: &Request,
) -> ExampleResult<StreamedTurn> {
    let mut stream = provider.stream(request).await?;
    let mut blocks = BTreeMap::<usize, ContentBlock>::new();
    let mut tool_arguments = BTreeMap::<usize, String>::new();
    let mut stop_reason = None;

    print!("\nassistant: ");
    io::stdout().flush()?;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::ContentBlockStart { index, block } => {
                if let ContentBlock::ToolUse { input, .. } = &block {
                    let initial_arguments = match input {
                        Value::String(arguments) => arguments.clone(),
                        arguments => arguments.to_string(),
                    };
                    tool_arguments.insert(index, initial_arguments);
                }
                blocks.insert(index, block);
            }
            StreamEvent::ContentBlockDelta { index, delta } => match delta {
                Delta::TextDelta { text } => {
                    let Some(ContentBlock::Text { text: accumulated }) = blocks.get_mut(&index)
                    else {
                        return Err(invalid_stream(format!(
                            "text delta received for unknown/non-text block {index}"
                        )));
                    };
                    print!("{text}");
                    io::stdout().flush()?;
                    accumulated.push_str(&text);
                }
                Delta::InputJsonDelta { partial_json } => {
                    let Some(arguments) = tool_arguments.get_mut(&index) else {
                        return Err(invalid_stream(format!(
                            "tool argument delta received for unknown tool block {index}"
                        )));
                    };
                    arguments.push_str(&partial_json);
                }
            },
            StreamEvent::MessageDelta {
                stop_reason: received_stop_reason,
                ..
            } => stop_reason = received_stop_reason,
            StreamEvent::MessageStart { .. }
            | StreamEvent::ContentBlockStop { .. }
            | StreamEvent::MessageStop => {}
        }
    }
    println!();

    // 将分片的 arguments JSON 回填到 ToolUse 块，之后才能把该消息加入历史。
    for (index, raw_arguments) in tool_arguments {
        let input = if raw_arguments.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&raw_arguments).map_err(|error| {
                invalid_stream(format!(
                    "invalid JSON arguments for tool block {index}: {error}"
                ))
            })?
        };
        let Some(ContentBlock::ToolUse { input: target, .. }) = blocks.get_mut(&index) else {
            return Err(invalid_stream(format!(
                "tool arguments collected for non-tool block {index}"
            )));
        };
        *target = input;
    }

    Ok(StreamedTurn {
        content: blocks.into_values().collect(),
        stop_reason,
    })
}

fn coding_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "read_file".to_string(),
            description: Some("Read a UTF-8 text file from the coding workspace.".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path from the workspace root" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "write_file".to_string(),
            description: Some(
                "Create or replace a UTF-8 text file in the coding workspace.".to_string(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path from the workspace root" },
                    "content": { "type": "string", "description": "Complete replacement file content" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        },
    ]
}

/// 在一轮中执行所有工具调用，并将结果合并成一条 user 消息的内容块。
fn execute_requested_tools(workspace_root: &Path, content: &[ContentBlock]) -> Vec<ContentBlock> {
    content
        .iter()
        .filter_map(|block| {
            let ContentBlock::ToolUse { id, name, input } = block else {
                return None;
            };

            println!("tool: {name}({input})");
            let (output, is_error) = execute_tool(workspace_root, name, input);
            Some(ContentBlock::ToolResult {
                tool_use_id: id.clone(),
                content: vec![ContentBlock::text(output)],
                is_error,
            })
        })
        .collect()
}

fn execute_tool(workspace_root: &Path, name: &str, input: &Value) -> (String, bool) {
    match name {
        "read_file" => {
            let result = tool_path(workspace_root, input).and_then(|path| {
                let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
                if metadata.len() > MAX_FILE_BYTES {
                    return Err(format!(
                        "{} is {} bytes; only files up to {MAX_FILE_BYTES} bytes may be read",
                        path.display(),
                        metadata.len()
                    ));
                }
                fs::read_to_string(&path).map_err(|error| error.to_string())
            });
            result_to_tool_output(result)
        }
        "write_file" => {
            let result = (|| {
                let path = tool_path(workspace_root, input)?;
                let content = input
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "missing string field 'content'".to_string())?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                fs::write(&path, content).map_err(|error| error.to_string())?;
                Ok(format!(
                    "Wrote {} bytes to {}",
                    content.len(),
                    path.display()
                ))
            })();
            result_to_tool_output(result)
        }
        other => (format!("unknown tool: {other}"), true),
    }
}

fn result_to_tool_output(result: Result<String, String>) -> (String, bool) {
    match result {
        Ok(output) => (output, false),
        Err(error) => (error, true),
    }
}

/// 仅接受普通相对路径，避免工具通过 `..` 或绝对路径离开工作区。
fn tool_path(workspace_root: &Path, input: &Value) -> Result<PathBuf, String> {
    let relative_path = input
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing string field 'path'".to_string())?;
    let relative_path = Path::new(relative_path);

    if relative_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(
            "path must be a non-empty relative path without '.' or '..' segments".to_string(),
        );
    }

    Ok(workspace_root.join(relative_path))
}

fn coding_workspace_root() -> ExampleResult<PathBuf> {
    let root = env::var_os("CODING_AGENT_ROOT")
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?);
    Ok(root.canonicalize()?)
}

fn coding_task() -> String {
    let task = env::args().skip(1).collect::<Vec<_>>().join(" ");
    if task.is_empty() {
        "Inspect this Rust project and suggest one small, useful improvement. Do not write files unless it is necessary to complete the improvement.".to_string()
    } else {
        task
    }
}

fn invalid_stream(message: String) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message))
}

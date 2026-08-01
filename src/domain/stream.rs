//! `Delta` / `StreamEvent`：统一流式事件模型。
//!
//! `StreamEvent` 镜像 Anthropic 的事件粒度；每个 provider 的原生流都被翻译成
//! 这套表示，harness 代码无需关心底层 provider 是谁。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use super::message::{ContentBlock, Role};
use super::response::{Response, StopReason, Usage};

/// 内容块的增量更新片段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    ThinkingDelta { thinking: String },
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

/// 统一流式事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    MessageStart {
        id: String,
        model: String,
    },
    ContentBlockStart {
        index: usize,
        block: ContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: Delta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        stop_reason: Option<StopReason>,
        usage: Option<Usage>,
    },
    MessageStop,
}

/// [`StreamCollector::finish`] 可能返回的错误。
#[derive(Debug, Error)]
pub enum StreamCollectorError {
    #[error("stream protocol error: {0}")]
    Stream(String),
}

/// 将流式事件累积为一条完整的 [`Response`]。
///
/// 逐条喂入 [`StreamEvent`]，流结束后调用 [`finish`](Self::finish) 拼装成
/// 与 [`Provider::complete`](crate::Provider::complete) 返回的同构 `Response`。
///
/// ```rust
/// # use oven_llm::*;
/// # fn example(stream: impl futures::Stream<Item = std::result::Result<StreamEvent, ProviderError>>) {
/// # futures::executor::block_on(async {
/// use futures::StreamExt;
/// let mut collector = StreamCollector::new();
/// let mut stream = Box::pin(stream);
/// while let Some(event) = stream.next().await {
///     collector.push(&event?);
/// }
/// let response = collector.finish()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # });
/// # }
/// ```
pub struct StreamCollector {
    id: Option<String>,
    model: Option<String>,
    blocks: BTreeMap<usize, ContentBlock>,
    tool_arguments: BTreeMap<usize, String>,
    stop_reason: Option<StopReason>,
    usage: Option<Usage>,
}

impl StreamCollector {
    /// 创建空的收集器。
    pub fn new() -> Self {
        Self {
            id: None,
            model: None,
            blocks: BTreeMap::new(),
            tool_arguments: BTreeMap::new(),
            stop_reason: None,
            usage: None,
        }
    }

    /// 喂入一条流式事件。
    ///
    /// 调用方可在调用前检查事件内容（例如将文本 delta 打印到终端），
    /// 因为参数为 `&StreamEvent`。
    pub fn push(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::MessageStart { id, model } => {
                self.id = Some(id.clone());
                self.model = Some(model.clone());
            }
            StreamEvent::ContentBlockStart { index, block } => {
                if let ContentBlock::ToolUse { input, .. } = block {
                    let initial_arguments = match input {
                        serde_json::Value::String(arguments) => arguments.clone(),
                        arguments => arguments.to_string(),
                    };
                    self.tool_arguments.insert(*index, initial_arguments);
                }
                self.blocks.insert(*index, block.clone());
            }
            StreamEvent::ContentBlockDelta { index, delta } => match delta {
                Delta::ThinkingDelta { thinking } => {
                    if let Some(ContentBlock::Thinking {
                        thinking: accumulated,
                    }) = self.blocks.get_mut(index)
                    {
                        accumulated.push_str(thinking);
                    }
                }
                Delta::TextDelta { text } => {
                    if let Some(ContentBlock::Text { text: accumulated }) =
                        self.blocks.get_mut(index)
                    {
                        accumulated.push_str(text);
                    }
                }
                Delta::InputJsonDelta { partial_json } => {
                    if let Some(arguments) = self.tool_arguments.get_mut(index) {
                        arguments.push_str(partial_json);
                    }
                }
            },
            StreamEvent::MessageDelta { stop_reason, usage } => {
                self.stop_reason = *stop_reason;
                self.usage = *usage;
            }
            StreamEvent::ContentBlockStop { .. } | StreamEvent::MessageStop => {}
        }
    }

    /// 将收集到的流式事件拼装为一条完整的 [`Response`]。
    ///
    /// 此方法同步执行，会解析分片的工具参数 JSON。
    pub fn finish(self) -> Result<Response, StreamCollectorError> {
        let id = self
            .id
            .ok_or_else(|| StreamCollectorError::Stream("missing MessageStart event".into()))?;
        let model = self.model.unwrap_or_default();

        let mut blocks = self.blocks;

        for (index, raw_arguments) in self.tool_arguments {
            let input = if raw_arguments.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&raw_arguments).map_err(|error| {
                    StreamCollectorError::Stream(format!(
                        "invalid JSON arguments for tool block {index}: {error}"
                    ))
                })?
            };
            let Some(ContentBlock::ToolUse { input: target, .. }) = blocks.get_mut(&index) else {
                return Err(StreamCollectorError::Stream(format!(
                    "tool arguments collected for non-tool block {index}"
                )));
            };
            *target = input;
        }

        Ok(Response {
            id,
            model,
            role: Role::Assistant,
            content: blocks.into_values().collect(),
            stop_reason: self.stop_reason,
            usage: self.usage,
        })
    }
}

impl Default for StreamCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_text_serializes_correctly() {
        let delta = Delta::TextDelta {
            text: "hello".to_string(),
        };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["type"], "text_delta");
        assert_eq!(json["text"], "hello");
        let decoded: Delta = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, delta);
    }

    #[test]
    fn delta_thinking_serializes_correctly() {
        let delta = Delta::ThinkingDelta {
            thinking: "let me reason...".to_string(),
        };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["type"], "thinking_delta");
        assert_eq!(json["thinking"], "let me reason...");
        let decoded: Delta = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, delta);
    }

    #[test]
    fn delta_input_json_round_trips() {
        let delta = Delta::InputJsonDelta {
            partial_json: "{\"a\":1}".to_string(),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: Delta = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, delta);
    }

    #[test]
    fn stream_event_message_start_serializes_correctly() {
        let event = StreamEvent::MessageStart {
            id: "msg_1".to_string(),
            model: "gpt-4".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "message_start");
        assert_eq!(json["id"], "msg_1");
        assert_eq!(json["model"], "gpt-4");
    }

    #[test]
    fn stream_event_content_block_start_serializes_correctly() {
        let event = StreamEvent::ContentBlockStart {
            index: 0,
            block: ContentBlock::text("hi"),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content_block_start");
        assert_eq!(json["index"], 0);
        assert_eq!(json["block"]["type"], "text");
        assert_eq!(json["block"]["text"], "hi");
    }

    #[test]
    fn stream_event_content_block_delta_serializes_correctly() {
        let event = StreamEvent::ContentBlockDelta {
            index: 1,
            delta: Delta::TextDelta {
                text: "chunk".to_string(),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content_block_delta");
        assert_eq!(json["index"], 1);
    }

    #[test]
    fn stream_event_content_block_stop_serializes_correctly() {
        let event = StreamEvent::ContentBlockStop { index: 2 };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "content_block_stop");
        assert_eq!(json["index"], 2);
    }

    #[test]
    fn stream_event_message_delta_serializes_with_optional_fields() {
        let event = StreamEvent::MessageDelta {
            stop_reason: Some(StopReason::EndTurn),
            usage: Some(Usage {
                input_tokens: 3,
                output_tokens: 4,
                cache_read_tokens: 0,
                reasoning_tokens: 0,
            }),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "message_delta");
        assert_eq!(json["stop_reason"], "end_turn");
        assert_eq!(json["usage"]["input_tokens"], 3);
        assert_eq!(json["usage"]["output_tokens"], 4);
    }

    #[test]
    fn stream_event_message_delta_allows_none_fields() {
        let event = StreamEvent::MessageDelta {
            stop_reason: None,
            usage: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["stop_reason"], serde_json::Value::Null);
        assert_eq!(json["usage"], serde_json::Value::Null);
    }

    #[test]
    fn stream_event_message_stop_serializes_correctly() {
        let event = StreamEvent::MessageStop;
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "message_stop");
    }

    #[test]
    fn stream_event_round_trips_through_json() {
        let event = StreamEvent::ContentBlockDelta {
            index: 5,
            delta: Delta::InputJsonDelta {
                partial_json: "{\"x\":1}".to_string(),
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: StreamEvent = serde_json::from_str(&json).unwrap();
        match decoded {
            StreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(index, 5);
                assert_eq!(
                    delta,
                    Delta::InputJsonDelta {
                        partial_json: "{\"x\":1}".to_string()
                    }
                );
            }
            _ => panic!("expected ContentBlockDelta"),
        }
    }

    #[test]
    fn collector_text_only_stream() {
        let mut c = StreamCollector::new();
        c.push(&StreamEvent::MessageStart {
            id: "msg_1".into(),
            model: "gpt-4".into(),
        });
        c.push(&StreamEvent::ContentBlockStart {
            index: 0,
            block: ContentBlock::Text {
                text: String::new(),
            },
        });
        c.push(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::TextDelta {
                text: "Hello".into(),
            },
        });
        c.push(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::TextDelta {
                text: " world".into(),
            },
        });
        c.push(&StreamEvent::ContentBlockStop { index: 0 });
        c.push(&StreamEvent::MessageDelta {
            stop_reason: Some(StopReason::EndTurn),
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: 0,
                reasoning_tokens: 0,
            }),
        });
        c.push(&StreamEvent::MessageStop);

        let resp = c.finish().unwrap();
        assert_eq!(resp.id, "msg_1");
        assert_eq!(resp.model, "gpt-4");
        assert_eq!(resp.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(resp.usage.unwrap().input_tokens, 10);
        assert_eq!(resp.content.len(), 1);
        match &resp.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("expected Text block"),
        }
    }

    #[test]
    fn collector_thinking_and_text_blocks() {
        let mut c = StreamCollector::new();
        c.push(&StreamEvent::MessageStart {
            id: "msg_2".into(),
            model: "claude-3".into(),
        });
        c.push(&StreamEvent::ContentBlockStart {
            index: 0,
            block: ContentBlock::Thinking {
                thinking: String::new(),
            },
        });
        c.push(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::ThinkingDelta {
                thinking: "let me".into(),
            },
        });
        c.push(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::ThinkingDelta {
                thinking: " think".into(),
            },
        });
        c.push(&StreamEvent::ContentBlockStart {
            index: 1,
            block: ContentBlock::Text {
                text: String::new(),
            },
        });
        c.push(&StreamEvent::ContentBlockDelta {
            index: 1,
            delta: Delta::TextDelta {
                text: "answer".into(),
            },
        });
        c.push(&StreamEvent::MessageDelta {
            stop_reason: Some(StopReason::EndTurn),
            usage: None,
        });

        let resp = c.finish().unwrap();
        assert_eq!(resp.content.len(), 2);
        match &resp.content[0] {
            ContentBlock::Thinking { thinking } => assert_eq!(thinking, "let me think"),
            _ => panic!("expected Thinking block"),
        }
        match &resp.content[1] {
            ContentBlock::Text { text } => assert_eq!(text, "answer"),
            _ => panic!("expected Text block"),
        }
    }

    #[test]
    fn collector_tool_use_with_json_arguments() {
        let mut c = StreamCollector::new();
        c.push(&StreamEvent::MessageStart {
            id: "msg_3".into(),
            model: "gpt-4".into(),
        });
        c.push(&StreamEvent::ContentBlockStart {
            index: 0,
            block: ContentBlock::ToolUse {
                id: "tool_1".into(),
                name: "read_file".into(),
                input: serde_json::Value::String(String::new()),
            },
        });
        c.push(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::InputJsonDelta {
                partial_json: "{\"path\":".into(),
            },
        });
        c.push(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::InputJsonDelta {
                partial_json: "\"src/main.rs\"}".into(),
            },
        });
        c.push(&StreamEvent::MessageDelta {
            stop_reason: Some(StopReason::ToolUse),
            usage: None,
        });

        let resp = c.finish().unwrap();
        assert_eq!(resp.stop_reason, Some(StopReason::ToolUse));
        assert_eq!(resp.content.len(), 1);
        match &resp.content[0] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "tool_1");
                assert_eq!(name, "read_file");
                assert_eq!(input["path"], "src/main.rs");
            }
            _ => panic!("expected ToolUse block"),
        }
    }

    #[test]
    fn collector_missing_message_start_errors() {
        let c = StreamCollector::new();
        let err = c.finish().unwrap_err();
        assert!(err.to_string().contains("missing MessageStart"));
    }

    #[test]
    fn collector_invalid_tool_json_errors() {
        let mut c = StreamCollector::new();
        c.push(&StreamEvent::MessageStart {
            id: "msg_4".into(),
            model: "gpt-4".into(),
        });
        c.push(&StreamEvent::ContentBlockStart {
            index: 0,
            block: ContentBlock::ToolUse {
                id: "tool_1".into(),
                name: "test".into(),
                input: serde_json::Value::String(String::new()),
            },
        });
        c.push(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::InputJsonDelta {
                partial_json: "not json".into(),
            },
        });
        c.push(&StreamEvent::MessageDelta {
            stop_reason: Some(StopReason::ToolUse),
            usage: None,
        });

        let err = c.finish().unwrap_err();
        assert!(err.to_string().contains("invalid JSON arguments"));
    }

    #[test]
    fn collector_default_is_same_as_new() {
        let c = StreamCollector::default();
        assert!(c.id.is_none());
        assert!(c.blocks.is_empty());
    }
}

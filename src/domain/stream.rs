//! `Delta` / `StreamEvent`：统一流式事件模型。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中
//! "StreamEvent / Delta（`domain/stream.rs`）" 一节。
//!
//! `StreamEvent` 镜像 Anthropic 的事件粒度；每个 provider 的原生流都被翻译成
//! 这套表示，harness 代码无需关心底层 provider 是谁。

use serde::{Deserialize, Serialize};

use super::message::ContentBlock;
use super::response::{StopReason, Usage};

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
}

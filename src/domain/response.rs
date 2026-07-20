//! `Usage` / `StopReason` / `Response`：provider 无关的非流式响应模型。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中
//! "Usage / StopReason / Response（`domain/response.rs`）" 一节。

use serde::{Deserialize, Serialize};

use super::message::{ContentBlock, Role};

/// 一次调用的 token 用量统计。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cache_read_tokens: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// 响应终止原因，与具体 provider 无关。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// 模型自然结束本轮生成。
    EndTurn,
    /// 命中了请求中指定的停止序列。
    StopSequence,
    /// 模型请求调用工具。
    ToolUse,
    /// 达到了 `max_tokens` 限制。
    MaxTokens,
}

/// 一次 LLM 调用的完整非流式响应，与具体 provider 无关。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub model: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<StopReason>,
    pub usage: Option<Usage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_default_is_zero() {
        let usage = Usage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cache_read_tokens, 0);
    }

    #[test]
    fn usage_serializes_fields() {
        let usage = Usage {
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: 0,
        };
        let json = serde_json::to_value(usage).unwrap();
        assert_eq!(json["input_tokens"], 10);
        assert_eq!(json["output_tokens"], 20);
        assert!(json.get("cache_read_tokens").is_none());
    }

    #[test]
    fn usage_serializes_cache_read_tokens_when_nonzero() {
        let usage = Usage {
            input_tokens: 10,
            output_tokens: 20,
            cache_read_tokens: 5,
        };
        let json = serde_json::to_value(usage).unwrap();
        assert_eq!(json["cache_read_tokens"], 5);
    }

    #[test]
    fn stop_reason_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&StopReason::EndTurn).unwrap(),
            "\"end_turn\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::StopSequence).unwrap(),
            "\"stop_sequence\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::ToolUse).unwrap(),
            "\"tool_use\""
        );
        assert_eq!(
            serde_json::to_string(&StopReason::MaxTokens).unwrap(),
            "\"max_tokens\""
        );
    }

    #[test]
    fn stop_reason_round_trips() {
        for reason in [
            StopReason::EndTurn,
            StopReason::StopSequence,
            StopReason::ToolUse,
            StopReason::MaxTokens,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            let decoded: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, reason);
        }
    }

    #[test]
    fn response_serializes_with_optional_fields_present() {
        let response = Response {
            id: "msg_1".to_string(),
            model: "gpt-4".to_string(),
            role: Role::Assistant,
            content: vec![ContentBlock::text("hi")],
            stop_reason: Some(StopReason::EndTurn),
            usage: Some(Usage {
                input_tokens: 5,
                output_tokens: 7,
                cache_read_tokens: 0,
            }),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["id"], "msg_1");
        assert_eq!(json["model"], "gpt-4");
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["stop_reason"], "end_turn");
        assert_eq!(json["usage"]["input_tokens"], 5);
        assert_eq!(json["usage"]["output_tokens"], 7);
    }

    #[test]
    fn response_allows_none_stop_reason_and_usage() {
        let response = Response {
            id: "msg_2".to_string(),
            model: "gpt-4".to_string(),
            role: Role::Assistant,
            content: vec![],
            stop_reason: None,
            usage: None,
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["stop_reason"], serde_json::Value::Null);
        assert_eq!(json["usage"], serde_json::Value::Null);
    }

    #[test]
    fn response_round_trips_through_json() {
        let response = Response {
            id: "msg_3".to_string(),
            model: "claude-3".to_string(),
            role: Role::Assistant,
            content: vec![ContentBlock::text("42")],
            stop_reason: Some(StopReason::ToolUse),
            usage: Some(Usage {
                input_tokens: 1,
                output_tokens: 2,
                cache_read_tokens: 0,
            }),
        };
        let json = serde_json::to_string(&response).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, response.id);
        assert_eq!(decoded.model, response.model);
        assert_eq!(decoded.role, response.role);
        assert_eq!(decoded.stop_reason, response.stop_reason);
        assert_eq!(decoded.usage, response.usage);
    }
}

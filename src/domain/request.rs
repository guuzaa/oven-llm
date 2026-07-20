//! `ModelId` / `SamplingParams` / `Request`：provider 无关的请求模型。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中
//! "SamplingParams / Request（`domain/request.rs`）" 一节。

use std::{borrow::Borrow, fmt};

use serde::{Deserialize, Serialize};

use super::message::Message;
use super::tool::{Tool, ToolChoice};

/// 单个 provider 范围内的模型标识。
///
/// 该类型保持与字符串完全相同的 serde 表示，但避免将模型选择与任意文本混用。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(String);

impl ModelId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ModelId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ModelId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl AsRef<str> for ModelId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for ModelId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 采样参数：控制模型生成时的随机性与长度限制。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SamplingParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stop: Option<Vec<String>>,
}

/// 一次 LLM 调用的完整请求，与具体 provider 无关。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Request {
    pub model: ModelId,
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub tool_choice: ToolChoice,
    pub sampling: SamplingParams,
    /// Provider 私有参数直通（跳过标准 Serialize，由 transport 层 merge 进
    /// wire JSON body，键名需与 provider wire format 一致，不应与标准字段重名）。
    #[serde(skip, default)]
    pub provider_options: serde_json::Map<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::message::{ContentBlock, Role};

    #[test]
    fn sampling_params_default_is_all_none() {
        let params = SamplingParams::default();
        assert_eq!(params.temperature, None);
        assert_eq!(params.top_p, None);
        assert_eq!(params.max_tokens, None);
        assert_eq!(params.stop, None);
    }

    #[test]
    fn sampling_params_serializes_all_fields() {
        let params = SamplingParams {
            temperature: Some(0.7),
            top_p: Some(0.9),
            max_tokens: Some(1024),
            stop: Some(vec!["STOP".to_string()]),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["temperature"].as_f64().unwrap(), 0.7_f32 as f64);
        assert_eq!(json["top_p"].as_f64().unwrap(), 0.9_f32 as f64);
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["stop"][0], "STOP");
    }

    #[test]
    fn request_default_has_empty_collections() {
        let req = Request::default();
        assert_eq!(req.model, ModelId::default());
        assert_eq!(req.system, None);
        assert!(req.messages.is_empty());
        assert!(req.tools.is_empty());
        assert_eq!(req.tool_choice, ToolChoice::Auto);
        assert_eq!(req.sampling, SamplingParams::default());
        assert!(req.provider_options.is_empty());
    }

    #[test]
    fn provider_options_is_skipped_in_serialization() {
        let mut req = Request {
            model: ModelId::from("gpt-4"),
            ..Default::default()
        };
        req.provider_options
            .insert("top_k".to_string(), serde_json::json!(40));

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("provider_options").is_none());
        assert_eq!(json["model"], "gpt-4");
    }

    #[test]
    fn provider_options_defaults_to_empty_when_deserializing() {
        let json = serde_json::json!({
            "model": "gpt-4",
            "system": null,
            "messages": [],
            "tools": [],
            "tool_choice": "auto",
            "sampling": { "temperature": null, "top_p": null, "max_tokens": null, "stop": null },
            "stream": false
        });
        let req: Request = serde_json::from_value(json).unwrap();
        assert_eq!(req.model.as_str(), "gpt-4");
        assert!(req.provider_options.is_empty());
    }

    #[test]
    fn request_round_trips_without_provider_options() {
        let req = Request {
            model: ModelId::from("gpt-4"),
            system: Some("be helpful".to_string()),
            messages: vec![Message::user(vec![ContentBlock::text("hi")])],
            tools: vec![],
            tool_choice: ToolChoice::Auto,
            sampling: SamplingParams {
                temperature: Some(0.5),
                ..Default::default()
            },
            provider_options: serde_json::Map::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.model, req.model);
        assert_eq!(decoded.system, req.system);
        assert_eq!(decoded.tool_choice, req.tool_choice);
        assert_eq!(decoded.sampling, req.sampling);
        assert_eq!(decoded.messages[0].role, Role::User);
        assert!(decoded.provider_options.is_empty());
    }
}

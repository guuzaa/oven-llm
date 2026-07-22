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

/// 思维链开关：启用或禁用模型的 thinking/reasoning 输出。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ThinkingMode {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "disabled")]
    Disabled,
}

/// 推理强度：控制模型在 thinking 阶段投入的计算量。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    None,
    Low,
    Medium,
    High,
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
    pub thinking: Option<ThinkingMode>,
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Provider 私有参数直通（跳过标准 Serialize，由 transport 层 merge 进
    /// wire JSON body，键名需与 provider wire format 一致，不应与标准字段重名）。
    #[serde(skip, default)]
    pub provider_options: serde_json::Map<String, serde_json::Value>,
}

impl Request {
    /// 设置模型标识。
    pub fn model(&mut self, model: impl Into<ModelId>) -> &mut Self {
        self.model = model.into();
        self
    }

    /// 设置 system 提示。
    pub fn system(&mut self, system: impl Into<String>) -> &mut Self {
        self.system = Some(system.into());
        self
    }

    /// 追加一条消息。
    pub fn message(&mut self, msg: Message) -> &mut Self {
        self.messages.push(msg);
        self
    }

    /// 替换全部消息。
    pub fn messages(&mut self, msgs: Vec<Message>) -> &mut Self {
        self.messages = msgs;
        self
    }

    /// 追加一个工具。
    pub fn tool(&mut self, tool: Tool) -> &mut Self {
        self.tools.push(tool);
        self
    }

    /// 替换全部工具。
    pub fn tools(&mut self, tools: Vec<Tool>) -> &mut Self {
        self.tools = tools;
        self
    }

    /// 设置 tool_choice。
    pub fn tool_choice(&mut self, choice: ToolChoice) -> &mut Self {
        self.tool_choice = choice;
        self
    }

    /// 设置 temperature。
    pub fn temperature(&mut self, value: f32) -> &mut Self {
        self.sampling.temperature = Some(value);
        self
    }

    /// 设置 top_p。
    pub fn top_p(&mut self, value: f32) -> &mut Self {
        self.sampling.top_p = Some(value);
        self
    }

    /// 设置 max_tokens。
    pub fn max_tokens(&mut self, value: u32) -> &mut Self {
        self.sampling.max_tokens = Some(value);
        self
    }

    /// 设置 stop 序列。
    pub fn stop(&mut self, sequences: Vec<String>) -> &mut Self {
        self.sampling.stop = Some(sequences);
        self
    }

    /// 设置思维链模式。
    pub fn thinking(&mut self, mode: ThinkingMode) -> &mut Self {
        self.thinking = Some(mode);
        self
    }

    /// 设置推理强度。
    pub fn reasoning_effort(&mut self, effort: ReasoningEffort) -> &mut Self {
        self.reasoning_effort = Some(effort);
        self
    }

    /// 添加一个 provider 私有参数。
    pub fn provider_option(
        &mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> &mut Self {
        self.provider_options.insert(key.into(), value);
        self
    }
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
        assert_eq!(req.thinking, None);
        assert_eq!(req.reasoning_effort, None);
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
            thinking: Some(ThinkingMode::Enabled),
            reasoning_effort: Some(ReasoningEffort::High),
            provider_options: serde_json::Map::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.model, req.model);
        assert_eq!(decoded.system, req.system);
        assert_eq!(decoded.tool_choice, req.tool_choice);
        assert_eq!(decoded.sampling, req.sampling);
        assert_eq!(decoded.thinking, Some(ThinkingMode::Enabled));
        assert_eq!(decoded.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(decoded.messages[0].role, Role::User);
        assert!(decoded.provider_options.is_empty());
    }

    #[test]
    fn thinking_mode_serializes_as_string() {
        assert_eq!(
            serde_json::to_value(ThinkingMode::Enabled).unwrap(),
            "enabled"
        );
        assert_eq!(
            serde_json::to_value(ThinkingMode::Disabled).unwrap(),
            "disabled"
        );
    }

    #[test]
    fn reasoning_effort_serializes_as_lowercase() {
        assert_eq!(serde_json::to_value(ReasoningEffort::None).unwrap(), "none");
        assert_eq!(serde_json::to_value(ReasoningEffort::Low).unwrap(), "low");
        assert_eq!(
            serde_json::to_value(ReasoningEffort::Medium).unwrap(),
            "medium"
        );
        assert_eq!(serde_json::to_value(ReasoningEffort::High).unwrap(), "high");
    }

    #[test]
    fn thinking_mode_deserializes_from_string() {
        assert_eq!(
            serde_json::from_value::<ThinkingMode>(serde_json::json!("enabled")).unwrap(),
            ThinkingMode::Enabled
        );
        assert_eq!(
            serde_json::from_value::<ThinkingMode>(serde_json::json!("disabled")).unwrap(),
            ThinkingMode::Disabled
        );
    }

    #[test]
    fn reasoning_effort_deserializes_from_string() {
        assert_eq!(
            serde_json::from_value::<ReasoningEffort>(serde_json::json!("none")).unwrap(),
            ReasoningEffort::None
        );
        assert_eq!(
            serde_json::from_value::<ReasoningEffort>(serde_json::json!("high")).unwrap(),
            ReasoningEffort::High
        );
    }

    #[test]
    fn fluent_setters_chain_correctly() {
        let mut req = Request::default();
        req.model("gpt-4")
            .system("be helpful")
            .temperature(0.7)
            .top_p(0.9)
            .max_tokens(1024)
            .stop(vec!["STOP".into()])
            .thinking(ThinkingMode::Enabled)
            .reasoning_effort(ReasoningEffort::High)
            .tool_choice(ToolChoice::Any);

        assert_eq!(req.model.as_str(), "gpt-4");
        assert_eq!(req.system.as_deref(), Some("be helpful"));
        assert_eq!(req.sampling.temperature, Some(0.7));
        assert_eq!(req.sampling.top_p, Some(0.9));
        assert_eq!(req.sampling.max_tokens, Some(1024));
        assert_eq!(req.sampling.stop.as_ref().unwrap(), &["STOP".to_string()]);
        assert_eq!(req.thinking, Some(ThinkingMode::Enabled));
        assert_eq!(req.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(req.tool_choice, ToolChoice::Any);
    }

    #[test]
    fn message_appends() {
        let mut req = Request::default();
        req.message(Message::user(vec![ContentBlock::text("hello")]))
            .message(Message::assistant(vec![ContentBlock::text("hi")]));

        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, Role::User);
        assert_eq!(req.messages[1].role, Role::Assistant);
    }

    #[test]
    fn messages_replaces() {
        let mut req = Request::default();
        req.message(Message::user(vec![ContentBlock::text("old")]))
            .messages(vec![Message::user(vec![ContentBlock::text("new")])]);

        assert_eq!(req.messages.len(), 1);
        // Verify the new content is present (check via serialization roundtrip).
        let json = serde_json::to_value(&req.messages[0]).unwrap();
        assert_eq!(json["content"][0]["text"], "new");
    }

    #[test]
    fn tool_appends() {
        let mut req = Request::default();
        req.tool(Tool {
            name: "a".into(),
            description: None,
            input_schema: serde_json::json!({}),
        })
        .tool(Tool {
            name: "b".into(),
            description: None,
            input_schema: serde_json::json!({}),
        });

        assert_eq!(req.tools.len(), 2);
        assert_eq!(req.tools[0].name, "a");
        assert_eq!(req.tools[1].name, "b");
    }

    #[test]
    fn tools_replaces() {
        let mut req = Request::default();
        req.tool(Tool {
            name: "old".into(),
            description: None,
            input_schema: serde_json::json!({}),
        })
        .tools(vec![Tool {
            name: "new".into(),
            description: None,
            input_schema: serde_json::json!({}),
        }]);

        assert_eq!(req.tools.len(), 1);
        assert_eq!(req.tools[0].name, "new");
    }

    #[test]
    fn provider_option_inserts() {
        let mut req = Request::default();
        req.model("gpt-4")
            .provider_option("top_k", serde_json::json!(40))
            .provider_option("custom_flag", serde_json::json!(true));

        assert_eq!(req.provider_options["top_k"], 40);
        assert_eq!(req.provider_options["custom_flag"], true);
    }

    #[test]
    fn realistic_construction_and_mutation() {
        // Construction phase
        let mut req = Request::default();
        req.model("claude-sonnet-4-20250514")
            .system("You are a coding assistant.")
            .message(Message::user(vec![ContentBlock::text(
                "Write hello world in Rust.",
            )]))
            .tool(Tool {
                name: "run_code".into(),
                description: Some("Execute code".into()),
                input_schema: serde_json::json!({"type": "object"}),
            })
            .temperature(0.0)
            .thinking(ThinkingMode::Enabled);

        // Simulate agent loop mutation
        req.message(Message::assistant(vec![
            ContentBlock::thinking("Let me write code."),
            ContentBlock::text(
                "Here is the code:\n```rust\nfn main() { println!(\"Hello!\"); }\n```",
            ),
        ]))
        .message(Message::user(vec![ContentBlock::text("Run it.")]));

        assert_eq!(req.messages.len(), 3);
        assert_eq!(req.messages[0].role, Role::User);
        assert_eq!(req.messages[1].role, Role::Assistant);
        assert_eq!(req.messages[2].role, Role::User);
        assert_eq!(req.tools.len(), 1);
        assert_eq!(req.thinking, Some(ThinkingMode::Enabled));
    }
}

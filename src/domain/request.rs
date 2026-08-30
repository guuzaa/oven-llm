//! `ModelId` / `SamplingParams` / `Request`：provider 无关的请求模型。

use std::{borrow::Borrow, fmt};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::message::Message;
use super::tool::{Tool, ToolChoice};

/// 模型标识：完整 slug（`vendor/wire-id[:variant]`）或裸 wire id。
///
/// serde 仍是普通字符串。发上游时只用 [`wire_id`](Self::wire_id)，不要把
/// `vendor/` 前缀写进请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(String);

impl ModelId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 构造时写入的完整字符串（可能是 slug，也可能是裸 id）。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// slug 里的 vendor 段（未规范化）。裸 id 返回 `None`。
    pub fn vendor(&self) -> Option<&str> {
        let (vendor, _, _) = split_model_id(&self.0);
        vendor
    }

    /// 发给上游的模型名：去掉 vendor 前缀和 `:variant`。
    pub fn wire_id(&self) -> &str {
        let (_, wire, _) = split_model_id(&self.0);
        wire
    }

    /// 协议变体，例如 `responses`。picker 不展示；手打 slug 时使用。
    pub fn variant(&self) -> Option<&str> {
        let (_, _, variant) = split_model_id(&self.0);
        variant
    }

    /// 把裸 id 补成 `vendor/wire-id[:variant]`。已有 vendor 时只规范 vendor 段。
    pub fn qualify(&self, vendor: &str) -> Self {
        let vendor = canonical_vendor(vendor);
        match self.variant() {
            Some(variant) => Self(format!("{vendor}/{}:{variant}", self.wire_id())),
            None => Self(format!("{vendor}/{}", self.wire_id())),
        }
    }
}

/// 已知厂商别名 → 规范 slug（小写）。未知值转成小写后原样返回。
pub fn canonical_vendor(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "kimi" | "moonshot" => "moonshot".to_string(),
        "glm" | "zhipu" => "zhipu".to_string(),
        "grok" | "xai" => "xai".to_string(),
        other => other.to_string(),
    }
}

fn split_model_id(raw: &str) -> (Option<&str>, &str, Option<&str>) {
    let (main, variant) = match raw.rsplit_once(':') {
        Some((left, right)) if !left.is_empty() && !right.is_empty() && !right.contains('/') => {
            (left, Some(right))
        }
        _ => (raw, None),
    };
    match main.split_once('/') {
        Some((vendor, wire)) if !vendor.is_empty() && !wire.is_empty() => {
            (Some(vendor), wire, variant)
        }
        _ => (None, main, variant),
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ThinkingMode {
    #[serde(rename = "enabled")]
    #[default]
    Enabled,
    #[serde(rename = "disabled")]
    Disabled,
}

impl fmt::Display for ThinkingMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            ThinkingMode::Enabled => "enabled",
            ThinkingMode::Disabled => "disabled",
        })
    }
}

/// 思维链配置：当前轮开关 + 是否清除历史思考。
///
/// `mode` 控制本轮是否产出思维链；`clear_thinking` 控制历史轮的
/// `reasoning_content` 是否被丢掉。两字段正交：后者不改变本轮是否思考。
///
/// `clear_thinking` 为 `None` 时不出现在 Completions wire 上，沿用上游默认
/// （智谱标准 API 为 `true`）。`Some(false)` 即 Preserved Thinking。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Thinking {
    pub mode: ThinkingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear_thinking: Option<bool>,
}

impl Thinking {
    /// 开启思维链，不显式设置 `clear_thinking`。
    pub fn enabled() -> Self {
        Self {
            mode: ThinkingMode::Enabled,
            clear_thinking: None,
        }
    }

    /// 关闭思维链，不显式设置 `clear_thinking`。
    pub fn disabled() -> Self {
        Self {
            mode: ThinkingMode::Disabled,
            clear_thinking: None,
        }
    }

    /// 智谱 Preserved Thinking：开启思考且保留历史思维链。
    pub fn preserved() -> Self {
        Self {
            mode: ThinkingMode::Enabled,
            clear_thinking: Some(false),
        }
    }

    /// 设置是否清除历史思维链（`false` 为 Preserved Thinking）。
    pub fn clear_thinking(mut self, clear: bool) -> Self {
        self.clear_thinking = Some(clear);
        self
    }
}

impl From<ThinkingMode> for Thinking {
    fn from(mode: ThinkingMode) -> Self {
        Self {
            mode,
            clear_thinking: None,
        }
    }
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

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            ReasoningEffort::None => "none",
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
        })
    }
}

/// 采样参数：控制模型生成时的随机性与长度限制。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SamplingParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stop: Option<Vec<String>>,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: Some(1.0),
            top_p: None,
            max_tokens: None,
            stop: None,
        }
    }
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
    pub thinking: Option<Thinking>,
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

    /// 设置思维链配置。接受 [`Thinking`] 或 [`ThinkingMode`]。
    pub fn thinking(&mut self, thinking: impl Into<Thinking>) -> &mut Self {
        self.thinking = Some(thinking.into());
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

// ---------------------------------------------------------------------------
// Builder（消费式 `self` 方法，用于构造阶段的链式调用）
// ---------------------------------------------------------------------------

/// [`RequestBuilder`] 构建失败的错误类型。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BuilderError {
    /// 必需字段 `model` 未设置。
    #[error("builder missing required field: model")]
    MissingModel,
}

/// [`Request`] 的构造器。通过 [`Request::builder()`] 创建。
///
/// ```rust
/// use oven_llm::*;
///
/// let req = Request::builder()
///     .model("gpt-4")
///     .system("be helpful")
///     .message(Message::user(vec![ContentBlock::text("hello")]))
///     .temperature(0.7)
///     .thinking(ThinkingMode::Enabled)
///     .build()
///     .unwrap();
/// ```
#[derive(Debug, Default)]
pub struct RequestBuilder {
    model: Option<ModelId>,
    system: Option<String>,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    tool_choice: ToolChoice,
    sampling: SamplingParams,
    thinking: Option<Thinking>,
    reasoning_effort: Option<ReasoningEffort>,
    provider_options: serde_json::Map<String, serde_json::Value>,
}

impl RequestBuilder {
    /// 设置模型标识。
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 设置 system 提示。
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// 追加一条消息。
    pub fn message(mut self, msg: Message) -> Self {
        self.messages.push(msg);
        self
    }

    pub fn prompt(mut self, msg: impl Into<String>) -> Self {
        self.messages.push(Message::user_text(msg.into()));
        self
    }

    /// 追加一个工具。
    pub fn tool(mut self, tool: Tool) -> Self {
        self.tools.push(tool);
        self
    }

    /// 替换全部工具。
    pub fn tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = tools;
        self
    }

    /// 设置 tool_choice。
    pub fn tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = choice;
        self
    }

    /// 设置 temperature。
    pub fn temperature(mut self, value: f32) -> Self {
        self.sampling.temperature = Some(value);
        self
    }

    /// 设置 top_p。
    pub fn top_p(mut self, value: f32) -> Self {
        self.sampling.top_p = Some(value);
        self
    }

    /// 设置 max_tokens。
    pub fn max_tokens(mut self, value: u32) -> Self {
        self.sampling.max_tokens = Some(value);
        self
    }

    /// 设置 stop 序列。
    pub fn stop(mut self, sequences: Vec<String>) -> Self {
        self.sampling.stop = Some(sequences);
        self
    }

    /// 设置思维链配置。接受 [`Thinking`] 或 [`ThinkingMode`]。
    pub fn thinking(mut self, thinking: impl Into<Thinking>) -> Self {
        let thinking = thinking.into();
        if thinking.mode == ThinkingMode::Enabled && self.reasoning_effort.is_none() {
            self.reasoning_effort = Some(ReasoningEffort::High);
        }
        self.thinking = Some(thinking);
        self
    }

    /// 设置推理强度。
    pub fn reasoning_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning_effort = Some(effort);
        self
    }

    /// 添加一个 provider 私有参数。
    pub fn provider_option(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.provider_options.insert(key.into(), value);
        self
    }

    /// 构建 [`Request`]。未调用 [`model`](Self::model) 时返回错误。
    pub fn build(self) -> Result<Request, BuilderError> {
        let model = self.model.ok_or(BuilderError::MissingModel)?;
        Ok(Request {
            model,
            system: self.system,
            messages: self.messages,
            tools: self.tools,
            tool_choice: self.tool_choice,
            sampling: self.sampling,
            thinking: self.thinking,
            reasoning_effort: self.reasoning_effort,
            provider_options: self.provider_options,
        })
    }
}

impl Request {
    /// 创建 [`RequestBuilder`]。
    pub fn builder() -> RequestBuilder {
        RequestBuilder::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::message::{ContentBlock, Role};

    #[test]
    fn model_id_parses_slug_variant_and_bare() {
        let slug = ModelId::from("deepseek/deepseek-v4-flash:responses");
        assert_eq!(slug.vendor(), Some("deepseek"));
        assert_eq!(slug.wire_id(), "deepseek-v4-flash");
        assert_eq!(slug.variant(), Some("responses"));
        assert_eq!(slug.as_str(), "deepseek/deepseek-v4-flash:responses");

        let nested = ModelId::from("my-proxy/org/model");
        assert_eq!(nested.vendor(), Some("my-proxy"));
        assert_eq!(nested.wire_id(), "org/model");
        assert_eq!(nested.variant(), None);

        let bare = ModelId::from("deepseek-v4-flash");
        assert_eq!(bare.vendor(), None);
        assert_eq!(bare.wire_id(), "deepseek-v4-flash");
        assert_eq!(bare.variant(), None);

        let bare_variant = ModelId::from("deepseek-v4-flash:responses");
        assert_eq!(bare_variant.vendor(), None);
        assert_eq!(bare_variant.wire_id(), "deepseek-v4-flash");
        assert_eq!(bare_variant.variant(), Some("responses"));
    }

    #[test]
    fn model_id_qualify_and_canonical_vendor() {
        let qualified = ModelId::from("deepseek-v4-flash:responses").qualify("kimi");
        assert_eq!(qualified.as_str(), "moonshot/deepseek-v4-flash:responses");
        assert_eq!(canonical_vendor("GROK"), "xai");
        assert_eq!(canonical_vendor("glm"), "zhipu");
        assert_eq!(canonical_vendor("My-Proxy"), "my-proxy");

        let rewritten = ModelId::from("grok/grok-4.6").qualify("xai");
        assert_eq!(rewritten.as_str(), "xai/grok-4.6");
    }

    #[test]
    fn sampling_params_default_is_all_none() {
        let params = SamplingParams::default();
        assert_eq!(params.temperature, Some(1.0));
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
            thinking: Some(Thinking::enabled()),
            reasoning_effort: Some(ReasoningEffort::High),
            provider_options: serde_json::Map::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.model, req.model);
        assert_eq!(decoded.system, req.system);
        assert_eq!(decoded.tool_choice, req.tool_choice);
        assert_eq!(decoded.sampling, req.sampling);
        assert_eq!(decoded.thinking, Some(Thinking::enabled()));
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
    fn builder_chain_produces_correct_request() {
        let req = Request::builder()
            .model("gpt-4")
            .system("be helpful")
            .message(Message::user(vec![ContentBlock::text("hello")]))
            .message(Message::assistant(vec![ContentBlock::text("hi")]))
            .tool(Tool {
                name: "search".into(),
                description: Some("Search the web".into()),
                input_schema: serde_json::json!({"type": "object"}),
            })
            .tool_choice(ToolChoice::Auto)
            .temperature(0.7)
            .top_p(0.9)
            .max_tokens(1024)
            .stop(vec!["STOP".into()])
            .thinking(ThinkingMode::Enabled)
            .reasoning_effort(ReasoningEffort::High)
            .provider_option("top_k", serde_json::json!(40))
            .build()
            .unwrap();

        assert_eq!(req.model.as_str(), "gpt-4");
        assert_eq!(req.system.as_deref(), Some("be helpful"));
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, Role::User);
        assert_eq!(req.messages[1].role, Role::Assistant);
        assert_eq!(req.tools.len(), 1);
        assert_eq!(req.tools[0].name, "search");
        assert_eq!(req.tool_choice, ToolChoice::Auto);
        assert_eq!(req.sampling.temperature, Some(0.7));
        assert_eq!(req.sampling.top_p, Some(0.9));
        assert_eq!(req.sampling.max_tokens, Some(1024));
        assert_eq!(req.sampling.stop.as_ref().unwrap(), &["STOP".to_string()]);
        assert_eq!(req.thinking, Some(Thinking::enabled()));
        assert_eq!(req.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(req.provider_options["top_k"], 40);
    }

    #[test]
    fn builder_errors_when_model_missing() {
        let err = Request::builder().system("be helpful").build().unwrap_err();

        assert_eq!(err, BuilderError::MissingModel);
        assert_eq!(err.to_string(), "builder missing required field: model");
    }

    #[test]
    fn builder_defaults_reasoning_effort_to_high() {
        let req = Request::builder()
            .model("gpt-4")
            .thinking(ThinkingMode::Enabled)
            .build()
            .unwrap();

        assert_eq!(req.reasoning_effort, Some(ReasoningEffort::High));
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
        assert_eq!(req.thinking, Some(Thinking::enabled()));
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
        // Construction phase — use builder
        let mut req = Request::builder()
            .model("claude-sonnet-4-20250514")
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
            .thinking(ThinkingMode::Enabled)
            .build()
            .expect("model id's set");

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
        assert_eq!(req.thinking, Some(Thinking::enabled()));
    }

    #[test]
    fn thinking_from_mode_omits_clear_thinking() {
        let thinking = Thinking::from(ThinkingMode::Enabled);
        assert_eq!(thinking.mode, ThinkingMode::Enabled);
        assert_eq!(thinking.clear_thinking, None);
        assert_eq!(thinking, Thinking::enabled());
    }

    #[test]
    fn thinking_preserved_sets_clear_thinking_false() {
        let thinking = Thinking::preserved();
        assert_eq!(thinking.mode, ThinkingMode::Enabled);
        assert_eq!(thinking.clear_thinking, Some(false));
        assert_eq!(thinking, Thinking::enabled().clear_thinking(false));
    }

    #[test]
    fn builder_accepts_thinking_struct() {
        let req = Request::builder()
            .model("gpt-4")
            .thinking(Thinking::preserved())
            .build()
            .unwrap();

        assert_eq!(req.thinking, Some(Thinking::preserved()));
        assert_eq!(req.reasoning_effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn thinking_serializes_mode_and_optional_clear_thinking() {
        let json = serde_json::to_value(Thinking::enabled()).unwrap();
        assert_eq!(json, serde_json::json!({"mode": "enabled"}));
        assert!(json.get("clear_thinking").is_none());

        let json = serde_json::to_value(Thinking::preserved()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"mode": "enabled", "clear_thinking": false})
        );
    }
}

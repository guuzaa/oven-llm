//! `ModelInfo` / `ModelCapabilities` / `Pricing`：模型能力身份证。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中
//! "模型能力层" 一节。

use crate::ProviderName;

/// 描述单个模型的元数据（所属 provider、上下文窗口、最大输出 token、能力集合、定价）。
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub provider: ProviderName,
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub capabilities: ModelCapabilities,
    pub pricing: Option<Pricing>,
}

impl ModelInfo {
    /// 构造一个仅包含 `id`/`provider` 的最小 `ModelInfo`，其余字段填充为
    /// 零值/默认值。用于 `Provider::list_models` 等无法获得完整能力信息的场景
    /// （Requirement 8.1）。
    pub fn minimal(id: impl Into<String>, provider: ProviderName) -> Self {
        Self {
            id: id.into(),
            provider,
            context_window: 0,
            max_output_tokens: 0,
            capabilities: ModelCapabilities::default(),
            pricing: None,
        }
    }
}

/// `ModelInfo` 中描述模型支持的功能集合。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ModelCapabilities {
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub supports_json_mode: bool,
    pub supports_parallel_tool_calls: bool,
    pub supports_system_prompt: bool,
    pub max_concurrent_tools: Option<u32>,
}

/// 模型的定价信息（每百万 token 的价格）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_fills_zero_defaults() {
        let info = ModelInfo::minimal("gpt-4", ProviderName::OpenAI);
        assert_eq!(info.id, "gpt-4");
        assert_eq!(info.provider, ProviderName::OpenAI);
        assert_eq!(info.context_window, 0);
        assert_eq!(info.max_output_tokens, 0);
        assert_eq!(info.capabilities, ModelCapabilities::default());
        assert_eq!(info.pricing, None);
    }
}

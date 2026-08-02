//! 静态模型元数据：`deepseek` / `grok` 两个 Responses API 服务商的已知模型
//! 列表。

use crate::provider::ProviderName;
use crate::provider::model::{ModelCapabilities, ModelInfo, Pricing};

/// DeepSeek Responses API 服务商的已知模型：`deepseek-v4-flash`。
///
/// 官方文档注明 `deepseek-v4-pro` 暂不支持 Responses API，因此目录中只登记
/// `deepseek-v4-flash`。
pub fn deepseek_models() -> Vec<ModelInfo> {
    vec![ModelInfo {
        id: "deepseek-v4-flash".to_string(),
        provider: ProviderName::DeepSeek,
        context_window: 1_000_000,
        max_output_tokens: 384_000,
        capabilities: ModelCapabilities {
            supports_vision: false,
            supports_tools: true,
            supports_streaming: true,
            supports_json_mode: true,
            supports_parallel_tool_calls: true,
            supports_system_prompt: true,
            max_concurrent_tools: Some(64),
        },
        pricing: Some(Pricing {
            input_per_million: 0.14,
            output_per_million: 0.28,
        }),
    }]
}

/// Grok（xAI）Responses API 服务商的已知模型：`grok-build-0.1`。
pub fn grok_models() -> Vec<ModelInfo> {
    vec![ModelInfo {
        id: "grok-build-0.1".to_string(),
        provider: ProviderName::Grok,
        context_window: 256_000,
        max_output_tokens: 100_000,
        capabilities: ModelCapabilities {
            supports_vision: false,
            supports_tools: true,
            supports_streaming: true,
            supports_json_mode: true,
            supports_parallel_tool_calls: true,
            supports_system_prompt: true,
            max_concurrent_tools: Some(64),
        },
        pricing: Some(Pricing {
            input_per_million: 0.5,
            output_per_million: 1.0,
        }),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn deepseek_models_have_unique_ids_and_provider() {
        let models = deepseek_models();
        let mut seen = HashSet::new();
        for model in &models {
            assert!(seen.insert(model.id.clone()));
            assert_eq!(model.provider, ProviderName::DeepSeek);
        }
        assert!(models.iter().any(|m| m.id == "deepseek-v4-flash"));
    }

    #[test]
    fn grok_models_have_unique_ids_and_provider() {
        let models = grok_models();
        let mut seen = HashSet::new();
        for model in &models {
            assert!(seen.insert(model.id.clone()));
            assert_eq!(model.provider, ProviderName::Grok);
        }
        assert!(models.iter().any(|m| m.id == "grok-build-0.1"));
    }
}

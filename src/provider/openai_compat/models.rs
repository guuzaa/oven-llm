//! 静态模型元数据：`deepseek`/`moonshot`/`zhipu` 三个 OpenAI 兼容服务商的
//! 已知模型列表。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中
//! "provider::openai_compat：OpenAI 兼容实现" 一节，以及
//! `Provider::known_models` 的默认实现（Requirement 8.1）。

use crate::provider::model::{ModelCapabilities, ModelInfo, Pricing};

/// DeepSeek 服务商的已知模型：`deepseek-v4-flash`（轻量、低延迟）与
/// `deepseek-v4-pro`（更大上下文与更强推理能力，支持视觉输入）。
pub fn deepseek_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "deepseek-v4-flash".to_string(),
            provider: "deepseek".to_string(),
            context_window: 128_000,
            max_output_tokens: 8_192,
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
        },
        ModelInfo {
            id: "deepseek-v4-pro".to_string(),
            provider: "deepseek".to_string(),
            context_window: 256_000,
            max_output_tokens: 8_192,
            capabilities: ModelCapabilities {
                supports_vision: true,
                supports_tools: true,
                supports_streaming: true,
                supports_json_mode: true,
                supports_parallel_tool_calls: true,
                supports_system_prompt: true,
                max_concurrent_tools: Some(128),
            },
            pricing: Some(Pricing {
                input_per_million: 2.5,
                output_per_million: 10.0,
            }),
        },
    ]
}

/// Moonshot（Kimi）服务商的已知模型：`kimi-k3`（最新旗舰）、`kimi-k2.7`
/// 与 `kimi-k2.6`（前代长上下文模型）。
pub fn moonshot_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "kimi-k3".to_string(),
            provider: "moonshot".to_string(),
            context_window: 256_000,
            max_output_tokens: 8_192,
            capabilities: ModelCapabilities {
                supports_vision: true,
                supports_tools: true,
                supports_streaming: true,
                supports_json_mode: true,
                supports_parallel_tool_calls: true,
                supports_system_prompt: true,
                max_concurrent_tools: Some(128),
            },
            pricing: Some(Pricing {
                input_per_million: 1.2,
                output_per_million: 4.8,
            }),
        },
        ModelInfo {
            id: "kimi-k2.7".to_string(),
            provider: "moonshot".to_string(),
            context_window: 128_000,
            max_output_tokens: 8_192,
            capabilities: ModelCapabilities {
                supports_vision: true,
                supports_tools: true,
                supports_streaming: true,
                supports_json_mode: true,
                supports_parallel_tool_calls: true,
                supports_system_prompt: true,
                max_concurrent_tools: Some(64),
            },
            pricing: Some(Pricing {
                input_per_million: 0.6,
                output_per_million: 2.4,
            }),
        },
        ModelInfo {
            id: "kimi-k2.6".to_string(),
            provider: "moonshot".to_string(),
            context_window: 128_000,
            max_output_tokens: 4_096,
            capabilities: ModelCapabilities {
                supports_vision: false,
                supports_tools: true,
                supports_streaming: true,
                supports_json_mode: false,
                supports_parallel_tool_calls: false,
                supports_system_prompt: true,
                max_concurrent_tools: Some(32),
            },
            pricing: Some(Pricing {
                input_per_million: 0.3,
                output_per_million: 1.2,
            }),
        },
    ]
}

/// Zhipu（智谱）服务商的已知模型：`glm-5.2`（最新旗舰）、`glm-5.1` 与
/// `glm-5.0`（前代模型）。
pub fn zhipu_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "glm-5.2".to_string(),
            provider: "zhipu".to_string(),
            context_window: 200_000,
            max_output_tokens: 16_384,
            capabilities: ModelCapabilities {
                supports_vision: true,
                supports_tools: true,
                supports_streaming: true,
                supports_json_mode: true,
                supports_parallel_tool_calls: true,
                supports_system_prompt: true,
                max_concurrent_tools: Some(128),
            },
            pricing: Some(Pricing {
                input_per_million: 1.0,
                output_per_million: 4.0,
            }),
        },
        ModelInfo {
            id: "glm-5.1".to_string(),
            provider: "zhipu".to_string(),
            context_window: 128_000,
            max_output_tokens: 8_192,
            capabilities: ModelCapabilities {
                supports_vision: true,
                supports_tools: true,
                supports_streaming: true,
                supports_json_mode: true,
                supports_parallel_tool_calls: true,
                supports_system_prompt: true,
                max_concurrent_tools: Some(64),
            },
            pricing: Some(Pricing {
                input_per_million: 0.5,
                output_per_million: 2.0,
            }),
        },
        ModelInfo {
            id: "glm-5.0".to_string(),
            provider: "zhipu".to_string(),
            context_window: 128_000,
            max_output_tokens: 4_096,
            capabilities: ModelCapabilities {
                supports_vision: false,
                supports_tools: true,
                supports_streaming: true,
                supports_json_mode: false,
                supports_parallel_tool_calls: false,
                supports_system_prompt: true,
                max_concurrent_tools: Some(32),
            },
            pricing: Some(Pricing {
                input_per_million: 0.2,
                output_per_million: 0.8,
            }),
        },
    ]
}

/// 合并 `deepseek_models`/`moonshot_models`/`zhipu_models` 三者，得到
/// 所有 openai_compat 服务商的静态模型元数据。
pub fn all_openai_compat_models() -> Vec<ModelInfo> {
    let mut models = deepseek_models();
    models.extend(moonshot_models());
    models.extend(zhipu_models());
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn assert_unique_ids(models: &[ModelInfo]) {
        let mut seen = HashSet::new();
        for model in models {
            assert!(
                seen.insert(model.id.clone()),
                "duplicate model id: {}",
                model.id
            );
        }
    }

    fn assert_provider(models: &[ModelInfo], expected: &str) {
        for model in models {
            assert_eq!(
                model.provider, expected,
                "model {} has unexpected provider {}",
                model.id, model.provider
            );
        }
    }

    #[test]
    fn deepseek_models_have_unique_ids_and_provider() {
        let models = deepseek_models();
        assert_unique_ids(&models);
        assert_provider(&models, "deepseek");
    }

    #[test]
    fn moonshot_models_have_unique_ids_and_provider() {
        let models = moonshot_models();
        assert_unique_ids(&models);
        assert_provider(&models, "moonshot");
    }

    #[test]
    fn zhipu_models_have_unique_ids_and_provider() {
        let models = zhipu_models();
        assert_unique_ids(&models);
        assert_provider(&models, "zhipu");
    }

    #[test]
    fn all_openai_compat_models_merges_all_three_lists() {
        let deepseek = deepseek_models();
        let moonshot = moonshot_models();
        let zhipu = zhipu_models();
        let all = all_openai_compat_models();

        assert_eq!(all.len(), deepseek.len() + moonshot.len() + zhipu.len());
        assert_unique_ids(&all);

        for model in deepseek.iter().chain(moonshot.iter()).chain(zhipu.iter()) {
            assert!(
                all.iter()
                    .any(|m| m.id == model.id && m.provider == model.provider),
                "missing model {} in all_openai_compat_models()",
                model.id
            );
        }
    }
}

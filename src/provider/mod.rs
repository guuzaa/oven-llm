//! `provider` 层：`Provider` trait、错误类型与模型能力信息。

mod builder;
mod completions;
mod error;
pub(crate) mod http;
pub mod model;
mod registry;
mod responses;
pub mod validate;

pub use builder::ProviderBuilder;
pub use completions::CompletionsProvider;
pub use error::{ProviderError, Result};
pub use model::{ModelCapabilities, ModelInfo, Pricing};
pub use registry::ModelRegistry;
pub use responses::ResponsesProvider;
pub use validate::{ValidationError, estimate_input_tokens, validate_request};

use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::domain::{ModelId, Request, Response, StreamEvent};

/// 统一的 LLM 调用抽象：harness/应用代码只依赖该 trait 与 domain 类型，
/// 永远不接触任何 wire format 类型。
#[async_trait]
pub trait Provider: Send + Sync {
    /// 发送一次非流式请求，返回完整响应。
    async fn complete(&self, req: &Request) -> Result<Response>;

    /// 发送一次流式请求，返回统一的 `StreamEvent` 流。
    async fn stream(&self, req: &Request) -> Result<BoxStream<'static, Result<StreamEvent>>>;

    /// 该 provider 静态已知的模型列表，默认空，由具体实现覆写。
    fn known_models(&self) -> Vec<ModelInfo> {
        vec![]
    }

    /// 查找静态模型元数据。未命中表示 provider 将以宽松策略透传该模型请求。
    fn resolve_model(&self, id: &ModelId) -> Option<&ModelInfo>;

    /// 从 provider 的 `/models` 端点动态获取模型列表（能力字段通常不可用）。
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![])
    }

    fn provider_name(&self) -> ProviderName;
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderName {
    OpenAI,
    DeepSeek,
    Moonshot,
    Zhipu,
    Anthropic,
    Grok,
    Custom(String),
}

/// Provider 的协议种类，用于统一构造入口（[`ProviderBuilder`]）的派发。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// OpenAI Chat Completions 兼容协议（`CompletionsProvider`）。
    Completions,
    /// OpenAI Responses API（`ResponsesProvider`）。
    Responses,
    /// Anthropic Messages API (Not Implemented yet)
    Messages,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个不覆写 `known_models` / `list_models` 的最小 `Provider` 实现，
    /// 用于验证默认实现的行为（Requirements 2.3, 2.4, 2.5）。
    struct StubProvider;

    #[async_trait]
    impl Provider for StubProvider {
        async fn complete(&self, _req: &Request) -> Result<Response> {
            unimplemented!("not needed for this test")
        }

        fn resolve_model(&self, _id: &ModelId) -> Option<&ModelInfo> {
            None
        }

        async fn stream(&self, _req: &Request) -> Result<BoxStream<'static, Result<StreamEvent>>> {
            unimplemented!("not needed for this test")
        }

        fn provider_name(&self) -> ProviderName {
            ProviderName::Custom("Stub".into())
        }
    }

    #[test]
    fn known_models_default_is_empty() {
        let provider = StubProvider;
        assert!(provider.known_models().is_empty());
        assert!(provider.resolve_model(&ModelId::from("unknown")).is_none());
    }

    #[tokio::test]
    async fn list_models_default_is_empty() {
        let provider = StubProvider;
        let models = provider.list_models().await.unwrap();
        assert!(models.is_empty());
    }
}

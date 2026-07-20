//! OpenAI 兼容 Provider 实现：wire 类型、encoder、decoder、静态模型元数据
//! 与 `Provider` 实现。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中
//! "provider::openai_compat：OpenAI 兼容实现" 一节。
//!
//! 目前已实现 `types.rs`（任务 8）、`encoder.rs`（任务 9）、`decoder.rs`
//! 的非流式与流式部分（任务 10、11）、`models.rs`（任务 13）与
//! `provider.rs`（任务 14）。

pub mod decoder;
pub mod encoder;
pub mod models;
pub mod provider;
pub mod types;

pub use models::{all_openai_compat_models, deepseek_models, moonshot_models, zhipu_models};
pub use provider::OpenAICompatProvider;

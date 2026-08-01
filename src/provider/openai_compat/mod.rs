//! OpenAI 兼容 Provider 实现：wire 类型、encoder、decoder、静态模型元数据
//! 与 `Provider` 实现。

pub mod decoder;
pub mod encoder;
pub mod models;
pub mod provider;
pub mod types;

pub use decoder::DecodeError;
pub use encoder::EncodeError;
pub use models::{all_openai_compat_models, deepseek_models, moonshot_models, zhipu_models};
pub use provider::OpenAICompatProvider;

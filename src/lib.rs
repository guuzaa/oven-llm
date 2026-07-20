//! `oven-llm` 核心能力层：统一的多 provider LLM 调用抽象。
//!
//! 参见 `.kiro/specs/oven-llm-core/design.md` 了解整体架构。

mod domain;
mod provider;

pub use domain::*;
pub use provider::*;

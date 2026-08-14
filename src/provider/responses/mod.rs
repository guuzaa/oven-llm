//! OpenAI Responses API Provider 实现：wire 类型、encoder、decoder 与
//! `Provider` 实现。
//!
//! 结构镜像 `openai_compat`，但 wire 格式为 OpenAI Responses API（`POST
//! /responses` + SSE 事件流）；base_url 一律由调用方通过 `with_base_url` /
//! `with_models` 显式配置，crate 不提供任何 provider 预设。

pub mod decoder;
pub mod encoder;
pub mod provider;
#[cfg(test)]
mod testdata;
pub mod types;

pub use decoder::DecodeError;
pub use encoder::EncodeError;
pub use provider::ResponsesProvider;

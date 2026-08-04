//! 统一 Provider 构造入口：`ProviderKind` + `ProviderName` + `api_key`。
//!
//! [`ProviderBuilder`] 把 `CompletionsProvider` / `ResponsesProvider` 的
//! 分别调用收口为单一入口：必填 `kind` / `provider_name` / `api_key`，可选
//! `base_url` / `extra_headers` / `known_models`。已知预设会保留自身的 base_url
//! 与静态模型目录，可在此基础上追加模型与请求头；`Custom` 服务商需显式
//! `base_url`。`build()` 返回 `Result<Box<dyn Provider>>`，不支持的组合返回
//! [`ProviderError::UnsupportedProvider`]，配置缺失或冲突返回
//! [`ProviderError::InvalidProviderConfig`]。

use isahc::http::header::HeaderMap;
use secrecy::SecretString;

use crate::provider::completions::CompletionsProvider;
use crate::provider::completions::models::{deepseek_models, moonshot_models, zhipu_models};
use crate::provider::completions::provider::{
    DEEPSEEK_BASE_URL as COMPLETIONS_DEEPSEEK_BASE_URL,
    MOONSHOT_BASE_URL as COMPLETIONS_MOONSHOT_BASE_URL,
    OPENAI_BASE_URL as COMPLETIONS_OPENAI_BASE_URL, ZHIPU_BASE_URL as COMPLETIONS_ZHIPU_BASE_URL,
};
use crate::provider::model::ModelInfo;
use crate::provider::responses::ResponsesProvider;
use crate::provider::responses::models::{
    deepseek_models as responses_deepseek_models, grok_models,
};
use crate::provider::responses::provider::{
    DEEPSEEK_BASE_URL as RESPONSES_DEEPSEEK_BASE_URL, GROK_BASE_URL as RESPONSES_GROK_BASE_URL,
    OPENAI_BASE_URL as RESPONSES_OPENAI_BASE_URL,
};
use crate::provider::{Provider, ProviderError, ProviderKind, ProviderName};

/// 统一创建 `Provider` 的 builder。
///
/// 示例：
/// ```
/// use oven_llm::{Provider, ProviderBuilder, ProviderKind, ProviderName};
///
/// let provider = ProviderBuilder::new(ProviderKind::Responses)
///     .provider_name(ProviderName::DeepSeek)
///     .api_key("sk-...")
///     .build()
///     .unwrap();
/// assert_eq!(provider.provider_name(), ProviderName::DeepSeek);
/// ```
#[derive(Debug)]
pub struct ProviderBuilder {
    kind: Option<ProviderKind>,
    provider_name: Option<ProviderName>,
    api_key: Option<SecretString>,
    base_url: Option<String>,
    extra_headers: HeaderMap,
    known_models: Vec<ModelInfo>,
}

impl Default for ProviderBuilder {
    fn default() -> Self {
        Self {
            kind: None,
            provider_name: None,
            api_key: None,
            base_url: None,
            extra_headers: HeaderMap::new(),
            known_models: Vec::new(),
        }
    }
}

impl ProviderBuilder {
    /// 创建一个 builder，随后用 `kind` / `provider_name` / `api_key`
    /// 设置协议种类（`Completions` / `Responses` / `Messages`）。
    /// 填充必填项并调用 [`Self::build`]。
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            kind: Some(kind),
            ..Default::default()
        }
    }

    /// 创建一个 `Completions` 协议的 builder（等价于
    /// `::new(ProviderKind::Completions)`）。
    pub fn completions() -> Self {
        Self {
            kind: Some(ProviderKind::Completions),
            ..Default::default()
        }
    }

    /// 创建一个 `Responses` 协议的 builder（等价于
    /// `::new(ProviderKind::Responses)`）。
    pub fn responses() -> Self {
        Self {
            kind: Some(ProviderKind::Responses),
            ..Default::default()
        }
    }

    /// 创建一个 `Messages` 协议的 builder（等价于
    /// `::new(ProviderKind::Messages)`）。
    ///
    /// !! `Messages` 尚未实现，`build()` 会返回 `UnsupportedProvider`。
    pub fn messages() -> Self {
        Self {
            kind: Some(ProviderKind::Messages),
            ..Default::default()
        }
    }

    /// 设置服务商名称（`ProviderName`，如 `DeepSeek` / `Custom(...)`）。
    pub fn provider_name(mut self, provider_name: ProviderName) -> Self {
        self.provider_name = Some(provider_name);
        self
    }

    /// 设置 API key；接受 `SecretString` 或任何能转换为它的类型
    /// （`String` / `&str`）。
    pub fn api_key(mut self, api_key: impl Into<SecretString>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// 设置自定义 `base_url`。提供后按 `Custom` 路径构造，任意
    /// `ProviderName`（含 `Custom`）都允许。
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// 设置额外请求头（已知预设或自定义 `base_url` 时生效）。
    pub fn extra_headers(mut self, extra_headers: HeaderMap) -> Self {
        self.extra_headers = extra_headers;
        self
    }

    /// 设置静态模型元数据（已知预设或自定义 `base_url` 时生效）。
    pub fn known_models(mut self, known_models: Vec<ModelInfo>) -> Self {
        self.known_models = known_models;
        self
    }

    /// 追加一个静态模型元数据（已知预设或自定义 `base_url` 时生效）。
    pub fn add_model(mut self, model: ModelInfo) -> Self {
        self.known_models.push(model);
        self
    }

    /// 按当前配置构造 provider。
    ///
    /// - 缺失 `kind` / `provider_name` / `api_key` → `InvalidProviderConfig`
    /// - 提供 `base_url` → 完全自定义路径（仅使用用户传入的模型/头）
    /// - 命中已知预设 → 预设 base_url + 静态模型目录，并叠加用户追加的模型
    ///   与请求头
    /// - 未知服务商带模型/头但未提供 `base_url` → `InvalidProviderConfig`
    /// - 不支持的组合 →
    ///   `UnsupportedProvider`
    pub fn build(self) -> Result<Box<dyn Provider>, ProviderError> {
        let kind = self.kind.ok_or_else(|| {
            ProviderError::InvalidProviderConfig("missing required field `kind`".into())
        })?;
        let provider_name = self.provider_name.ok_or_else(|| {
            ProviderError::InvalidProviderConfig("missing required field `provider_name`".into())
        })?;
        let api_key = self.api_key.ok_or_else(|| {
            ProviderError::InvalidProviderConfig("missing required field `api_key`".into())
        })?;

        if let Some(base_url) = self.base_url {
            return match kind {
                ProviderKind::Completions => Ok(Box::new(CompletionsProvider::with_models(
                    base_url,
                    provider_name,
                    api_key,
                    self.known_models,
                    self.extra_headers,
                ))),
                ProviderKind::Responses => Ok(Box::new(ResponsesProvider::with_models(
                    base_url,
                    provider_name,
                    api_key,
                    self.known_models,
                    self.extra_headers,
                ))),
                ProviderKind::Messages => Err(ProviderError::UnsupportedProvider {
                    kind,
                    name: provider_name,
                }),
            };
        }

        if let Some((base_url, mut preset_models)) = preset(kind, &provider_name) {
            preset_models.extend(self.known_models);
            return match kind {
                ProviderKind::Completions => Ok(Box::new(CompletionsProvider::with_models(
                    base_url,
                    provider_name,
                    api_key,
                    preset_models,
                    self.extra_headers,
                ))),
                ProviderKind::Responses => Ok(Box::new(ResponsesProvider::with_models(
                    base_url,
                    provider_name,
                    api_key,
                    preset_models,
                    self.extra_headers,
                ))),
                ProviderKind::Messages => Err(ProviderError::UnsupportedProvider {
                    kind,
                    name: provider_name,
                }),
            };
        }

        if !self.known_models.is_empty() || !self.extra_headers.is_empty() {
            return Err(ProviderError::InvalidProviderConfig(
                "`known_models` / `extra_headers` require `base_url`".into(),
            ));
        }

        Err(ProviderError::UnsupportedProvider {
            kind,
            name: provider_name,
        })
    }
}

/// 已知预设表：返回 `(base_url, 静态模型目录)`；未知组合返回 `None`。
fn preset(kind: ProviderKind, name: &ProviderName) -> Option<(&'static str, Vec<ModelInfo>)> {
    match (kind, name) {
        (ProviderKind::Completions, ProviderName::OpenAI) => {
            Some((COMPLETIONS_OPENAI_BASE_URL, Vec::new()))
        }
        (ProviderKind::Completions, ProviderName::DeepSeek) => {
            Some((COMPLETIONS_DEEPSEEK_BASE_URL, deepseek_models()))
        }
        (ProviderKind::Completions, ProviderName::Moonshot) => {
            Some((COMPLETIONS_MOONSHOT_BASE_URL, moonshot_models()))
        }
        (ProviderKind::Completions, ProviderName::Zhipu) => {
            Some((COMPLETIONS_ZHIPU_BASE_URL, zhipu_models()))
        }
        (ProviderKind::Responses, ProviderName::OpenAI) => {
            Some((RESPONSES_OPENAI_BASE_URL, Vec::new()))
        }
        (ProviderKind::Responses, ProviderName::DeepSeek) => {
            Some((RESPONSES_DEEPSEEK_BASE_URL, responses_deepseek_models()))
        }
        (ProviderKind::Responses, ProviderName::Grok) => {
            Some((RESPONSES_GROK_BASE_URL, grok_models()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(kind: ProviderKind, name: ProviderName) -> Result<Box<dyn Provider>, ProviderError> {
        ProviderBuilder::new(kind)
            .provider_name(name)
            .api_key("k")
            .build()
    }

    #[test]
    fn completions_presets_dispatch() {
        for name in [
            ProviderName::OpenAI,
            ProviderName::DeepSeek,
            ProviderName::Moonshot,
            ProviderName::Zhipu,
        ] {
            let provider = build(ProviderKind::Completions, name.clone()).unwrap();
            assert_eq!(provider.provider_name(), name);
        }
    }

    #[test]
    fn responses_presets_dispatch() {
        for name in [
            ProviderName::OpenAI,
            ProviderName::DeepSeek,
            ProviderName::Grok,
        ] {
            let provider = build(ProviderKind::Responses, name.clone()).unwrap();
            assert_eq!(provider.provider_name(), name);
        }
    }

    #[test]
    fn unsupported_combinations_return_error() {
        let cases = [
            (ProviderKind::Completions, ProviderName::Anthropic),
            (ProviderKind::Completions, ProviderName::Grok),
            (ProviderKind::Completions, ProviderName::Custom("x".into())),
            (ProviderKind::Responses, ProviderName::Anthropic),
            (ProviderKind::Responses, ProviderName::Moonshot),
            (ProviderKind::Responses, ProviderName::Zhipu),
            (ProviderKind::Responses, ProviderName::Custom("x".into())),
        ];

        for (kind, name) in cases {
            let err = build(kind, name.clone()).err().unwrap();
            match err {
                ProviderError::UnsupportedProvider {
                    kind: got_kind,
                    name: got_name,
                } => {
                    assert_eq!(got_kind, kind);
                    assert_eq!(got_name, name);
                }
                other => panic!("expected UnsupportedProvider, got {other:?}"),
            }
        }
    }

    #[test]
    fn custom_provider_via_base_url() {
        for kind in [ProviderKind::Completions, ProviderKind::Responses] {
            let provider = ProviderBuilder::new(kind)
                .provider_name(ProviderName::Custom("my-gateway".into()))
                .api_key("k")
                .base_url("https://example.com")
                .build()
                .unwrap();
            assert_eq!(
                provider.provider_name(),
                ProviderName::Custom("my-gateway".into())
            );
        }
    }

    #[test]
    fn missing_required_fields_fail() {
        let err = ProviderBuilder::completions()
            .api_key("k")
            .build()
            .err()
            .unwrap();
        assert!(matches!(err, ProviderError::InvalidProviderConfig(_)));

        let err = ProviderBuilder::completions()
            .provider_name(ProviderName::DeepSeek)
            .build()
            .err()
            .unwrap();
        assert!(matches!(err, ProviderError::InvalidProviderConfig(_)));
    }

    #[test]
    fn extra_config_without_base_url_fails() {
        let err = ProviderBuilder::completions()
            .provider_name(ProviderName::Custom("x".into()))
            .api_key("k")
            .known_models(vec![ModelInfo::minimal(
                "m",
                ProviderName::Custom("x".into()),
            )])
            .build()
            .err()
            .unwrap();
        assert!(matches!(err, ProviderError::InvalidProviderConfig(_)));

        let mut headers = HeaderMap::new();
        headers.insert("x-test", "1".parse().unwrap());
        let err = ProviderBuilder::new(ProviderKind::Completions)
            .provider_name(ProviderName::Custom("x".into()))
            .api_key("k")
            .extra_headers(headers)
            .build()
            .err()
            .unwrap();
        assert!(matches!(err, ProviderError::InvalidProviderConfig(_)));
    }

    #[test]
    fn preset_catalog_preserved_and_models_appended() {
        let provider = ProviderBuilder::completions()
            .provider_name(ProviderName::DeepSeek)
            .api_key("k")
            .add_model(ModelInfo::minimal("custom-extra", ProviderName::DeepSeek))
            .build()
            .unwrap();

        assert_eq!(provider.provider_name(), ProviderName::DeepSeek);
        let models = provider.known_models();
        assert!(
            models.iter().any(|m| m.id == "deepseek-v4-flash"),
            "preset catalog should be preserved"
        );
        assert!(
            models.iter().any(|m| m.id == "custom-extra"),
            "added model should be present"
        );
    }

    #[test]
    fn all_presets_accept_added_models() {
        let cases = [
            (ProviderKind::Completions, ProviderName::OpenAI),
            (ProviderKind::Completions, ProviderName::DeepSeek),
            (ProviderKind::Completions, ProviderName::Moonshot),
            (ProviderKind::Completions, ProviderName::Zhipu),
            (ProviderKind::Responses, ProviderName::OpenAI),
            (ProviderKind::Responses, ProviderName::DeepSeek),
            (ProviderKind::Responses, ProviderName::Grok),
        ];

        for (kind, name) in cases {
            let provider = ProviderBuilder::new(kind)
                .provider_name(name.clone())
                .api_key("k")
                .add_model(ModelInfo::minimal("custom-extra", name.clone()))
                .build()
                .unwrap();

            assert_eq!(provider.provider_name(), name);
            assert!(
                provider
                    .known_models()
                    .iter()
                    .any(|m| m.id == "custom-extra"),
                "{kind:?} + {name:?} should include the added model"
            );
        }
    }

    #[test]
    fn preset_accepts_extra_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-test", "1".parse().unwrap());

        let provider = ProviderBuilder::responses()
            .provider_name(ProviderName::DeepSeek)
            .api_key("k")
            .extra_headers(headers)
            .build()
            .unwrap();

        assert_eq!(provider.provider_name(), ProviderName::DeepSeek);
    }

    #[test]
    fn base_url_wins_over_preset_catalog() {
        let provider = ProviderBuilder::completions()
            .provider_name(ProviderName::DeepSeek)
            .api_key("k")
            .base_url("https://example.com")
            .add_model(ModelInfo::minimal("custom-extra", ProviderName::DeepSeek))
            .build()
            .unwrap();

        let models = provider.known_models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "custom-extra");
    }

    #[test]
    fn messages_kind_is_unsupported() {
        for with_base_url in [false, true] {
            let mut builder = ProviderBuilder::messages()
                .provider_name(ProviderName::Anthropic)
                .api_key("k");
            if with_base_url {
                builder = builder.base_url("https://example.com");
            }

            let err = builder.build().err().unwrap();
            assert!(
                matches!(
                    err,
                    ProviderError::UnsupportedProvider {
                        kind: ProviderKind::Messages,
                        ..
                    }
                ),
                "Messages should be unsupported (with_base_url={with_base_url})"
            );
        }
    }

    #[test]
    fn known_models_with_base_url_are_used() {
        let provider = ProviderBuilder::completions()
            .provider_name(ProviderName::Custom("gw".into()))
            .api_key("k")
            .base_url("https://example.com")
            .known_models(vec![ModelInfo::minimal(
                "custom-model",
                ProviderName::Custom("gw".into()),
            )])
            .build()
            .unwrap();

        let models = provider.known_models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "custom-model");
    }

    #[test]
    fn add_model_appends_after_known_models() {
        let first = ModelInfo::minimal("first-model", ProviderName::Custom("gw".into()));
        let second = ModelInfo::minimal("second-model", ProviderName::Custom("gw".into()));

        let provider = ProviderBuilder::completions()
            .provider_name(ProviderName::Custom("gw".into()))
            .api_key("k")
            .base_url("https://example.com")
            .known_models(vec![first])
            .add_model(second)
            .build()
            .unwrap();

        let models = provider.known_models();
        assert_eq!(models.len(), 2);
        let mut ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, ["first-model", "second-model"]);
    }
}

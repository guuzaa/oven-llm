//! 统一 Provider 构造入口：`ProviderKind` + `ProviderName` + `api_key` +
//! `base_url`。
//!
//! [`ProviderBuilder`] 把 `CompletionsProvider` / `ResponsesProvider` 的
//! 分别调用收口为单一入口：必填 `kind` / `provider_name` / `api_key` /
//! `base_url`，可选 `extra_headers` / 模型目录。crate 不提供任何 provider
//! 预设或模型信息；base_url 与模型元数据（`known_models` / `add_model` /
//! `model_registry`）全部由用户配置。`build()` 返回
//! `Result<Box<dyn Provider>>`，未实现的协议返回
//! [`ProviderError::UnsupportedProvider`]，配置缺失返回
//! [`ProviderError::InvalidProviderConfig`]。

use isahc::http::header::HeaderMap;
use secrecy::SecretString;

use crate::provider::completions::CompletionsProvider;
use crate::provider::model::ModelInfo;
use crate::provider::responses::ResponsesProvider;
use crate::provider::{ModelRegistry, Provider, ProviderError, ProviderKind, ProviderName};

/// 统一创建 `Provider` 的 builder。
///
/// 示例：
/// ```
/// use oven_llm::{Provider, ProviderBuilder, ProviderKind, ProviderName};
///
/// let provider = ProviderBuilder::new(ProviderKind::Responses)
///     .provider_name(ProviderName::DeepSeek)
///     .api_key("sk-...")
///     .base_url("https://api.deepseek.com")
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
    /// / `base_url` 设置协议种类（`Completions` / `Responses` / `Messages`）
    /// 与端点。填充必填项并调用 [`Self::build`]。
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

    /// 设置 provider 的 `base_url`（必填）。任意 `ProviderName`
    /// （含 `Custom`）都允许。
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// 设置额外请求头（已知预设或自定义 `base_url` 时生效）。
    pub fn extra_headers(mut self, extra_headers: HeaderMap) -> Self {
        self.extra_headers = extra_headers;
        self
    }

    /// 设置模型元数据（已知预设或自定义 `base_url` 时生效）。
    pub fn known_models(mut self, known_models: Vec<ModelInfo>) -> Self {
        self.known_models = known_models;
        self
    }

    /// 追加一个模型元数据（已知预设或自定义 `base_url` 时生效）。
    pub fn add_model(mut self, model: ModelInfo) -> Self {
        self.known_models.push(model);
        self
    }

    /// 用 [`ModelRegistry`] 设置模型目录，等价于把注册表中的模型批量传给
    /// [`Self::known_models`]。
    ///
    /// 与 `known_models` 同为设置语义：后调用者覆盖前者；随后仍可用
    /// [`Self::add_model`] 追加单个模型。
    pub fn model_registry(mut self, registry: ModelRegistry) -> Self {
        self.known_models = registry.into_models();
        self
    }

    /// 按当前配置构造 provider。
    ///
    /// - 缺失 `kind` / `provider_name` / `api_key` / `base_url` →
    ///   `InvalidProviderConfig`
    /// - `Messages` 协议尚未实现 → `UnsupportedProvider`
    /// - 其余组合：用传入的 base_url、模型目录与请求头构造对应 provider
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

        if kind == ProviderKind::Messages {
            return Err(ProviderError::UnsupportedProvider {
                kind,
                name: provider_name,
            });
        }

        let base_url = self.base_url.ok_or_else(|| {
            ProviderError::InvalidProviderConfig("missing required field `base_url`".into())
        })?;

        match kind {
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
            ProviderKind::Messages => unreachable!("Messages handled above"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(kind: ProviderKind, name: ProviderName) -> Result<Box<dyn Provider>, ProviderError> {
        ProviderBuilder::new(kind)
            .provider_name(name)
            .api_key("k")
            .base_url("https://example.com")
            .build()
    }

    #[test]
    fn any_provider_name_with_base_url_builds() {
        let cases = [
            (ProviderKind::Completions, ProviderName::OpenAI),
            (ProviderKind::Completions, ProviderName::DeepSeek),
            (ProviderKind::Completions, ProviderName::Moonshot),
            (ProviderKind::Completions, ProviderName::Zhipu),
            (ProviderKind::Completions, ProviderName::Anthropic),
            (ProviderKind::Completions, ProviderName::Grok),
            (ProviderKind::Completions, ProviderName::Custom("x".into())),
            (ProviderKind::Responses, ProviderName::OpenAI),
            (ProviderKind::Responses, ProviderName::DeepSeek),
            (ProviderKind::Responses, ProviderName::Grok),
            (ProviderKind::Responses, ProviderName::Anthropic),
            (ProviderKind::Responses, ProviderName::Moonshot),
            (ProviderKind::Responses, ProviderName::Zhipu),
            (ProviderKind::Responses, ProviderName::Custom("x".into())),
        ];

        for (kind, name) in cases {
            let provider = build(kind, name.clone()).unwrap();
            assert_eq!(provider.provider_name(), name);
        }
    }

    #[test]
    fn build_requires_base_url() {
        let cases = [
            ProviderName::OpenAI,
            ProviderName::DeepSeek,
            ProviderName::Moonshot,
            ProviderName::Zhipu,
            ProviderName::Anthropic,
            ProviderName::Grok,
            ProviderName::Custom("x".into()),
        ];

        for kind in [ProviderKind::Completions, ProviderKind::Responses] {
            for name in &cases {
                let err = ProviderBuilder::new(kind)
                    .provider_name(name.clone())
                    .api_key("k")
                    .build()
                    .err()
                    .unwrap();
                assert!(
                    matches!(err, ProviderError::InvalidProviderConfig(_)),
                    "{kind:?} + {name:?} without base_url should fail"
                );
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

        let err = ProviderBuilder::completions()
            .provider_name(ProviderName::DeepSeek)
            .api_key("k")
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
    fn provider_with_base_url_accepts_extra_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-test", "1".parse().unwrap());

        let provider = ProviderBuilder::responses()
            .provider_name(ProviderName::DeepSeek)
            .api_key("k")
            .base_url("https://example.com")
            .extra_headers(headers)
            .build()
            .unwrap();

        assert_eq!(provider.provider_name(), ProviderName::DeepSeek);
    }

    #[test]
    fn custom_base_url_uses_only_user_models() {
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
    fn model_registry_sets_known_models() {
        let registry = ModelRegistry::from_models([
            ModelInfo::minimal("m1", ProviderName::Custom("gw".into())),
            ModelInfo::minimal("m2", ProviderName::Custom("gw".into())),
        ]);

        let provider = ProviderBuilder::completions()
            .provider_name(ProviderName::Custom("gw".into()))
            .api_key("k")
            .base_url("https://example.com")
            .model_registry(registry)
            .add_model(ModelInfo::minimal("m3", ProviderName::Custom("gw".into())))
            .build()
            .unwrap();

        let mut ids: Vec<String> = provider.known_models().into_iter().map(|m| m.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, ["m1", "m2", "m3"]);
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

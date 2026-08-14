//! `Router`：按模型 ID 把请求派发到已注册 provider 的路由层。
//!
//! `Router` 组合多个 [`Provider`]（例如通过 [`ProviderBuilder`](crate::ProviderBuilder)
//! 构造的 `Box<dyn Provider>`），并以 `Request.model`（[`ModelId`]）为键自动选择
//! 目标 provider。调用方只需维护"模型 → provider"的注册关系，不再需要手动保持
//! provider 与模型同步。
//!
//! 派发优先级（由高到低）：
//! 1. [`Router::alias`] 显式绑定的精确模型 ID；
//! 2. [`Router::route`] 配置的前缀规则（最长前缀优先；等长时先注册的规则优先）；
//! 3. 按注册顺序扫描各 provider 的静态模型目录
//!    （[`Provider::resolve_model`] 命中即归属，先注册者胜出）；
//! 4. 均未命中则返回 [`RouterError::UnknownModel`]。
//!
//! 规则按 [`Provider::provider_name`] 引用 provider，解析发生在派发时，因此
//! `route`/`alias` 可以写在 [`register`](Self::register) 之前或之后；指向未注册
//! provider 名称的规则会被跳过。

use std::cmp::Reverse;
use std::collections::HashMap;

use futures::stream::BoxStream;
use thiserror::Error;

use crate::domain::{ModelId, Request, Response, StreamEvent};
use crate::provider::{Provider, ProviderError, ProviderName};

/// `Router` 派发失败或转发 provider 失败时的错误类型。
#[derive(Debug, Error)]
pub enum RouterError {
    /// 未注册任何 provider。
    #[error("no provider registered")]
    NoProviderRegistered,
    /// 没有任何已注册 provider 或规则匹配该模型。
    #[error("no registered provider or route matches model {0}")]
    UnknownModel(ModelId),
    /// 目标 provider 调用失败（`complete` / `stream` 启动阶段）。
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
}

/// 按 `Request.model` 自动派发的多 provider 路由层。
#[derive(Default)]
pub struct Router {
    /// 按注册顺序保存的 provider。
    providers: Vec<Box<dyn Provider>>,
    /// 精确模型绑定：`模型 ID -> provider 名称`。
    aliases: HashMap<ModelId, ProviderName>,
    /// 前缀规则：`(前缀, provider 名称)`，按注册顺序保存。
    prefixes: Vec<(String, ProviderName)>,
}

impl Router {
    /// 创建空路由。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 provider，返回 `self` 以支持链式调用。
    ///
    /// 注册顺序决定目录扫描的归属：同一模型被多个 provider 的目录命中时，
    /// 先注册者胜出；同名 provider 重复注册时，名称查找取最先注册的那个。
    pub fn register(&mut self, provider: Box<dyn Provider>) -> &mut Self {
        self.providers.push(provider);
        self
    }

    /// 把单个模型 ID 精确绑定到指定名称的 provider。
    ///
    /// 精确绑定优先于前缀规则与目录扫描，且不会匹配该 ID 的扩展形式。
    pub fn alias(&mut self, model: impl Into<ModelId>, provider: &ProviderName) -> &mut Self {
        self.aliases.insert(model.into(), provider.clone());
        self
    }

    /// 添加一条前缀路由规则：所有以 `prefix` 开头的模型 ID 派发到该名称的
    /// provider。
    ///
    /// 规则按 [`Provider::provider_name`] 引用 provider，派发时取最先注册的
    /// 同名 provider；规则指向的 provider 未注册时被跳过。
    pub fn route(&mut self, prefix: impl Into<String>, provider: &ProviderName) -> &mut Self {
        self.prefixes.push((prefix.into(), provider.clone()));
        self
    }

    /// 解析 `model` 对应的 provider。详见模块文档中的派发优先级。
    pub fn provider(&self, model: &ModelId) -> Result<&dyn Provider, RouterError> {
        if self.providers.is_empty() {
            return Err(RouterError::NoProviderRegistered);
        }

        // 1. 精确模型绑定。
        if let Some(name) = self.aliases.get(model)
            && let Some(provider) = self
                .providers
                .iter()
                .find(|provider| provider.provider_name() == *name)
        {
            return Ok(provider.as_ref());
        }

        // 2. 前缀规则：最长匹配优先，等长时先注册的规则优先。
        if let Some((_, (_, name))) = self
            .prefixes
            .iter()
            .enumerate()
            .filter(|(_, (prefix, _))| model.as_str().starts_with(prefix.as_str()))
            .max_by_key(|(index, (prefix, _))| (prefix.len(), Reverse(*index)))
            && let Some(provider) = self
                .providers
                .iter()
                .find(|provider| provider.provider_name() == *name)
        {
            return Ok(provider.as_ref());
        }

        // 3. 静态目录扫描：按注册顺序取首个命中。
        if let Some(provider) = self
            .providers
            .iter()
            .find(|provider| provider.resolve_model(model).is_some())
        {
            return Ok(provider.as_ref());
        }

        Err(RouterError::UnknownModel(model.clone()))
    }

    /// 非流式调用：解析 `req.model` 后转发给目标 provider。
    pub async fn complete(&self, req: &Request) -> Result<Response, RouterError> {
        let provider = self.provider(&req.model)?;
        Ok(provider.complete(req).await?)
    }

    /// 流式调用：解析 `req.model` 后转发给目标 provider。
    ///
    /// 流启动前的派发失败以 `Err` 返回；流启动后的事件错误保持
    /// `ProviderError`，与直接调用 provider 的语义一致。
    pub async fn stream(
        &self,
        req: &Request,
    ) -> Result<BoxStream<'static, Result<StreamEvent, ProviderError>>, RouterError> {
        let provider = self.provider(&req.model)?;
        Ok(provider.stream(req).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use futures::StreamExt;

    use crate::domain::message::Role;
    use crate::provider::model::{ModelCapabilities, ModelInfo};

    fn model_info(id: &str, provider: ProviderName) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            provider,
            context_window: 1000,
            max_output_tokens: 100,
            capabilities: ModelCapabilities::default(),
            pricing: None,
        }
    }

    /// 一个带模型目录、可记录调用并可注入失败的 stub provider。
    struct StubProvider {
        name: ProviderName,
        catalog: HashMap<ModelId, ModelInfo>,
        calls: Arc<Mutex<Vec<ProviderName>>>,
        fail: bool,
    }

    impl StubProvider {
        fn new(name: ProviderName, models: &[&str]) -> Self {
            let catalog = models
                .iter()
                .map(|id| (ModelId::from(*id), model_info(id, name.clone())))
                .collect();
            Self {
                name,
                catalog,
                calls: Arc::new(Mutex::new(Vec::new())),
                fail: false,
            }
        }

        fn failing(name: ProviderName, models: &[&str]) -> Self {
            let mut provider = Self::new(name, models);
            provider.fail = true;
            provider
        }

        fn calls(&self) -> Arc<Mutex<Vec<ProviderName>>> {
            self.calls.clone()
        }
    }

    #[async_trait]
    impl Provider for StubProvider {
        async fn complete(&self, req: &Request) -> Result<Response, ProviderError> {
            if self.fail {
                return Err(ProviderError::Auth("stub auth failure".to_string()));
            }
            self.calls.lock().unwrap().push(self.name.clone());
            Ok(Response {
                id: "stub-response".to_string(),
                model: req.model.as_str().to_owned(),
                role: Role::Assistant,
                content: Vec::new(),
                stop_reason: None,
                usage: None,
            })
        }

        async fn stream(
            &self,
            _req: &Request,
        ) -> Result<BoxStream<'static, Result<StreamEvent, ProviderError>>, ProviderError> {
            if self.fail {
                return Err(ProviderError::Auth("stub auth failure".to_string()));
            }
            self.calls.lock().unwrap().push(self.name.clone());
            let events = futures::stream::iter(vec![Ok(StreamEvent::MessageStop)]);
            Ok(Box::pin(events))
        }

        fn known_models(&self) -> Vec<ModelInfo> {
            self.catalog.values().cloned().collect()
        }

        fn resolve_model(&self, id: &ModelId) -> Option<&ModelInfo> {
            self.catalog.get(id)
        }

        fn provider_name(&self) -> ProviderName {
            self.name.clone()
        }
    }

    fn request(model: &str) -> Request {
        Request::builder().model(model).build().unwrap()
    }

    #[test]
    fn no_provider_registered_is_error() {
        let router = Router::new();
        assert!(matches!(
            router.provider(&ModelId::from("anything")),
            Err(RouterError::NoProviderRegistered)
        ));
    }

    #[test]
    fn catalog_dispatch_picks_matching_provider() {
        let mut router = Router::new();
        router
            .register(Box::new(StubProvider::new(
                ProviderName::DeepSeek,
                &["deepseek-v4-flash"],
            )))
            .register(Box::new(StubProvider::new(
                ProviderName::Zhipu,
                &["glm-5.2"],
            )));

        let provider = router.provider(&ModelId::from("glm-5.2")).unwrap();
        assert_eq!(provider.provider_name(), ProviderName::Zhipu);
    }

    #[test]
    fn first_registered_provider_wins_on_catalog_conflict() {
        let mut router = Router::new();
        router
            .register(Box::new(StubProvider::new(
                ProviderName::DeepSeek,
                &["shared-model"],
            )))
            .register(Box::new(StubProvider::new(
                ProviderName::Zhipu,
                &["shared-model"],
            )));

        let provider = router.provider(&ModelId::from("shared-model")).unwrap();
        assert_eq!(provider.provider_name(), ProviderName::DeepSeek);
    }

    #[test]
    fn alias_overrides_catalog() {
        let mut router = Router::new();
        router
            .register(Box::new(StubProvider::new(
                ProviderName::DeepSeek,
                &["deepseek-v4-flash"],
            )))
            .register(Box::new(StubProvider::new(ProviderName::Zhipu, &[])))
            .alias("deepseek-v4-flash", &ProviderName::Zhipu);

        let provider = router
            .provider(&ModelId::from("deepseek-v4-flash"))
            .unwrap();
        assert_eq!(provider.provider_name(), ProviderName::Zhipu);
    }

    #[test]
    fn alias_is_exact_and_does_not_match_extensions() {
        let mut router = Router::new();
        router
            .register(Box::new(StubProvider::new(
                ProviderName::DeepSeek,
                &["m-1", "m-10"],
            )))
            .register(Box::new(StubProvider::new(ProviderName::Zhipu, &[])))
            .alias("m-1", &ProviderName::Zhipu);

        // 精确 ID 命中别名。
        assert_eq!(
            router
                .provider(&ModelId::from("m-1"))
                .unwrap()
                .provider_name(),
            ProviderName::Zhipu
        );
        // 扩展 ID 不受精确别名影响，退回目录扫描。
        assert_eq!(
            router
                .provider(&ModelId::from("m-10"))
                .unwrap()
                .provider_name(),
            ProviderName::DeepSeek
        );
    }

    #[test]
    fn prefix_rule_beats_catalog() {
        let mut router = Router::new();
        router
            .register(Box::new(StubProvider::new(
                ProviderName::DeepSeek,
                &["glm-5.2"],
            )))
            .register(Box::new(StubProvider::new(
                ProviderName::Zhipu,
                &["glm-5.2"],
            )))
            .route("glm-", &ProviderName::Zhipu);

        let provider = router.provider(&ModelId::from("glm-5.2")).unwrap();
        assert_eq!(provider.provider_name(), ProviderName::Zhipu);
    }

    #[test]
    fn longest_prefix_rule_wins() {
        let mut router = Router::new();
        router
            .register(Box::new(StubProvider::new(ProviderName::DeepSeek, &[])))
            .register(Box::new(StubProvider::new(ProviderName::Zhipu, &[])))
            .route("deepseek-", &ProviderName::DeepSeek)
            .route("deepseek-v4-", &ProviderName::Zhipu);

        let provider = router
            .provider(&ModelId::from("deepseek-v4-flash"))
            .unwrap();
        assert_eq!(provider.provider_name(), ProviderName::Zhipu);
    }

    #[test]
    fn equal_length_prefix_uses_earliest_rule() {
        let mut router = Router::new();
        router
            .register(Box::new(StubProvider::new(ProviderName::DeepSeek, &[])))
            .register(Box::new(StubProvider::new(ProviderName::Zhipu, &[])))
            .route("ab-", &ProviderName::DeepSeek)
            .route("ab-", &ProviderName::Zhipu);

        let provider = router.provider(&ModelId::from("ab-x")).unwrap();
        assert_eq!(provider.provider_name(), ProviderName::DeepSeek);
    }

    #[test]
    fn unknown_model_is_error() {
        let mut router = Router::new();
        router.register(Box::new(StubProvider::new(
            ProviderName::DeepSeek,
            &["deepseek-v4-flash"],
        )));

        let result = router.provider(&ModelId::from("unknown"));
        assert!(matches!(
            result,
            Err(RouterError::UnknownModel(ref model)) if model.as_str() == "unknown"
        ));
    }

    #[test]
    fn route_to_unregistered_provider_is_skipped() {
        let mut router = Router::new();
        router
            .register(Box::new(StubProvider::new(ProviderName::DeepSeek, &[])))
            .route("gpt-", &ProviderName::OpenAI);

        assert!(matches!(
            router.provider(&ModelId::from("gpt-4o")),
            Err(RouterError::UnknownModel(_))
        ));
    }

    #[tokio::test]
    async fn complete_delegates_to_resolved_provider() {
        let mut router = Router::new();
        let deepseek = StubProvider::new(ProviderName::DeepSeek, &["deepseek-v4-flash"]);
        let calls = deepseek.calls();
        router.register(Box::new(deepseek));

        let response = router
            .complete(&request("deepseek-v4-flash"))
            .await
            .unwrap();
        assert_eq!(response.model, "deepseek-v4-flash");
        assert_eq!(*calls.lock().unwrap(), vec![ProviderName::DeepSeek]);
    }

    #[tokio::test]
    async fn stream_delegates_to_resolved_provider() {
        let mut router = Router::new();
        let zhipu = StubProvider::new(ProviderName::Zhipu, &["glm-5.2"]);
        let calls = zhipu.calls();
        router.register(Box::new(zhipu));

        let stream = router.stream(&request("glm-5.2")).await.unwrap();
        let events: Vec<_> = stream.collect().await;
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Ok(StreamEvent::MessageStop)));
        assert_eq!(*calls.lock().unwrap(), vec![ProviderName::Zhipu]);
    }

    #[tokio::test]
    async fn provider_errors_pass_through_complete() {
        let mut router = Router::new();
        router.register(Box::new(StubProvider::failing(
            ProviderName::DeepSeek,
            &["deepseek-v4-flash"],
        )));

        let err = router
            .complete(&request("deepseek-v4-flash"))
            .await
            .unwrap_err();
        assert!(matches!(err, RouterError::Provider(ProviderError::Auth(_))));
    }

    #[tokio::test]
    async fn stream_start_failure_is_an_error_not_a_stream() {
        let mut router = Router::new();
        router.register(Box::new(StubProvider::failing(
            ProviderName::DeepSeek,
            &["deepseek-v4-flash"],
        )));

        let result = router.stream(&request("deepseek-v4-flash")).await;
        assert!(matches!(
            result,
            Err(RouterError::Provider(ProviderError::Auth(_)))
        ));
    }

    #[tokio::test]
    async fn stream_routing_failure_returns_err() {
        let mut router = Router::new();
        router.register(Box::new(StubProvider::new(
            ProviderName::DeepSeek,
            &["deepseek-v4-flash"],
        )));

        let result = router.stream(&request("unknown")).await;
        assert!(matches!(result, Err(RouterError::UnknownModel(_))));
    }
}

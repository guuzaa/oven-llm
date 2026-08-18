//! `Router`：按 `Request.model` 把请求派发到已注册 provider。
//!
//! 派发顺序：
//! 1. slug 的 vendor 段匹配已注册 provider（再按 `:variant` / 目录默认协议选实现）；
//! 2. 各 provider 静态目录（[`Provider::resolve_model`]，先注册者胜出）；
//! 3. 均未命中则返回 [`RouterError::UnknownModel`]。
//!
//! 裸 id 在只注册了一家 vendor 时会先补成 `vendor/wire-id`。

use async_trait::async_trait;
use futures::stream::BoxStream;
use thiserror::Error;

use crate::domain::{ModelId, Request, Response, StreamEvent};
use crate::provider::catalog;
use crate::provider::model::ModelInfo;
use crate::provider::{Provider, ProviderError, ProviderKind, ProviderName};

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

impl From<RouterError> for ProviderError {
    fn from(err: RouterError) -> Self {
        match err {
            RouterError::NoProviderRegistered => ProviderError::NoProviderRegistered,
            RouterError::UnknownModel(model) => ProviderError::UnknownModel(model),
            RouterError::Provider(err) => err,
        }
    }
}

/// 按 `Request.model` 自动派发的多 provider 路由层。
#[derive(Default)]
pub struct Router {
    /// 按注册顺序保存的 provider。
    providers: Vec<Box<dyn Provider>>,
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

    /// 裸 id 在只注册了一家 vendor 时补上前缀；已有 vendor 则规范别名。
    pub fn qualify(&self, model: &ModelId) -> ModelId {
        if model.vendor().is_some() {
            return model.qualify(model.vendor().expect("vendor present"));
        }
        if let Some(vendor) = self.single_vendor_slug() {
            return model.qualify(&vendor);
        }
        model.clone()
    }

    /// 解析 `model` 对应的 provider。详见模块文档中的派发优先级。
    pub fn provider(&self, model: &ModelId) -> Result<&dyn Provider, RouterError> {
        if self.providers.is_empty() {
            return Err(RouterError::NoProviderRegistered);
        }

        let model = self.qualify(model);

        // slug vendor → 已注册 provider，再按协议挑选实现。
        if let Some(vendor) = model.vendor()
            && let Some(provider) = self.provider_for_vendor(vendor, &model)
        {
            return Ok(provider);
        }

        // 静态目录扫描：按注册顺序取首个命中。
        if let Some(provider) = self
            .providers
            .iter()
            .find(|provider| provider.resolve_model(&model).is_some())
        {
            return Ok(provider.as_ref());
        }

        Err(RouterError::UnknownModel(model))
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

    fn single_vendor_slug(&self) -> Option<String> {
        let mut slugs = self
            .providers
            .iter()
            .map(|provider| provider.provider_name().slug().to_string())
            .collect::<Vec<_>>();
        slugs.sort();
        slugs.dedup();
        if slugs.len() == 1 { slugs.pop() } else { None }
    }

    fn provider_for_vendor(&self, vendor: &str, model: &ModelId) -> Option<&dyn Provider> {
        let protocol = self.resolve_protocol(model);
        let matches: Vec<&dyn Provider> = self
            .providers
            .iter()
            .filter(|provider| provider.provider_name().matches_vendor(vendor))
            .map(|provider| provider.as_ref())
            .collect();
        pick_protocol(matches, protocol)
    }

    fn resolve_protocol(&self, model: &ModelId) -> ProviderKind {
        if let Some(variant) = model.variant() {
            return match variant.to_ascii_lowercase().as_str() {
                "responses" => ProviderKind::Responses,
                "messages" => ProviderKind::Messages,
                _ => ProviderKind::Completions,
            };
        }
        if let Some(info) = self.lookup_catalog(model) {
            return info.default_protocol();
        }
        ProviderKind::Completions
    }

    fn lookup_catalog(&self, model: &ModelId) -> Option<ModelInfo> {
        for provider in &self.providers {
            if let Some(info) = provider.resolve_model(model) {
                return Some(info.clone());
            }
        }
        catalog::all_models()
            .into_iter()
            .find(|info| {
                info.id == model.wire_id()
                    && info.provider.matches_vendor(model.vendor().unwrap_or(""))
            })
            .or_else(|| {
                catalog::all_models()
                    .into_iter()
                    .find(|info| info.id == model.wire_id())
            })
    }
}

fn pick_protocol(matches: Vec<&dyn Provider>, protocol: ProviderKind) -> Option<&dyn Provider> {
    if matches.is_empty() {
        return None;
    }
    if let Some(exact) = matches
        .iter()
        .copied()
        .find(|provider| provider.protocol() == Some(protocol))
    {
        return Some(exact);
    }
    if matches.len() == 1 {
        return Some(matches[0]);
    }
    matches
        .iter()
        .copied()
        .find(|provider| provider.protocol().is_none())
        .or_else(|| matches.first().copied())
}

#[async_trait]
impl Provider for Router {
    async fn complete(&self, req: &Request) -> Result<Response, ProviderError> {
        Ok(Router::complete(self, req).await?)
    }

    async fn stream(
        &self,
        req: &Request,
    ) -> Result<BoxStream<'static, Result<StreamEvent, ProviderError>>, ProviderError> {
        Ok(Router::stream(self, req).await?)
    }

    fn known_models(&self) -> Vec<ModelInfo> {
        let mut models = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for provider in &self.providers {
            for mut model in provider.known_models() {
                let slug = model.slug();
                if seen.insert(slug.clone()) {
                    model.id = slug;
                    models.push(model);
                }
            }
        }
        models
    }

    fn resolve_model(&self, id: &ModelId) -> Option<&ModelInfo> {
        let id = self.qualify(id);
        self.providers
            .iter()
            .find_map(|provider| provider.resolve_model(&id))
    }

    fn provider_name(&self) -> ProviderName {
        match self.single_vendor_slug() {
            Some(slug) => ProviderName::from(slug.as_str()),
            None => ProviderName::Custom("router".into()),
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let mut models = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for provider in &self.providers {
            for mut model in provider.list_models().await? {
                let slug = model.slug();
                if seen.insert(slug.clone()) {
                    model.id = slug;
                    models.push(model);
                }
            }
        }
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
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
            protocols: Vec::new(),
        }
    }

    /// 一个带模型目录、可记录调用并可注入失败的 stub provider。
    struct StubProvider {
        name: ProviderName,
        catalog: HashMap<ModelId, ModelInfo>,
        calls: Arc<Mutex<Vec<ProviderName>>>,
        fail: bool,
        protocol: Option<ProviderKind>,
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
                protocol: None,
            }
        }

        fn with_protocol(mut self, protocol: ProviderKind) -> Self {
            self.protocol = Some(protocol);
            self
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

        fn protocol(&self) -> Option<ProviderKind> {
            self.protocol
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
    fn unknown_model_is_error() {
        let mut router = Router::new();
        router.register(Box::new(StubProvider::new(
            ProviderName::DeepSeek,
            &["deepseek-v4-flash"],
        )));

        let result = router.provider(&ModelId::from("xai/unknown"));
        assert!(matches!(
            result,
            Err(RouterError::UnknownModel(ref model)) if model.as_str() == "xai/unknown"
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

        let result = router.stream(&request("xai/grok-4.6")).await;
        assert!(matches!(result, Err(RouterError::UnknownModel(_))));
    }

    #[test]
    fn slug_vendor_dispatches_and_variant_picks_protocol() {
        let mut router = Router::new();
        router
            .register(Box::new(
                StubProvider::new(ProviderName::DeepSeek, &["deepseek-v4-flash"])
                    .with_protocol(ProviderKind::Completions),
            ))
            .register(Box::new(
                StubProvider::new(ProviderName::DeepSeek, &["deepseek-v4-flash"])
                    .with_protocol(ProviderKind::Responses),
            ))
            .register(Box::new(
                StubProvider::new(ProviderName::Grok, &["grok-4.6"])
                    .with_protocol(ProviderKind::Responses),
            ));

        assert_eq!(
            router
                .provider(&ModelId::from("deepseek/deepseek-v4-flash"))
                .unwrap()
                .protocol(),
            Some(ProviderKind::Completions)
        );
        assert_eq!(
            router
                .provider(&ModelId::from("deepseek/deepseek-v4-flash:responses"))
                .unwrap()
                .protocol(),
            Some(ProviderKind::Responses)
        );
        assert_eq!(
            router
                .provider(&ModelId::from("xai/grok-4.6"))
                .unwrap()
                .provider_name(),
            ProviderName::Grok
        );
        assert_eq!(
            router.qualify(&ModelId::from("grok/grok-4.6")).as_str(),
            "xai/grok-4.6"
        );
    }

    #[test]
    fn single_vendor_qualifies_bare_id() {
        let mut router = Router::new();
        router.register(Box::new(StubProvider::new(
            ProviderName::Moonshot,
            &["kimi-k3"],
        )));
        assert_eq!(
            router.qualify(&ModelId::from("kimi-k3")).as_str(),
            "moonshot/kimi-k3"
        );
        assert_eq!(
            router
                .provider(&ModelId::from("kimi-k3"))
                .unwrap()
                .provider_name(),
            ProviderName::Moonshot
        );
    }
}

//! `OpenAICompatProvider`：OpenAI Chat Completions 兼容协议的 `Provider`
//! 实现（传输层）。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中
//! "`OpenAICompatProvider`（`provider.rs`）" 一节，以及需求文档
//! Requirement 6（传输与错误映射）与 Requirement 7（provider_options 直通）。
//!
//! 本模块负责把 domain 层的 `Request`/`Response`/`StreamEvent` 与
//! `encoder`/`decoder` 串联起来，完成实际的 HTTP 调用：
//! - `build_body`：`encoder::encode_request` + `provider_options` 合并
//! - `post_chat_completions`：POST `chat/completions`，把非 2xx 状态码映射为
//!   具体的 `ProviderError` 变体
//! - `Provider::complete` / `Provider::stream`：分别对接非流式响应解码与
//!   基于 `eventsource-stream` 的 SSE 流式解码
//! - `Provider::known_models` / `Provider::list_models`：模型元数据查询

use std::collections::HashMap;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use futures::stream::BoxStream;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};

use super::decoder::{self, StreamDecoder};
use super::encoder;
use super::models::{deepseek_models, moonshot_models, zhipu_models};
use super::types::{ChatCompletionChunk, ChatCompletionResponse};
use crate::domain::{ModelId, Request, Response, StreamEvent};
use crate::provider::model::ModelInfo;
use crate::provider::validate::validate_request;
use crate::provider::{Provider, ProviderError};

/// OpenAI Chat Completions 兼容协议的通用 `Provider` 实现。
///
/// 通过 `base_url` 区分具体服务商（DeepSeek / Moonshot / 智谱 / OpenAI 官方
/// 等），wire 格式（`types.rs`/`encoder.rs`/`decoder.rs`）在这些服务商之间
/// 完全共享。
pub struct OpenAICompatProvider {
    base_url: String,
    provider_name: String,
    api_key: SecretString,
    extra_headers: HeaderMap,
    model_catalog: HashMap<ModelId, ModelInfo>,
    client: reqwest::Client,
}

impl OpenAICompatProvider {
    /// 构造一个不带静态模型元数据、不带额外请求头的 provider。
    pub fn new(
        base_url: impl Into<String>,
        provider_name: impl Into<String>,
        api_key: SecretString,
    ) -> Self {
        Self::with_models(
            base_url,
            provider_name,
            api_key,
            Vec::new(),
            HeaderMap::new(),
        )
    }

    /// 构造一个带静态模型元数据与额外请求头的 provider。
    pub fn with_models(
        base_url: impl Into<String>,
        provider_name: impl Into<String>,
        api_key: SecretString,
        known_models: Vec<ModelInfo>,
        extra_headers: HeaderMap,
    ) -> Self {
        let model_catalog = known_models
            .into_iter()
            .map(|model| (ModelId::from(model.id.as_str()), model))
            .collect();

        Self {
            base_url: base_url.into(),
            provider_name: provider_name.into(),
            api_key,
            extra_headers,
            model_catalog,
            client: reqwest::Client::new(),
        }
    }

    /// DeepSeek 预设：`base_url = https://api.deepseek.com`。
    pub fn deepseek(api_key: SecretString) -> Self {
        Self::with_models(
            "https://api.deepseek.com",
            "deepseek",
            api_key,
            deepseek_models(),
            HeaderMap::new(),
        )
    }

    /// Moonshot（Kimi）预设：`base_url = https://api.moonshot.cn/v1`。
    pub fn moonshot(api_key: SecretString) -> Self {
        Self::with_models(
            "https://api.moonshot.cn/v1",
            "moonshot",
            api_key,
            moonshot_models(),
            HeaderMap::new(),
        )
    }

    /// 智谱 GLM 预设：`base_url = https://open.bigmodel.cn/api/paas/v4`。
    pub fn zhipu(api_key: SecretString) -> Self {
        Self::with_models(
            "https://open.bigmodel.cn/api/paas/v4",
            "zhipu",
            api_key,
            zhipu_models(),
            HeaderMap::new(),
        )
    }

    /// OpenAI 官方预设：`base_url = https://api.openai.com/v1`。
    ///
    /// `models.rs` 目前没有为 OpenAI 官方提供静态模型列表（其模型迭代速度快，
    /// 硬编码容易过期），因此 `known_models()` 返回空列表；调用方应优先使用
    /// `list_models()` 动态发现，或自行通过 `with_models` 传入。
    pub fn openai(api_key: SecretString) -> Self {
        Self::with_models(
            "https://api.openai.com/v1",
            "openai",
            api_key,
            Vec::new(),
            HeaderMap::new(),
        )
    }

    /// 构造本次请求的完整请求头：`Authorization: Bearer <api_key>` +
    /// `Content-Type: application/json`，再合并 `extra_headers`
    /// （`extra_headers` 中的同名头会覆盖前两者）。
    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let auth_value = format!("Bearer {}", self.api_key.expose_secret());
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value)
                .unwrap_or_else(|_| HeaderValue::from_static("Bearer invalid-api-key")),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        for (name, value) in self.extra_headers.iter() {
            headers.insert(name.clone(), value.clone());
        }

        headers
    }

    /// 将 `base_url` 与 `path` 拼接为完整端点 URL，正确处理
    /// `base_url` 末尾和 `path` 开头可能存在/不存在的 `/`。
    fn endpoint(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{base}/{path}")
    }

    /// 将 `Request` 编码为最终发送的 JSON 请求体：先由 `encoder::encode_request`
    /// 产出标准 wire JSON，再把 `req.provider_options` 中的每个键值对合并进去
    /// （Requirement 7.1）。`provider_options` 的键与标准字段重名时会覆盖标准
    /// 字段——这是设计文档中明确允许的“覆盖”语义，而非错误。
    ///
    /// `encoder` 不读取/处理 `provider_options`（Requirement 7.2）；当
    /// `provider_options` 为空时不会产生任何额外字段（Requirement 7.3）。
    fn build_body(&self, req: &Request, stream: bool) -> Result<serde_json::Value, ProviderError> {
        let wire = encoder::encode_request(req, stream)
            .map_err(|err| ProviderError::InvalidRequest(err.to_string()))?;

        let mut body = serde_json::to_value(wire)
            .map_err(|err| ProviderError::InvalidRequest(err.to_string()))?;

        if !req.provider_options.is_empty() {
            let object = body
                .as_object_mut()
                .expect("ChatCompletionRequest always serializes to a JSON object");
            for (key, value) in req.provider_options.iter() {
                object.insert(key.clone(), value.clone());
            }
        }

        Ok(body)
    }

    /// 对静态目录命中的模型执行调用模式相关校验；未命中时按宽松策略透传，
    /// 交由上游服务决定其可用性与能力约束。
    fn validate_known_model(&self, req: &Request, stream: bool) -> Result<(), ProviderError> {
        if let Some(model) = self.resolve_model(&req.model) {
            validate_request(req, model, stream)
                .map_err(|error| ProviderError::InvalidRequest(error.to_string()))?;
        }
        Ok(())
    }

    /// POST `body` 到 `{base_url}/chat/completions`，返回响应的原始文本；
    /// 非 2xx 状态码按 Requirement 6.3/6.4/6.5 映射为具体的 `ProviderError`。
    async fn post_chat_completions(
        &self,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let response = self
            .client
            .post(self.endpoint("chat/completions"))
            .headers(self.build_headers())
            .json(body)
            .send()
            .await?;

        self.check_status(response).await
    }

    /// 检查响应状态码，非 2xx 时读取响应体并映射为具体的 `ProviderError`；
    /// 2xx 时原样返回 `response` 供调用方继续处理（消费其 body/字节流）。
    async fn check_status(
        &self,
        response: reqwest::Response,
    ) -> Result<reqwest::Response, ProviderError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let body = response.text().await.unwrap_or_default();
        match status.as_u16() {
            401 | 403 => Err(ProviderError::Auth(body)),
            429 => Err(ProviderError::RateLimit {
                retry_after_ms: None,
            }),
            other => Err(ProviderError::Api {
                status: other,
                body,
            }),
        }
    }
}

#[async_trait]
impl Provider for OpenAICompatProvider {
    /// 发送一次非流式请求（Requirement 6.1：强制 `stream = false`）。
    async fn complete(&self, req: &Request) -> Result<Response, ProviderError> {
        self.validate_known_model(req, false)?;
        let body = self.build_body(req, false)?;
        let response = self.post_chat_completions(&body).await?;

        let wire: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|err| ProviderError::Decode(err.to_string()))?;

        decoder::decode_response(wire).map_err(|err| ProviderError::Decode(err.to_string()))
    }

    /// 发送一次流式请求（Requirement 6.2：强制 `stream = true`），基于
    /// `eventsource-stream` 解析 SSE 字节流，逐个事件委托给 `StreamDecoder`
    /// （Requirement 5.8：连接在 `[DONE]` 之前终止时产出
    /// `ProviderError::Stream`，不伪造 `MessageStop`）。
    async fn stream(
        &self,
        req: &Request,
    ) -> Result<BoxStream<'static, Result<StreamEvent, ProviderError>>, ProviderError> {
        self.validate_known_model(req, true)?;
        let body = self.build_body(req, true)?;
        let response = self.post_chat_completions(&body).await?;

        let byte_stream = response.bytes_stream();
        let mut sse_stream = byte_stream.eventsource();

        let stream = async_stream::stream! {
            let mut decoder_state = StreamDecoder::new();

            while let Some(event) = sse_stream.next().await {
                let event = match event {
                    Ok(event) => event,
                    Err(err) => {
                        yield Err(ProviderError::Stream(err.to_string()));
                        return;
                    }
                };

                if event.data == "[DONE]" {
                    match decoder_state.finish() {
                        Ok(events) => {
                            for stream_event in events {
                                yield Ok(stream_event);
                            }
                        }
                        Err(err) => {
                            yield Err(ProviderError::Decode(err.to_string()));
                        }
                    }
                    return;
                }

                let chunk: ChatCompletionChunk = match serde_json::from_str(&event.data) {
                    Ok(chunk) => chunk,
                    Err(err) => {
                        yield Err(ProviderError::Decode(err.to_string()));
                        return;
                    }
                };

                match decoder_state.decode_chunk(chunk) {
                    Ok(events) => {
                        for stream_event in events {
                            yield Ok(stream_event);
                        }
                    }
                    Err(err) => {
                        yield Err(ProviderError::Decode(err.to_string()));
                        return;
                    }
                }
            }

            // The loop only exits normally (without an early `return`) when the
            // underlying SSE stream ended without ever seeing `[DONE]`.
            yield Err(ProviderError::Stream(
                "stream ended before [DONE]".to_string(),
            ));
        };

        Ok(stream.boxed())
    }

    /// 该 provider 静态已知的模型列表（构造时传入，不涉及网络）。
    fn known_models(&self) -> Vec<ModelInfo> {
        self.model_catalog.values().cloned().collect()
    }

    /// 通过构造时建立的哈希索引查找静态模型元数据。
    fn resolve_model(&self, id: &ModelId) -> Option<&ModelInfo> {
        self.model_catalog.get(id)
    }

    /// GET `{base_url}/models`，仅填充 `id` 与 `provider` 字段
    /// （Requirement 6.6）。
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let response = self
            .client
            .get(self.endpoint("models"))
            .headers(self.build_headers())
            .send()
            .await?;

        let response = self.check_status(response).await?;

        let body: ModelsListResponse = response
            .json()
            .await
            .map_err(|err| ProviderError::Decode(err.to_string()))?;

        Ok(body
            .data
            .into_iter()
            .map(|entry| ModelInfo::minimal(entry.id, self.provider_name.clone()))
            .collect())
    }
}

/// `GET /models` 响应体的最小反序列化目标：`{"data": [{"id": "..."}, ...]}`。
#[derive(Debug, serde::Deserialize)]
struct ModelsListResponse {
    data: Vec<ModelsListEntry>,
}

/// `ModelsListResponse.data` 中的单个条目：只关心 `id`，其余字段忽略。
#[derive(Debug, serde::Deserialize)]
struct ModelsListEntry {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::message::{ContentBlock, Message};
    use reqwest::header::HeaderName;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn api_key(value: &str) -> SecretString {
        SecretString::new(value.to_string().into())
    }

    fn sample_request() -> Request {
        Request {
            model: ModelId::from("gpt-4"),
            messages: vec![Message::user(vec![ContentBlock::text("hi")])],
            ..Default::default()
        }
    }

    // --- constructors (Requirement 6.1, 6.2) ---

    #[test]
    fn new_sets_base_url_and_provider_name() {
        let provider = OpenAICompatProvider::new("https://example.com", "example", api_key("k"));
        assert_eq!(provider.base_url, "https://example.com");
        assert_eq!(provider.provider_name, "example");
        assert!(provider.known_models().is_empty());
    }

    #[test]
    fn deepseek_preset_has_correct_base_url_and_models() {
        let provider = OpenAICompatProvider::deepseek(api_key("k"));
        assert_eq!(provider.base_url, "https://api.deepseek.com");
        assert_eq!(provider.provider_name, "deepseek");
        assert_eq!(provider.known_models().len(), deepseek_models().len());
    }

    #[test]
    fn moonshot_preset_has_correct_base_url_and_models() {
        let provider = OpenAICompatProvider::moonshot(api_key("k"));
        assert_eq!(provider.base_url, "https://api.moonshot.cn/v1");
        assert_eq!(provider.provider_name, "moonshot");
        assert_eq!(provider.known_models().len(), moonshot_models().len());
    }

    #[test]
    fn zhipu_preset_has_correct_base_url_and_models() {
        let provider = OpenAICompatProvider::zhipu(api_key("k"));
        assert_eq!(provider.base_url, "https://open.bigmodel.cn/api/paas/v4");
        assert_eq!(provider.provider_name, "zhipu");
        assert_eq!(provider.known_models().len(), zhipu_models().len());
    }

    #[test]
    fn openai_preset_has_correct_base_url_and_empty_models() {
        let provider = OpenAICompatProvider::openai(api_key("k"));
        assert_eq!(provider.base_url, "https://api.openai.com/v1");
        assert_eq!(provider.provider_name, "openai");
        assert!(provider.known_models().is_empty());
    }

    // --- build_headers ---

    #[test]
    fn build_headers_includes_authorization_and_content_type() {
        let provider =
            OpenAICompatProvider::new("https://example.com", "example", api_key("secret-token"));
        let headers = provider.build_headers();
        assert_eq!(headers.get(AUTHORIZATION).unwrap(), "Bearer secret-token");
        assert_eq!(headers.get(CONTENT_TYPE).unwrap(), "application/json");
    }

    #[test]
    fn build_headers_merges_extra_headers() {
        let mut extra = HeaderMap::new();
        extra.insert(
            HeaderName::from_static("x-custom"),
            HeaderValue::from_static("value"),
        );
        let provider = OpenAICompatProvider::with_models(
            "https://example.com",
            "example",
            api_key("k"),
            Vec::new(),
            extra,
        );
        let headers = provider.build_headers();
        assert_eq!(headers.get("x-custom").unwrap(), "value");
    }

    // --- endpoint ---

    #[test]
    fn endpoint_joins_base_url_without_trailing_slash() {
        let provider = OpenAICompatProvider::new("https://example.com", "example", api_key("k"));
        assert_eq!(
            provider.endpoint("chat/completions"),
            "https://example.com/chat/completions"
        );
    }

    #[test]
    fn endpoint_handles_trailing_and_leading_slashes() {
        let provider = OpenAICompatProvider::new("https://example.com/", "example", api_key("k"));
        assert_eq!(
            provider.endpoint("/chat/completions"),
            "https://example.com/chat/completions"
        );
    }

    // --- build_body (Requirement 7.1, 7.2, 7.3) ---

    #[test]
    fn build_body_with_empty_provider_options_has_no_extra_fields() {
        let provider = OpenAICompatProvider::new("https://example.com", "example", api_key("k"));
        let req = sample_request();
        let body = provider.build_body(&req, false).unwrap();
        let object = body.as_object().unwrap();
        assert!(!object.contains_key("top_k"));
        assert_eq!(object["model"], "gpt-4");
    }

    #[test]
    fn build_body_merges_provider_options() {
        let provider = OpenAICompatProvider::new("https://example.com", "example", api_key("k"));
        let mut req = sample_request();
        req.provider_options.insert("top_k".to_string(), json!(40));
        let body = provider.build_body(&req, false).unwrap();
        assert_eq!(body["top_k"], 40);
    }

    #[test]
    fn build_body_provider_options_override_standard_field() {
        let provider = OpenAICompatProvider::new("https://example.com", "example", api_key("k"));
        let mut req = sample_request();
        req.sampling.temperature = Some(0.5);
        req.provider_options
            .insert("temperature".to_string(), json!(1.5));
        let body = provider.build_body(&req, false).unwrap();
        assert_eq!(body["temperature"], 1.5);
    }

    // --- status code mapping (Requirement 6.3, 6.4, 6.5) via wiremock ---

    #[tokio::test]
    async fn complete_maps_401_to_auth_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad key"))
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let err = provider.complete(&sample_request()).await.unwrap_err();
        match err {
            ProviderError::Auth(body) => assert_eq!(body, "bad key"),
            other => panic!("expected Auth error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn complete_maps_403_to_auth_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let err = provider.complete(&sample_request()).await.unwrap_err();
        assert!(matches!(err, ProviderError::Auth(_)));
    }

    #[tokio::test]
    async fn complete_maps_429_to_rate_limit_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let err = provider.complete(&sample_request()).await.unwrap_err();
        match err {
            ProviderError::RateLimit { retry_after_ms } => assert_eq!(retry_after_ms, None),
            other => panic!("expected RateLimit error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn complete_maps_other_non_2xx_to_api_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let err = provider.complete(&sample_request()).await.unwrap_err();
        match err {
            ProviderError::Api { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body, "boom");
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    // --- complete forces stream = false ---

    #[tokio::test]
    async fn complete_sends_stream_false_regardless_of_request() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-1",
                "model": "gpt-4",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "hi" },
                    "finish_reason": "stop"
                }]
            })))
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let req = sample_request();

        let response = provider.complete(&req).await.unwrap();
        assert_eq!(response.id, "chatcmpl-1");

        let requests = mock_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let sent_body: serde_json::Value = requests[0].body_json().unwrap();
        assert_eq!(sent_body["stream"], false);
    }

    // --- known_models ---

    #[test]
    fn known_models_returns_configured_list() {
        let models = deepseek_models();
        let provider = OpenAICompatProvider::with_models(
            "https://example.com",
            "example",
            api_key("k"),
            models.clone(),
            HeaderMap::new(),
        );
        assert_eq!(provider.known_models(), models.as_slice());
    }

    // --- list_models (Requirement 6.6) ---

    #[tokio::test]
    async fn list_models_returns_minimal_model_info() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    { "id": "gpt-4", "object": "model" },
                    { "id": "gpt-3.5-turbo", "object": "model" }
                ]
            })))
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-4");
        assert_eq!(models[0].provider, "example");
        assert_eq!(models[0], ModelInfo::minimal("gpt-4", "example"));
        assert_eq!(models[1].id, "gpt-3.5-turbo");
    }

    #[tokio::test]
    async fn list_models_maps_error_status_codes() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let err = provider.list_models().await.unwrap_err();
        assert!(matches!(err, ProviderError::Auth(_)));
    }

    // --- stream forces stream = true, terminates on [DONE] ---

    #[tokio::test]
    async fn stream_sends_stream_true_and_decodes_events_until_done() {
        let mock_server = MockServer::start().await;
        let sse_body = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let req = sample_request();

        let mut event_stream = provider.stream(&req).await.unwrap();
        let mut events = Vec::new();
        while let Some(event) = event_stream.next().await {
            events.push(event.unwrap());
        }

        assert!(matches!(events[0], StreamEvent::MessageStart { .. }));
        assert!(matches!(events.last().unwrap(), StreamEvent::MessageStop));

        let requests = mock_server.received_requests().await.unwrap();
        let sent_body: serde_json::Value = requests[0].body_json().unwrap();
        assert_eq!(sent_body["stream"], true);
    }

    #[tokio::test]
    async fn stream_yields_error_when_connection_ends_before_done() {
        let mock_server = MockServer::start().await;
        let sse_body = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let mut event_stream = provider.stream(&sample_request()).await.unwrap();
        let mut events = Vec::new();
        while let Some(event) = event_stream.next().await {
            events.push(event);
        }

        let last = events.last().unwrap();
        match last {
            Err(ProviderError::Stream(msg)) => {
                assert_eq!(msg, "stream ended before [DONE]");
            }
            other => panic!("expected Stream error, got {other:?}"),
        }
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Ok(StreamEvent::MessageStop)))
        );
    }

    #[tokio::test]
    async fn stream_maps_error_status_codes() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock_server)
            .await;

        let provider = OpenAICompatProvider::new(mock_server.uri(), "example", api_key("k"));
        let result = provider.stream(&sample_request()).await;
        match result {
            Err(ProviderError::RateLimit { .. }) => {}
            Err(other) => panic!("expected RateLimit error, got {other:?}"),
            Ok(_) => panic!("expected an error, got Ok"),
        }
    }
}

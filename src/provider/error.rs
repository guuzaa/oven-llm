//! `ProviderError`：所有 provider 操作的统一错误类型。
//!
//! 参见设计文档 `.kiro/specs/oven-llm-core/design.md` 中
//! "provider 层：`Provider` trait 与错误类型" 一节。

use thiserror::Error;

/// 所有 `Provider` 方法（`complete` / `stream` / `list_models`）共用的错误类型。
#[derive(Debug, Error)]
pub enum ProviderError {
    /// HTTP 请求失败（网络层），由 `reqwest::Error` 自动转换。
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    /// HTTP 401/403，携带响应体。
    #[error("authentication failed: {0}")]
    Auth(String),
    /// HTTP 429。
    #[error("rate limited, retry after {retry_after_ms:?}ms")]
    RateLimit { retry_after_ms: Option<u64> },
    /// JSON 反序列化失败 / `DecodeError` 包装。
    #[error("decode error: {0}")]
    Decode(String),
    /// SSE 解析失败 / 流在 `[DONE]` 前终止。
    #[error("stream protocol error: {0}")]
    Stream(String),
    /// 其他非 2xx 状态码，携带状态码与响应体。
    #[error("api error ({status}): {body}")]
    Api { status: u16, body: String },
    /// 请求发出前的编码失败（例如 `encode_request` 返回 `EncodeError`）。
    #[error("invalid request: {0}")]
    InvalidRequest(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_error_displays_message() {
        let err = ProviderError::Auth("bad token".to_string());
        assert_eq!(err.to_string(), "authentication failed: bad token");
    }

    #[test]
    fn rate_limit_error_displays_retry_after() {
        let err = ProviderError::RateLimit {
            retry_after_ms: Some(1000),
        };
        assert_eq!(err.to_string(), "rate limited, retry after Some(1000)ms");
    }

    #[test]
    fn rate_limit_error_displays_none_retry_after() {
        let err = ProviderError::RateLimit {
            retry_after_ms: None,
        };
        assert_eq!(err.to_string(), "rate limited, retry after Nonems");
    }

    #[test]
    fn api_error_displays_status_and_body() {
        let err = ProviderError::Api {
            status: 500,
            body: "internal error".to_string(),
        };
        assert_eq!(err.to_string(), "api error (500): internal error");
    }

    #[test]
    fn decode_error_displays_message() {
        let err = ProviderError::Decode("unexpected field".to_string());
        assert_eq!(err.to_string(), "decode error: unexpected field");
    }

    #[test]
    fn stream_error_displays_message() {
        let err = ProviderError::Stream("stream ended before [DONE]".to_string());
        assert_eq!(
            err.to_string(),
            "stream protocol error: stream ended before [DONE]"
        );
    }

    #[test]
    fn invalid_request_error_displays_message() {
        let err = ProviderError::InvalidRequest("missing model".to_string());
        assert_eq!(err.to_string(), "invalid request: missing model");
    }
}

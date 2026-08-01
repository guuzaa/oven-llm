//! `ProviderError`：所有 provider 操作的统一错误类型。

use thiserror::Error;

use crate::domain::StreamCollectorError;
use crate::provider::openai_compat::{DecodeError, EncodeError};
use crate::provider::responses::{
    DecodeError as ResponsesDecodeError, EncodeError as ResponsesEncodeError,
};
use crate::provider::validate::ValidationError;

/// `Provider` 方法的便捷 Result 别名。
pub type Result<T> = std::result::Result<T, ProviderError>;

/// 所有 `Provider` 方法（`complete` / `stream` / `list_models`）共用的错误类型。
#[derive(Debug, Error)]
pub enum ProviderError {
    /// HTTP 请求/传输层失败，由 `isahc::Error` 自动转换。
    #[error("transport error: {0}")]
    Transport(#[from] isahc::Error),
    /// 流式 / 响应体读取 I/O 错误。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// HTTP 401/403，携带响应体。
    #[error("authentication failed: {0}")]
    Auth(String),
    /// HTTP 429。
    #[error("rate limited, retry after {retry_after_ms:?}ms")]
    RateLimit { retry_after_ms: Option<u64> },
    /// JSON 反序列化失败 / wire 响应解码错误。
    #[error("decode error")]
    Decode(#[source] DecodeError),
    /// SSE 解析失败 / 流在 `[DONE]` 前终止。
    #[error("stream protocol error")]
    Stream(#[source] StreamCollectorError),
    /// 其他非 2xx 状态码，携带状态码与响应体。
    #[error("api error ({status}): {body}")]
    Api { status: u16, body: String },
    /// 请求发出前的校验失败。
    #[error("invalid request")]
    InvalidRequest(#[source] ValidationError),
    /// 请求编码为 wire 格式失败。
    #[error("encoding error")]
    Encode(#[source] EncodeError),
    /// Responses API 响应解码失败。
    #[error("responses decode error")]
    ResponsesDecode(#[source] ResponsesDecodeError),
    /// Responses API 请求编码失败。
    #[error("responses encoding error")]
    ResponsesEncode(#[source] ResponsesEncodeError),
}

impl From<StreamCollectorError> for ProviderError {
    fn from(err: StreamCollectorError) -> Self {
        ProviderError::Stream(err)
    }
}

impl From<DecodeError> for ProviderError {
    fn from(err: DecodeError) -> Self {
        ProviderError::Decode(err)
    }
}

impl From<EncodeError> for ProviderError {
    fn from(err: EncodeError) -> Self {
        ProviderError::Encode(err)
    }
}

impl From<ResponsesDecodeError> for ProviderError {
    fn from(err: ResponsesDecodeError) -> Self {
        ProviderError::ResponsesDecode(err)
    }
}

impl From<ResponsesEncodeError> for ProviderError {
    fn from(err: ResponsesEncodeError) -> Self {
        ProviderError::ResponsesEncode(err)
    }
}

impl From<ValidationError> for ProviderError {
    fn from(err: ValidationError) -> Self {
        ProviderError::InvalidRequest(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

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
    fn decode_error_display() {
        let inner = DecodeError::MissingChoice;
        let err = ProviderError::Decode(inner);
        assert_eq!(err.to_string(), "decode error");
        assert!(err.source().is_some());
    }

    #[test]
    fn stream_error_display() {
        let inner = StreamCollectorError::Stream("stream ended before [DONE]".to_string());
        let err = ProviderError::Stream(inner);
        assert_eq!(err.to_string(), "stream protocol error");
        assert!(err.source().is_some());
    }

    #[test]
    fn invalid_request_display() {
        let inner = ValidationError::ToolsUnsupported;
        let err = ProviderError::InvalidRequest(inner);
        assert_eq!(err.to_string(), "invalid request");
        assert!(err.source().is_some());
    }

    #[test]
    fn encode_error_display() {
        let inner = EncodeError::InvalidContent("bad".to_string());
        let err = ProviderError::Encode(inner);
        assert_eq!(err.to_string(), "encoding error");
        assert!(err.source().is_some());
    }

    #[test]
    fn responses_decode_error_display() {
        let inner = ResponsesDecodeError::Failed {
            message: "boom".to_string(),
        };
        let err = ProviderError::ResponsesDecode(inner);
        assert_eq!(err.to_string(), "responses decode error");
        assert!(err.source().is_some());
    }

    #[test]
    fn responses_encode_error_display() {
        let inner = ResponsesEncodeError::InvalidContent("bad".to_string());
        let err = ProviderError::ResponsesEncode(inner);
        assert_eq!(err.to_string(), "responses encoding error");
        assert!(err.source().is_some());
    }

    #[test]
    fn error_chain_preserves_source() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let decode_inner = DecodeError::InvalidToolArguments {
            id: "call_1".to_string(),
            source: json_err,
        };
        let err = ProviderError::Decode(decode_inner);

        assert_eq!(err.to_string(), "decode error");
        let source = err.source().unwrap();
        assert!(
            source
                .to_string()
                .contains("invalid tool arguments JSON for tool call call_1")
        );
    }
}

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidRequest,
    Authentication,
    Permission,
    RateLimit,
    Overloaded,
    Upstream,
    Timeout,
    Internal,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ProxyError {
    pub kind: ErrorKind,
    pub message: String,
    pub request_id: Option<String>,
    pub retry_after_seconds: Option<u64>,
    pub is_retryable: bool,
}

impl ProxyError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            request_id: None,
            retry_after_seconds: None,
            is_retryable: false,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidRequest, message)
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn status(&self) -> StatusCode {
        match self.kind {
            ErrorKind::InvalidRequest => StatusCode::BAD_REQUEST,
            ErrorKind::Authentication => StatusCode::UNAUTHORIZED,
            ErrorKind::Permission => StatusCode::FORBIDDEN,
            ErrorKind::RateLimit => StatusCode::TOO_MANY_REQUESTS,
            ErrorKind::Overloaded => StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::Timeout => StatusCode::GATEWAY_TIMEOUT,
            ErrorKind::Upstream => StatusCode::BAD_GATEWAY,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn anthropic_type(&self) -> &'static str {
        match self.kind {
            ErrorKind::InvalidRequest => "invalid_request_error",
            ErrorKind::Authentication => "authentication_error",
            ErrorKind::Permission => "permission_error",
            ErrorKind::RateLimit => "rate_limit_error",
            ErrorKind::Overloaded => "overloaded_error",
            ErrorKind::Timeout | ErrorKind::Upstream | ErrorKind::Internal => "api_error",
        }
    }

    pub fn retryable(&self) -> bool {
        self.is_retryable
    }

    pub fn with_retryable(mut self) -> Self {
        self.is_retryable = true;
        self
    }
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    error: ErrorBody<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<&'a str>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    message: &'a str,
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let status = self.status();
        let mut response = (
            status,
            Json(ErrorEnvelope {
                kind: "error",
                error: ErrorBody {
                    kind: self.anthropic_type(),
                    message: &self.message,
                },
                request_id: self.request_id.as_deref(),
            }),
        )
            .into_response();
        if let Some(seconds) = self.retry_after_seconds {
            if let Ok(value) = seconds.to_string().parse() {
                response.headers_mut().insert("retry-after", value);
            }
        }
        response
    }
}

pub type Result<T> = std::result::Result<T, ProxyError>;

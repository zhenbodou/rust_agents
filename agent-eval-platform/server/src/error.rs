//! 统一错误类型：内部错误 → HTTP 响应的映射（带机器可读 code，ch39 的错误响应约定）

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            ApiError::Db(sqlx::Error::RowNotFound) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            ApiError::Db(_) | ApiError::Internal(_) => {
                tracing::error!(error = %self, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL")
            }
        };
        let message = match status {
            StatusCode::INTERNAL_SERVER_ERROR => "internal server error".to_string(),
            _ => self.to_string(),
        };
        (
            status,
            Json(json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]  Validation(String),
    #[error("unauthorized")]  Unauthorized,
    #[error("forbidden")]     Forbidden,
    #[error("not found")]     NotFound,
    #[error("{0}")]  Conflict(String),
    #[error(transparent)]     Db(#[from] sqlx::Error),
    #[error(transparent)]     Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "VALIDATION_FAILED",
            AppError::Unauthorized => "UNAUTHORIZED",
            AppError::Forbidden    => "FORBIDDEN",
            AppError::NotFound     => "NOT_FOUND",
            AppError::Conflict(_)  => "CONFLICT",
            AppError::Db(_)        => "DB_ERROR",
            AppError::Internal(_)  => "INTERNAL",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Unauthorized  => StatusCode::UNAUTHORIZED,
            AppError::Forbidden     => StatusCode::FORBIDDEN,
            AppError::NotFound      => StatusCode::NOT_FOUND,
            AppError::Conflict(_)   => StatusCode::CONFLICT,
            AppError::Db(e)         => { tracing::error!(?e, "db"); StatusCode::INTERNAL_SERVER_ERROR }
            AppError::Internal(e)   => { tracing::error!(?e, "internal"); StatusCode::INTERNAL_SERVER_ERROR }
        };
        let body = json!({ "error": { "code": self.code(), "message": self.to_string() } });
        (status, Json(body)).into_response()
    }
}
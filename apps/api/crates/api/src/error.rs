use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use sqlx::Error as SqlxError;
use thiserror::Error;
use validator::ValidationErrors;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("internal server error")]
    Internal,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("database error")]
    Database(#[from] SqlxError),
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::Internal | AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
        };

        let body = Json(ErrorBody {
            error: self.to_string(),
        });

        (status, body).into_response()
    }
}

impl From<ValidationErrors> for AppError {
    fn from(errors: ValidationErrors) -> Self {
        AppError::BadRequest(errors.to_string())
    }
}

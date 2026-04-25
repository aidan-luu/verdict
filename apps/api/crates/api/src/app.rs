use axum::{routing::get, Router};

use crate::routes::health::health_handler;

pub fn router() -> Router {
    Router::new().route("/health", get(health_handler))
}

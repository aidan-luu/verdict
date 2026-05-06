use axum::{routing::get, Router};

use crate::routes::events::{create_event_handler, list_events_handler};
use crate::routes::health::health_handler;
use crate::state::AppState;

pub fn router(app_state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route(
            "/events",
            get(list_events_handler).post(create_event_handler),
        )
        .with_state(app_state)
}

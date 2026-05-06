use axum::{routing::get, Router};

use crate::routes::events::{
    create_event_handler, create_forecast_handler, list_events_handler, resolve_event_handler,
};
use crate::routes::health::health_handler;
use crate::routes::scoring::score_summary_handler;
use crate::state::AppState;

pub fn router(app_state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route(
            "/events",
            get(list_events_handler).post(create_event_handler),
        )
        .route(
            "/events/{event_id}/forecasts",
            axum::routing::post(create_forecast_handler),
        )
        .route(
            "/events/{event_id}/resolve",
            axum::routing::post(resolve_event_handler),
        )
        .route("/forecasts/scores/summary", get(score_summary_handler))
        .with_state(app_state)
}

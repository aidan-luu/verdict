use axum::{routing::get, Router};

use crate::routes::admin::override_historical_event_handler;
use crate::routes::events::{
    create_event_handler, create_forecast_handler, get_event_handler,
    ingest_from_fda_briefing_handler, list_events_handler, resolve_event_handler,
};
use crate::routes::health::health_handler;
use crate::routes::reference_class::reference_class_handler;
use crate::routes::scoring::score_summary_handler;
use crate::state::AppState;

pub fn router(app_state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route(
            "/events/from-fda-briefing",
            axum::routing::post(ingest_from_fda_briefing_handler),
        )
        .route(
            "/events",
            get(list_events_handler).post(create_event_handler),
        )
        .route("/events/{event_id}", get(get_event_handler))
        .route(
            "/events/{event_id}/forecasts",
            axum::routing::post(create_forecast_handler),
        )
        .route(
            "/events/{event_id}/resolve",
            axum::routing::post(resolve_event_handler),
        )
        .route(
            "/events/{event_id}/reference_class",
            get(reference_class_handler),
        )
        .route("/forecasts/scores/summary", get(score_summary_handler))
        .route(
            "/admin/historical_events/{historical_event_id}",
            axum::routing::post(override_historical_event_handler),
        )
        .with_state(app_state)
}

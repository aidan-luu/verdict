use axum::Json;
use serde::Serialize;

use crate::error::AppError;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub async fn health_handler() -> Result<Json<HealthResponse>, AppError> {
    Ok(Json(HealthResponse { status: "ok" }))
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use sqlx::postgres::PgPoolOptions;
    use tower::util::ServiceExt;

    use crate::{app::router, state::AppState};

    #[tokio::test]
    async fn health_route_returns_ok_json() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://verdict:verdict@127.0.0.1:5432/verdict")
            .expect("lazy pool should build");
        let app = router(AppState::for_tests(pool));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        assert_eq!(body, r#"{"status":"ok"}"#);
    }
}

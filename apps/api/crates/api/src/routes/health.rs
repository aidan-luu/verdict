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
    use tower::util::ServiceExt;

    use crate::app::router;

    #[tokio::test]
    async fn health_route_returns_ok_json() {
        let app = router();
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

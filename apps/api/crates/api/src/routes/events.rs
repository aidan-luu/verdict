use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::{error::AppError, state::AppState};

#[derive(Debug, Deserialize, Validate)]
pub struct CreateEventRequest {
    #[validate(length(min = 1))]
    pub title: String,
    #[validate(length(min = 1))]
    pub drug_name: String,
    #[validate(length(min = 1))]
    pub sponsor: String,
    #[validate(length(min = 1))]
    pub indication: String,
    pub decision_date: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct ListEventsQuery {
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EventResponse {
    pub id: Uuid,
    pub title: String,
    pub kind: String,
    pub drug_name: String,
    pub sponsor: String,
    pub indication: String,
    pub decision_date: NaiveDate,
    pub status: String,
}

pub async fn create_event_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<EventResponse>), AppError> {
    payload.validate()?;

    let event = sqlx::query_as!(
        EventResponse,
        r#"
        INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status)
        VALUES ($1, 'fda_pdufa', $2, $3, $4, $5, 'upcoming')
        RETURNING id, title, kind, drug_name, sponsor, indication, decision_date, status
        "#,
        payload.title,
        payload.drug_name,
        payload.sponsor,
        payload.indication,
        payload.decision_date
    )
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(event)))
}

pub async fn list_events_handler(
    State(state): State<AppState>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<Vec<EventResponse>>, AppError> {
    if let Some(status) = query.status {
        validate_status(&status)?;
        let events = sqlx::query_as!(
            EventResponse,
            r#"
            SELECT id, title, kind, drug_name, sponsor, indication, decision_date, status
            FROM events
            WHERE status = $1
            ORDER BY decision_date ASC
            "#,
            status
        )
        .fetch_all(&state.pool)
        .await?;

        return Ok(Json(events));
    }

    let events = sqlx::query_as!(
        EventResponse,
        r#"
        SELECT id, title, kind, drug_name, sponsor, indication, decision_date, status
        FROM events
        ORDER BY decision_date ASC
        "#
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(events))
}

fn validate_status(status: &str) -> Result<(), AppError> {
    if matches!(status, "upcoming" | "resolved" | "voided") {
        return Ok(());
    }

    Err(AppError::BadRequest("invalid status filter".to_string()))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use sqlx::PgPool;
    use tower::util::ServiceExt;

    use crate::{app::router, state::AppState};

    #[sqlx::test(migrations = "../../migrations")]
    async fn create_event_rejects_empty_title(pool: PgPool) {
        let app = router(AppState { pool });
        let request = Request::builder()
            .method("POST")
            .uri("/events")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"title":"","drug_name":"Drug","sponsor":"Sponsor","indication":"Use","decision_date":"2026-12-01"}"#,
            ))
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn create_event_returns_created_payload(pool: PgPool) {
        let app = router(AppState { pool });
        let request = Request::builder()
            .method("POST")
            .uri("/events")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"title":"Drug X PDUFA","drug_name":"Drug X","sponsor":"Acme","indication":"Condition","decision_date":"2026-12-01"}"#,
            ))
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_events_filters_status(pool: PgPool) {
        sqlx::query!(
            r#"
            INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status)
            VALUES
              ('A', 'fda_pdufa', 'Drug A', 'Sponsor A', 'Indication A', '2026-01-01', 'upcoming'),
              ('B', 'fda_pdufa', 'Drug B', 'Sponsor B', 'Indication B', '2026-02-01', 'resolved')
            "#
        )
        .execute(&pool)
        .await
        .expect("seed should succeed");

        let app = router(AppState { pool });
        let request = Request::builder()
            .uri("/events?status=upcoming")
            .body(Body::empty())
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        let status = response.status();
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("response body should collect")
            .to_bytes();
        let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("\"title\":\"A\""));
        assert!(!body.contains("\"title\":\"B\""));
    }
}

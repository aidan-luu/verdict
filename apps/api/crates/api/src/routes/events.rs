use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
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

#[derive(Debug, Deserialize, Validate)]
pub struct CreateForecastRequest {
    #[validate(custom(function = "validate_probability_range"))]
    pub probability: Decimal,
    #[validate(length(min = 1))]
    pub rationale: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResolveEventRequest {
    #[validate(custom(function = "validate_resolution_outcome"))]
    pub outcome: String,
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

#[derive(Debug, Serialize)]
pub struct ForecastResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub event_id: Uuid,
    pub probability: Decimal,
    pub rationale: String,
}

#[derive(Debug, Serialize)]
pub struct ResolveEventResponse {
    pub id: Uuid,
    pub status: String,
    pub outcome: Option<String>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

// TODO(P1): replace this stub user with Clerk JWT-derived user identity.
const STUB_USER_ID: Uuid = Uuid::from_u128(0x00000000000040008000000000000001);

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

pub async fn create_forecast_handler(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(payload): Json<CreateForecastRequest>,
) -> Result<(StatusCode, Json<ForecastResponse>), AppError> {
    payload.validate()?;

    let event_status = sqlx::query!(
        r#"
        SELECT status
        FROM events
        WHERE id = $1
        "#,
        event_id
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(event_status) = event_status else {
        return Err(AppError::NotFound("event not found".to_string()));
    };

    if event_status.status != "upcoming" {
        return Err(AppError::Conflict(
            "cannot create forecast for non-upcoming event".to_string(),
        ));
    }

    let forecast = sqlx::query_as!(
        ForecastResponse,
        r#"
        INSERT INTO forecasts (user_id, event_id, probability, rationale)
        VALUES ($1, $2, $3, $4)
        RETURNING id, user_id, event_id, probability, rationale
        "#,
        STUB_USER_ID,
        event_id,
        payload.probability,
        payload.rationale
    )
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(forecast)))
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

pub async fn resolve_event_handler(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(payload): Json<ResolveEventRequest>,
) -> Result<Json<ResolveEventResponse>, AppError> {
    payload.validate()?;

    let event_status = sqlx::query!(
        r#"
        SELECT status
        FROM events
        WHERE id = $1
        "#,
        event_id
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(event_status) = event_status else {
        return Err(AppError::NotFound("event not found".to_string()));
    };

    if event_status.status != "upcoming" {
        return Err(AppError::Conflict(
            "cannot resolve event that is not upcoming".to_string(),
        ));
    }

    let (next_status, outcome_value): (&str, Option<&str>) = match payload.outcome.as_str() {
        "approved" | "rejected" => ("resolved", Some(payload.outcome.as_str())),
        "voided" => ("voided", None),
        _ => {
            return Err(AppError::BadRequest(
                "invalid resolution outcome".to_string(),
            ))
        }
    };

    let resolved_event = sqlx::query_as!(
        ResolveEventResponse,
        r#"
        UPDATE events
        SET status = $2, outcome = $3, resolved_at = now()
        WHERE id = $1
        RETURNING id, status, outcome, resolved_at
        "#,
        event_id,
        next_status,
        outcome_value
    )
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(resolved_event))
}

fn validate_status(status: &str) -> Result<(), AppError> {
    if matches!(status, "upcoming" | "resolved" | "voided") {
        return Ok(());
    }

    Err(AppError::BadRequest("invalid status filter".to_string()))
}

fn validate_probability_range(value: &Decimal) -> Result<(), validator::ValidationError> {
    if *value >= Decimal::ZERO && *value <= Decimal::ONE {
        return Ok(());
    }

    Err(validator::ValidationError::new("probability_range"))
}

fn validate_resolution_outcome(value: &str) -> Result<(), validator::ValidationError> {
    if matches!(value, "approved" | "rejected" | "voided") {
        return Ok(());
    }

    Err(validator::ValidationError::new("resolution_outcome"))
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

    #[sqlx::test(migrations = "../../migrations")]
    async fn create_forecast_returns_not_found_for_unknown_event(pool: PgPool) {
        let app = router(AppState { pool });
        let request = Request::builder()
            .method("POST")
            .uri("/events/11111111-1111-4111-8111-111111111111/forecasts")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"probability":"0.7000","rationale":"test"}"#))
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn create_forecast_returns_conflict_for_non_upcoming_event(pool: PgPool) {
        let event_id = sqlx::query!(
            r#"
            INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status)
            VALUES ('A', 'fda_pdufa', 'Drug A', 'Sponsor A', 'Indication A', '2026-01-01', 'resolved')
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("seed should succeed")
        .id;

        let app = router(AppState { pool });
        let request = Request::builder()
            .method("POST")
            .uri(format!("/events/{event_id}/forecasts"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"probability":"0.7000","rationale":"test"}"#))
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn create_forecast_returns_created_for_upcoming_event(pool: PgPool) {
        let event_id = sqlx::query!(
            r#"
            INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status)
            VALUES ('A', 'fda_pdufa', 'Drug A', 'Sponsor A', 'Indication A', '2026-01-01', 'upcoming')
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("seed should succeed")
        .id;

        let app = router(AppState { pool });
        let request = Request::builder()
            .method("POST")
            .uri(format!("/events/{event_id}/forecasts"))
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"probability":"0.7000","rationale":"forecast rationale"}"#,
            ))
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn resolve_event_returns_not_found_for_unknown_event(pool: PgPool) {
        let app = router(AppState { pool });
        let request = Request::builder()
            .method("POST")
            .uri("/events/11111111-1111-4111-8111-111111111111/resolve")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"outcome":"approved"}"#))
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn resolve_event_returns_conflict_for_non_upcoming_event(pool: PgPool) {
        let event_id = sqlx::query!(
            r#"
            INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status)
            VALUES ('A', 'fda_pdufa', 'Drug A', 'Sponsor A', 'Indication A', '2026-01-01', 'resolved')
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("seed should succeed")
        .id;

        let app = router(AppState { pool });
        let request = Request::builder()
            .method("POST")
            .uri(format!("/events/{event_id}/resolve"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"outcome":"approved"}"#))
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn resolve_event_returns_resolved_for_approved(pool: PgPool) {
        let event_id = sqlx::query!(
            r#"
            INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status)
            VALUES ('A', 'fda_pdufa', 'Drug A', 'Sponsor A', 'Indication A', '2026-01-01', 'upcoming')
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("seed should succeed")
        .id;

        let app = router(AppState { pool });
        let request = Request::builder()
            .method("POST")
            .uri(format!("/events/{event_id}/resolve"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"outcome":"approved"}"#))
            .expect("request should build");

        let response = app.oneshot(request).await.expect("request should run");
        let status = response.status();
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("response body should collect")
            .to_bytes();
        let body = String::from_utf8(bytes.to_vec()).expect("utf8 body");

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("\"status\":\"resolved\""));
        assert!(body.contains("\"outcome\":\"approved\""));
    }
}

use axum::{extract::State, Json};
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use crate::{error::AppError, scoring, state::AppState};

#[derive(Debug, Serialize)]
pub struct ScoreContribution {
    pub forecast_id: Uuid,
    pub event_id: Uuid,
    pub probability: Decimal,
    pub occurred: bool,
    pub brier_contribution: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ScoreSummaryResponse {
    pub resolved_forecast_count: u64,
    pub total_brier: Decimal,
    pub mean_brier: Decimal,
    pub contributions: Vec<ScoreContribution>,
}

pub async fn score_summary_handler(
    State(state): State<AppState>,
) -> Result<Json<ScoreSummaryResponse>, AppError> {
    let rows = sqlx::query!(
        r#"
        SELECT
            f.id AS forecast_id,
            f.event_id,
            f.probability,
            (e.outcome = 'approved') AS "occurred!"
        FROM forecasts f
        JOIN events e ON e.id = f.event_id
        WHERE e.status = 'resolved' AND e.outcome IS NOT NULL
        ORDER BY f.created_at ASC
        "#
    )
    .fetch_all(&state.pool)
    .await?;

    let contributions: Vec<ScoreContribution> = rows
        .into_iter()
        .map(|row| {
            let brier_contribution = scoring::brier_contribution(row.probability, row.occurred);
            ScoreContribution {
                forecast_id: row.forecast_id,
                event_id: row.event_id,
                probability: row.probability,
                occurred: row.occurred,
                brier_contribution,
            }
        })
        .collect();

    let brier_values: Vec<Decimal> = contributions
        .iter()
        .map(|item| item.brier_contribution)
        .collect();
    let total_brier: Decimal = brier_values.iter().copied().sum();
    let mean_brier = scoring::mean_brier(&brier_values);

    Ok(Json(ScoreSummaryResponse {
        resolved_forecast_count: contributions.len() as u64,
        total_brier,
        mean_brier,
        contributions,
    }))
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
    async fn score_summary_returns_hand_computed_brier(pool: PgPool) {
        let event_a = sqlx::query!(
            r#"
            INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status, outcome, resolved_at)
            VALUES ('A', 'fda_pdufa', 'Drug A', 'Sponsor A', 'Indication A', '2026-01-01', 'resolved', 'approved', now())
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("seed event a should succeed")
        .id;

        let event_b = sqlx::query!(
            r#"
            INSERT INTO events (title, kind, drug_name, sponsor, indication, decision_date, status, outcome, resolved_at)
            VALUES ('B', 'fda_pdufa', 'Drug B', 'Sponsor B', 'Indication B', '2026-02-01', 'resolved', 'rejected', now())
            RETURNING id
            "#
        )
        .fetch_one(&pool)
        .await
        .expect("seed event b should succeed")
        .id;

        sqlx::query!(
            r#"
            INSERT INTO forecasts (user_id, event_id, probability, rationale)
            VALUES
              ('00000000-0000-4000-8000-000000000001', $1, 0.7000, 'A'),
              ('00000000-0000-4000-8000-000000000001', $2, 0.2000, 'B')
            "#,
            event_a,
            event_b
        )
        .execute(&pool)
        .await
        .expect("seed forecasts should succeed");

        let app = router(AppState::for_tests(pool));
        let request = Request::builder()
            .uri("/forecasts/scores/summary")
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
        assert!(body.contains("\"resolved_forecast_count\":2"));
        assert!(body.contains("\"total_brier\":\"0.13000000\""));
        assert!(body.contains("\"mean_brier\":\"0.06500000\""));
    }
}

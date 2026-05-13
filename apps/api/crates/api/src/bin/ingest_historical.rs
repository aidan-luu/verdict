//! Phase 3 PR A binary: ingest the openFDA `drug/drugsfda` universe of
//! original NDA/BLA approvals into the local `historical_event` table.
//!
//! Usage:
//!   cargo run --bin ingest_historical
//!   cargo run --bin ingest_historical -- --from-date 2010-01-01 --to-date 2026-12-31
//!   cargo run --bin ingest_historical -- --page-limit 200
//!
//! Re-running is safe: the upsert is keyed on `application_number` and
//! enriched fields on existing rows are preserved.

use std::path::Path;
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use verdict_api::services::historical_event_repo::{upsert_from_openfda, UpsertOutcome};
use verdict_api::services::openfda::{
    map_record, ApprovalWindow, DrugsFdaPage, MapOutcome, OpenFdaClient, OpenFdaConfig, SkipReason,
    MAX_SKIP, PAGE_LIMIT,
};

#[derive(Debug)]
struct CliArgs {
    from_date: NaiveDate,
    to_date: NaiveDate,
    page_limit: u32,
}

impl CliArgs {
    fn defaults() -> Self {
        Self {
            from_date: NaiveDate::from_ymd_opt(2010, 1, 1).expect("2010-01-01 is valid"),
            to_date: Utc::now().date_naive(),
            page_limit: 100,
        }
    }
}

fn parse_cli_args() -> Result<CliArgs, String> {
    let mut args = CliArgs::defaults();
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = raw.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--from-date" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--from-date requires a value".to_string())?;
                args.from_date = NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                    .map_err(|_| format!("--from-date must be YYYY-MM-DD, got {value:?}"))?;
            }
            "--to-date" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--to-date requires a value".to_string())?;
                args.to_date = NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                    .map_err(|_| format!("--to-date must be YYYY-MM-DD, got {value:?}"))?;
            }
            "--page-limit" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--page-limit requires a value".to_string())?;
                let parsed = value.parse::<u32>().map_err(|_| {
                    format!("--page-limit must be a positive integer, got {value:?}")
                })?;
                if parsed == 0 || parsed > PAGE_LIMIT {
                    return Err(format!(
                        "--page-limit must be between 1 and {PAGE_LIMIT}, got {parsed}"
                    ));
                }
                args.page_limit = parsed;
            }
            "--help" | "-h" => {
                eprintln!(
                    "ingest_historical [--from-date YYYY-MM-DD] [--to-date YYYY-MM-DD] [--page-limit N (<= {PAGE_LIMIT})]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if args.from_date > args.to_date {
        return Err(format!(
            "--from-date ({from}) must be on or before --to-date ({to})",
            from = args.from_date,
            to = args.to_date,
        ));
    }
    Ok(args)
}

#[derive(Default, Debug)]
struct IngestCounters {
    pages_fetched: u64,
    records_seen: u64,
    inserted: u64,
    updated: u64,
    skipped_orig_missing: u64,
    skipped_date_window: u64,
    skipped_unsupported_type: u64,
    skipped_invalid_date: u64,
    skipped_missing_drug: u64,
    skipped_missing_sponsor: u64,
    errors: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::from_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env")).ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = match parse_cli_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}");
            std::process::exit(2);
        }
    };

    let database_url = std::env::var("DATABASE_URL")?;
    let openfda_config = OpenFdaConfig::from_env()?;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let client = OpenFdaClient::new(openfda_config, http);

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;

    let window = ApprovalWindow {
        from: cli.from_date,
        to: cli.to_date,
    };
    info!(
        from = %window.from,
        to = %window.to,
        page_limit = cli.page_limit,
        "starting openFDA drugsfda ingestion"
    );

    let counters = run_ingestion(&client, &pool, window, cli.page_limit).await;
    info!(?counters, "ingestion complete");
    Ok(())
}

async fn run_ingestion(
    client: &OpenFdaClient,
    pool: &PgPool,
    window: ApprovalWindow,
    page_limit: u32,
) -> IngestCounters {
    let mut counters = IngestCounters::default();
    let mut skip: u32 = 0;

    loop {
        let page = match client.search_drugsfda(window, skip, page_limit).await {
            Ok(page) => page,
            Err(error) => {
                error!(%error, skip, "openFDA page fetch failed; aborting");
                counters.errors += 1;
                return counters;
            }
        };
        counters.pages_fetched += 1;

        let total = page
            .meta
            .as_ref()
            .and_then(|meta| meta.results.as_ref())
            .map(|results| results.total);
        info!(skip, returned = page.results.len(), total = ?total, "fetched page");

        if page.results.is_empty() {
            break;
        }

        let page_size = page.results.len() as u32;
        process_page(pool, &page, window, &mut counters).await;

        skip = skip.saturating_add(page_size);
        if skip > MAX_SKIP || skip >= total.unwrap_or(u32::MAX) {
            break;
        }

        tokio::time::sleep(client.page_delay()).await;
    }

    counters
}

async fn process_page(
    pool: &PgPool,
    page: &DrugsFdaPage,
    window: ApprovalWindow,
    counters: &mut IngestCounters,
) {
    for record in &page.results {
        counters.records_seen += 1;
        let outcome = map_record(record, window);
        match outcome {
            MapOutcome::Insert(row) => {
                // Preserve the raw record for future re-processing.
                let raw = serde_json::to_value(record).ok();
                match upsert_from_openfda(pool, &row, raw.as_ref()).await {
                    Ok((_, UpsertOutcome::Inserted)) => counters.inserted += 1,
                    Ok((_, UpsertOutcome::Updated)) => counters.updated += 1,
                    Err(error) => {
                        warn!(
                            application_number = %row.application_number,
                            %error,
                            "upsert failed"
                        );
                        counters.errors += 1;
                    }
                }
            }
            MapOutcome::Skipped(reason) => match reason {
                SkipReason::NoOriginalApproval => counters.skipped_orig_missing += 1,
                SkipReason::DateOutOfWindow { .. } => counters.skipped_date_window += 1,
                SkipReason::UnsupportedApplicationType { .. } => {
                    counters.skipped_unsupported_type += 1
                }
                SkipReason::InvalidApprovalDate(_) => counters.skipped_invalid_date += 1,
                SkipReason::MissingDrugName => counters.skipped_missing_drug += 1,
                SkipReason::MissingSponsor => counters.skipped_missing_sponsor += 1,
            },
        }
    }
}

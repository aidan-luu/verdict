use sqlx::{
    migrate::{MigrateError, Migrator},
    postgres::PgPoolOptions,
    PgPool,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
const DEBUG_LOG_PATH: &str = "/Users/nhan-tuanaidanluu/Downloads/verdict/.cursor/debug-1f3328.log";
const DEBUG_SESSION_ID: &str = "1f3328";

fn write_debug_log(run_id: &str, hypothesis_id: &str, location: &str, message: &str, data: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64);
    let payload = format!(
        "{{\"sessionId\":\"{session}\",\"runId\":\"{run_id}\",\"hypothesisId\":\"{hypothesis_id}\",\"location\":\"{location}\",\"message\":\"{message}\",\"data\":{data},\"timestamp\":{timestamp}}}\n",
        session = DEBUG_SESSION_ID
    );

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(DEBUG_LOG_PATH)
    {
        let _ = file.write_all(payload.as_bytes());
    }
}

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    // #region agent log
    write_debug_log(
        "pre-fix",
        "H1",
        "db.rs:connect",
        "attempting database connection",
        "{\"databaseUrlConfigured\":true}",
    );
    // #endregion
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), MigrateError> {
    // #region agent log
    write_debug_log(
        "pre-fix",
        "H2",
        "db.rs:run_migrations",
        "starting migration run",
        "{\"migrationsPath\":\"../../migrations\"}",
    );
    // #endregion
    // TODO(P4): revisit migration-on-startup for production deploys.
    let result = MIGRATOR.run(pool).await;
    // #region agent log
    write_debug_log(
        "pre-fix",
        "H3",
        "db.rs:run_migrations",
        "migration run finished",
        &format!("{{\"ok\":{}}}", result.is_ok()),
    );
    // #endregion
    result
}

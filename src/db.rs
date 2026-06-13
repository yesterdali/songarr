use std::path::Path;
use std::str::FromStr;

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

/// Open (creating if missing) the SQLite database and run embedded migrations.
pub async fn init(db_path: &Path) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating database directory {}", parent.display()))?;
    }

    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))
        .with_context(|| format!("invalid database path {}", db_path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .with_context(|| format!("opening database {}", db_path.display()))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("running database migrations")?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_creates_db_and_migrates() {
        let dir = std::env::temp_dir().join(format!("songarr-test-{}", uuid::Uuid::new_v4()));
        let pool = init(&dir.join("songarr.db")).await.unwrap();

        for table in [
            "virtual_tracks",
            "stream_jobs",
            "pending_actions",
            "upgrade_requests",
            "lyrics_cache",
        ] {
            let found: Option<String> =
                sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name=?")
                    .bind(table)
                    .fetch_optional(&pool)
                    .await
                    .unwrap();
            assert_eq!(found.as_deref(), Some(table), "missing table {table}");
        }

        pool.close().await;
        std::fs::remove_dir_all(&dir).ok();
    }
}

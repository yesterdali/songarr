//! stream_jobs persistence.

use sqlx::SqlitePool;

use crate::vtrack::now_utc;

pub async fn create(
    pool: &SqlitePool,
    virtual_track_id: &str,
    requested_by: &str,
) -> sqlx::Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    sqlx::query(
        "INSERT INTO stream_jobs (id, virtual_track_id, requested_by, status, created_at, updated_at)
         VALUES (?, ?, ?, 'resolving', ?, ?)",
    )
    .bind(&id)
    .bind(virtual_track_id)
    .bind(requested_by)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn set_resolution(
    pool: &SqlitePool,
    id: &str,
    source_url: &str,
    match_score: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE stream_jobs SET source_url = ?, match_score = ?, status = 'piping', updated_at = ?
         WHERE id = ?",
    )
    .bind(source_url)
    .bind(match_score)
    .bind(now_utc())
    .bind(id)
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn set_staging_path(pool: &SqlitePool, id: &str, path: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE stream_jobs SET staging_path = ?, updated_at = ? WHERE id = ?")
        .bind(path)
        .bind(now_utc())
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
}

pub async fn set_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    error: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE stream_jobs SET status = ?, error = ?, updated_at = ? WHERE id = ?")
        .bind(status)
        .bind(error)
        .bind(now_utc())
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
}

//! pending_actions: scrobbles/stars received for virtual ids, stored with
//! the original request's auth params so M4 can replay them as the user
//! against the real id after import (Subsonic tokens never expire).

use sqlx::SqlitePool;

use crate::vtrack::now_utc;

pub async fn store(
    pool: &SqlitePool,
    virtual_track_id: &str,
    username: &str,
    action: &str,
    payload_json: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO pending_actions (id, virtual_track_id, username, action, payload_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(virtual_track_id)
    .bind(username)
    .bind(action)
    .bind(payload_json)
    .bind(now_utc())
    .execute(pool)
    .await
    .map(|_| ())
}

#[derive(Debug, sqlx::FromRow)]
pub struct PendingAction {
    pub id: String,
    pub virtual_track_id: String,
    pub username: String,
    pub action: String,
    pub payload_json: Option<String>,
}

pub async fn delete(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM pending_actions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
}

pub async fn for_track(
    pool: &SqlitePool,
    virtual_track_id: &str,
) -> sqlx::Result<Vec<PendingAction>> {
    sqlx::query_as(
        "SELECT id, virtual_track_id, username, action, payload_json
         FROM pending_actions WHERE virtual_track_id = ? ORDER BY created_at",
    )
    .bind(virtual_track_id)
    .fetch_all(pool)
    .await
}

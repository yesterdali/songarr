use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

/// A user's stored Songarr/Subsonic credentials. We keep the per-link salt and
/// derived token (md5(password+salt)) rather than the raw password — Subsonic
/// tokens don't expire, so this is reusable forever and avoids storing secrets.
#[derive(Clone)]
pub struct Link {
    pub server_url: String,
    pub username: String,
    pub salt: String,
    pub token: String,
}

pub async fn init(path: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS links (
            discord_id TEXT PRIMARY KEY,
            server_url TEXT NOT NULL,
            username   TEXT NOT NULL,
            salt       TEXT NOT NULL,
            token      TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

pub async fn get_link(pool: &SqlitePool, discord_id: u64) -> anyhow::Result<Option<Link>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT server_url, username, salt, token FROM links WHERE discord_id = ?",
    )
    .bind(discord_id.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(server_url, username, salt, token)| Link {
        server_url,
        username,
        salt,
        token,
    }))
}

pub async fn set_link(pool: &SqlitePool, discord_id: u64, link: &Link) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO links (discord_id, server_url, username, salt, token)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(discord_id) DO UPDATE SET
            server_url = excluded.server_url,
            username = excluded.username,
            salt = excluded.salt,
            token = excluded.token",
    )
    .bind(discord_id.to_string())
    .bind(&link.server_url)
    .bind(&link.username)
    .bind(&link.salt)
    .bind(&link.token)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_link(pool: &SqlitePool, discord_id: u64) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM links WHERE discord_id = ?")
        .bind(discord_id.to_string())
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

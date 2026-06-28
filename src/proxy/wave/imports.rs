use super::*;

pub async fn imports_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let (_username, _, _) = match authenticated(&state, &params).await {
        Ok(auth) => auth,
        Err(response) => return response,
    };
    let limit = requested_import_limit(params.get("limit").map(String::as_str));
    match recent_import_jobs(&state, limit).await {
        Ok(jobs) => Json(ImportJobsResponse { jobs }).into_response(),
        Err(error) => {
            tracing::warn!(%error, "wave imports failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "imports failed").into_response()
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportJobsResponse {
    jobs: Vec<ImportJob>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportJob {
    id: String,
    track_id: String,
    title: String,
    artist: String,
    album: Option<String>,
    provider: String,
    status: String,
    requested_by: String,
    match_score: Option<i64>,
    error: Option<String>,
    real_subsonic_id: Option<String>,
    created_at: String,
    updated_at: String,
}

async fn recent_import_jobs(state: &AppState, limit: i64) -> sqlx::Result<Vec<ImportJob>> {
    sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            Option<i64>,
            Option<String>,
            Option<String>,
            String,
            String,
        ),
    >(
        "SELECT
            sj.id,
            sj.virtual_track_id,
            vt.title,
            vt.artist,
            vt.album,
            vt.provider,
            sj.status,
            sj.requested_by,
            sj.match_score,
            sj.error,
            vt.real_subsonic_id,
            sj.created_at,
            sj.updated_at
         FROM stream_jobs sj
         JOIN virtual_tracks vt ON vt.id = sj.virtual_track_id
         ORDER BY sj.updated_at DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(
                |(
                    id,
                    track_id,
                    title,
                    artist,
                    album,
                    provider,
                    status,
                    requested_by,
                    match_score,
                    error,
                    real_subsonic_id,
                    created_at,
                    updated_at,
                )| ImportJob {
                    id,
                    track_id,
                    title,
                    artist,
                    album,
                    provider,
                    status,
                    requested_by,
                    match_score,
                    error,
                    real_subsonic_id,
                    created_at,
                    updated_at,
                },
            )
            .collect()
    })
}

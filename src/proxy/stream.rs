//! stream/download for virtual ids: resolve a YouTube source, then pipe
//!
//!   yt-dlp -f bestaudio -o - <url> ──tee──► staging/<job>.src   (original)
//!                                   └─────► ffmpeg → opus/mp3 → client
//!
//! The response is `200` + chunked with `Accept-Ranges: none` (length is
//! unknowable; Range headers are deliberately ignored). If the client
//! disconnects after enough bytes were staged, the acquisition continues in
//! the background — the user pressed play, so we finish the download.

use std::process::Stdio;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::config::StreamFormat;
use crate::subsonic::{auth, error_not_found, Format};
use crate::vtrack::{self, VirtualTrack};
use crate::{jobs, AppState};

use super::passthrough;

/// Disconnect grace: keep downloading to staging only if at least this much
/// source audio was already fetched (the plan's "≥20%" needs a total size
/// yt-dlp doesn't give us; an absolute floor approximates the intent).
const KEEP_DOWNLOAD_MIN_BYTES: u64 = 256 * 1024;
/// Safety net against zombie pipelines.
const PIPELINE_MAX_SECS: u64 = 30 * 60;

pub async fn handler(State(state): State<AppState>, req: Request) -> Response {
    let (id, format, username) = {
        let params = auth::query_params(req.uri().query().unwrap_or(""));
        (
            params.get("id").map(|v| v.to_string()).unwrap_or_default(),
            Format::from_query_value(params.get("f").map(|v| v.as_ref())).unwrap_or(Format::Xml),
            params.get("u").map(|v| v.to_string()).unwrap_or_default(),
        )
    };

    if !vtrack::is_virtual_id(&id) {
        return passthrough::handler(State(state), req).await;
    }

    let track = match vtrack::get(&state.db, &id).await {
        Ok(Some(track)) => track,
        Ok(None) => return error_not_found(&state.envelope().await, format),
        Err(error) => {
            tracing::error!(%error, id, "stream db lookup failed");
            return stream_error(&state, format, "internal error").await;
        }
    };

    // Already imported → serve the real file via Navidrome (seeking works).
    if let Some(real_id) = track.real_subsonic_id.clone() {
        return super::rewrite_id_and_passthrough(state, req, &real_id).await;
    }

    // Concurrency gate. Brief wait so a just-finishing stream frees a slot.
    let permit = match tokio::time::timeout(
        Duration::from_secs(10),
        state.stream_gate.clone().acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => permit,
        _ => {
            tracing::warn!(id, "stream slots exhausted");
            return stream_error(&state, format, "too many concurrent streams").await;
        }
    };

    match start_pipeline(state.clone(), track, username, permit).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, id, "virtual stream failed to start");
            stream_error(&state, format, &error.to_string()).await
        }
    }
}

async fn stream_error(state: &AppState, format: Format, message: &str) -> Response {
    let mut response = state.envelope().await.render_error(format, 0, message);
    // Plan M3: nothing audible by the deadline must surface as an error the
    // client notices quickly; 503 makes non-subsonic-aware players bail too.
    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
    response
}

async fn start_pipeline(
    state: AppState,
    track: VirtualTrack,
    username: String,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> anyhow::Result<Response> {
    let streaming = &state.config.streaming;
    let job_id = jobs::create(&state.db, &track.id, &username).await?;

    let resolution = match crate::resolve::resolve_cached(&state, &track).await {
        Ok(resolution) => resolution,
        Err(error) => {
            jobs::set_status(&state.db, &job_id, "failed", Some(&error.to_string())).await?;
            vtrack::set_status(&state.db, &track.id, "failed", Some(&error.to_string())).await?;
            return Err(error);
        }
    };
    tracing::info!(
        track = %format!("{} — {}", track.artist, track.title),
        url = %resolution.url,
        score = resolution.score,
        candidate = %resolution.candidate_title,
        "resolved virtual stream source"
    );
    jobs::set_resolution(&state.db, &job_id, &resolution.url, resolution.score).await?;
    vtrack::set_status(&state.db, &track.id, "streaming", None).await?;

    tokio::fs::create_dir_all(&state.config.library.staging_dir).await?;
    let staging_path = state
        .config
        .library
        .staging_dir
        .join(format!("{job_id}.src"));
    jobs::set_staging_path(&state.db, &job_id, &staging_path.to_string_lossy()).await?;

    // Source of original-quality bytes: innertube direct HTTP when enabled
    // and working (~1s to audio), else the yt-dlp pipe (~1.5s with the
    // manifest-skip flags). Direct-fetch failure must degrade speed, never
    // availability.
    let (mut source_reader, mut ytdlp) = if streaming.innertube {
        match direct_source(&state, &resolution.url).await {
            Ok(reader) => {
                tracing::info!(url = %resolution.url, "using innertube direct source");
                (reader, None)
            }
            Err(error) => {
                tracing::info!(%error, "innertube fast path unavailable; using yt-dlp");
                let (reader, child) = spawn_ytdlp_source(streaming, &resolution.url)?;
                (reader, Some(child))
            }
        }
    } else {
        let (reader, child) = spawn_ytdlp_source(streaming, &resolution.url)?;
        (reader, Some(child))
    };

    // ffmpeg: transcode for the live pipe.
    let (codec_args, content_type): (&[&str], &str) = match streaming.format {
        StreamFormat::Opus => (&["-c:a", "libopus", "-f", "ogg"], "audio/ogg"),
        StreamFormat::Mp3 => (&["-c:a", "libmp3lame", "-f", "mp3"], "audio/mpeg"),
    };
    let mut ffmpeg = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            "pipe:0",
            "-vn",
            "-map",
            "a",
        ])
        .args(codec_args)
        // Push pages out as they're encoded instead of buffering ~1s —
        // shaves noticeable time off the first audible byte.
        .args(["-flush_packets", "1"])
        .args(["-b:a", &format!("{}k", streaming.bitrate_kbps), "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let ffmpeg_stdin = ffmpeg.stdin.take().expect("ffmpeg stdin piped");
    let mut ffmpeg_stdout = ffmpeg.stdout.take().expect("ffmpeg stdout piped");
    drain_stderr(ffmpeg.stderr.take(), "ffmpeg");

    let (resp_tx, mut resp_rx) = mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(32);

    // Transcode pump: ffmpeg stdout → response channel. When the client
    // goes away, dropping ffmpeg_stdout EPIPEs ffmpeg, whose death flips the
    // source pump into staging-only mode.
    tokio::spawn(async move {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match ffmpeg_stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if resp_tx
                        .send(Ok(axum::body::Bytes::copy_from_slice(&buf[..n])))
                        .await
                        .is_err()
                    {
                        tracing::debug!("client disconnected from virtual stream");
                        break;
                    }
                }
            }
        }
    });

    // Source pump: yt-dlp stdout → staging file + ffmpeg stdin.
    let pump_state = state.clone();
    let pump_job = job_id.clone();
    let pump_track_id = track.id.clone();
    let pump_staging = staging_path.clone();
    // A yt-dlp that produces nothing must release its slot quickly, not
    // squat on it for the whole pipeline cap.
    let first_read_grace = Duration::from_secs(streaming.timeout_first_byte_secs + 30);
    tokio::spawn(async move {
        let _permit = permit; // slot held until the acquisition finishes
        let result = tokio::time::timeout(
            Duration::from_secs(PIPELINE_MAX_SECS),
            pump_source(
                source_reader.as_mut(),
                Some(ffmpeg_stdin),
                &pump_staging,
                first_read_grace,
            ),
        )
        .await;

        let mut outcome = match result {
            Err(_) => Err(anyhow::anyhow!("pipeline exceeded {PIPELINE_MAX_SECS}s")),
            Ok(outcome) => outcome,
        };

        // On failure the children may still be alive — reap them before
        // waiting, or the slot would stay occupied.
        if outcome.is_err() {
            if let Some(child) = ytdlp.as_mut() {
                let _ = child.start_kill();
            }
            let _ = ffmpeg.start_kill();
        }
        if let Some(child) = ytdlp.as_mut() {
            match child.wait().await {
                Ok(status) if !status.success() && outcome.is_ok() => {
                    outcome = Err(anyhow::anyhow!("yt-dlp exited with {status}"));
                }
                Err(e) if outcome.is_ok() => {
                    outcome = Err(anyhow::anyhow!("yt-dlp wait failed: {e}"));
                }
                _ => {}
            }
        }
        let _ = ffmpeg.wait().await;

        let staged_ok = matches!(&outcome, Ok(bytes) if *bytes > 0);
        if staged_ok {
            tracing::info!(
                job = pump_job,
                bytes = outcome.as_ref().unwrap(),
                "staging complete"
            );
            let _ = jobs::set_status(&pump_state.db, &pump_job, "finalizing", None).await;
            let _ = vtrack::set_status(&pump_state.db, &pump_track_id, "staged", None).await;
        } else {
            let reason = match outcome {
                Err(e) => e.to_string(),
                Ok(_) => "source produced no data".to_string(),
            };
            tracing::warn!(job = pump_job, %reason, "virtual stream failed");
            let _ = tokio::fs::remove_file(&pump_staging).await;
            let _ = jobs::set_status(&pump_state.db, &pump_job, "failed", Some(&reason)).await;
            let _ =
                vtrack::set_status(&pump_state.db, &pump_track_id, "failed", Some(&reason)).await;
        }
    });

    // First byte gate: nothing audible by the deadline → error response.
    let first_chunk = match tokio::time::timeout(
        Duration::from_secs(streaming.timeout_first_byte_secs),
        resp_rx.recv(),
    )
    .await
    {
        Ok(Some(Ok(chunk))) => chunk,
        Ok(Some(Err(e))) => return Err(e.into()),
        Ok(None) => anyhow::bail!("stream produced no audio"),
        Err(_) => {
            anyhow::bail!(
                "no audio within {}s (yt-dlp slow or blocked)",
                streaming.timeout_first_byte_secs
            )
        }
    };

    use tokio_stream::StreamExt;
    let body_stream = tokio_stream::once(Ok(first_chunk))
        .chain(tokio_stream::wrappers::ReceiverStream::new(resp_rx));
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, HeaderValue::from_static("none"))
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))
        .body(Body::from_stream(body_stream))?;
    response
        .headers_mut()
        .insert("X-Songarr-Job", HeaderValue::from_str(&job_id)?);
    Ok(response)
}

type SourceReader = Box<dyn tokio::io::AsyncRead + Send + Unpin>;

/// Direct innertube source: resolve the media URL and open the HTTP stream.
async fn direct_source(state: &AppState, watch_url: &str) -> anyhow::Result<SourceReader> {
    let audio = crate::resolve::innertube::direct_audio(
        &state.yt_http,
        &state.config.streaming.innertube_api_base,
        watch_url,
    )
    .await?;
    tracing::debug!(
        mime = audio.mime_type,
        bitrate = audio.bitrate,
        content_length = audio.content_length,
        "innertube format chosen"
    );
    crate::resolve::innertube::open_media_stream(&state.yt_http, &audio.url, audio.content_length)
        .await
}

/// Classic yt-dlp pipe source.
fn spawn_ytdlp_source(
    streaming: &crate::config::Streaming,
    url: &str,
) -> anyhow::Result<(SourceReader, tokio::process::Child)> {
    let mut command = tokio::process::Command::new(&streaming.ytdlp_path);
    command
        .arg("-f")
        .arg("bestaudio")
        .arg("--no-warnings")
        .arg("--no-playlist")
        // Skip the HLS/DASH manifest fetches during extraction: bestaudio
        // comes from adaptiveFormats anyway, and dropping those round-trips
        // cuts time-to-first-byte from ~2.5s to ~1.5s (measured 2026-06).
        .arg("--extractor-args")
        .arg("youtube:skip=hls,dash,translated_subs")
        .arg("-o")
        .arg("-")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if !streaming.ytdlp_proxy.is_empty() {
        command.arg("--proxy").arg(&streaming.ytdlp_proxy);
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("ytdlp stdout piped");
    drain_stderr(child.stderr.take(), "yt-dlp");
    Ok((Box::new(stdout), child))
}

/// Copy source output to the staging file and (while it lives) ffmpeg.
/// Returns total bytes staged. ffmpeg death only aborts the download when
/// too little was fetched to be worth keeping.
///
/// Takes the ffmpeg stdin BY VALUE: dropping the handle is the only way to
/// close the pipe and let ffmpeg see EOF (shutdown() merely flushes).
async fn pump_source(
    source: &mut (dyn tokio::io::AsyncRead + Send + Unpin),
    mut ffmpeg_stdin: Option<tokio::process::ChildStdin>,
    staging_path: &std::path::Path,
    first_read_grace: Duration,
) -> anyhow::Result<u64> {
    let mut staging = tokio::fs::File::create(staging_path).await?;
    let mut staged: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];

    loop {
        let n = if staged == 0 {
            tokio::time::timeout(first_read_grace, source.read(&mut buf))
                .await
                .map_err(|_| anyhow::anyhow!("yt-dlp produced no data"))??
        } else {
            source.read(&mut buf).await?
        };
        if n == 0 {
            break;
        }
        staging.write_all(&buf[..n]).await?;
        staged += n as u64;

        if let Some(stdin) = ffmpeg_stdin.as_mut() {
            if let Err(error) = stdin.write_all(&buf[..n]).await {
                ffmpeg_stdin = None; // close the pipe; ffmpeg is done
                if staged < KEEP_DOWNLOAD_MIN_BYTES {
                    anyhow::bail!("client left after only {staged} bytes: {error}");
                }
                tracing::info!(staged, "client gone; continuing download to staging");
            }
        }
    }
    staging.flush().await?;
    drop(ffmpeg_stdin); // EOF → ffmpeg flushes its tail and exits
    Ok(staged)
}

fn drain_stderr(stderr: Option<tokio::process::ChildStderr>, name: &'static str) {
    if let Some(mut stderr) = stderr {
        tokio::spawn(async move {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf).await;
            let trimmed = buf.trim();
            if !trimmed.is_empty() {
                tracing::debug!(process = name, output = %trimmed, "child stderr");
            }
        });
    }
}

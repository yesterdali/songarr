# Songarr Proxy

A self-hosted **Subsonic/OpenSubsonic reverse proxy** that sits between your
Subsonic clients (Amperfy, Symfonium, Tempo, the Navidrome web UI, …) and an
**unmodified Navidrome** server, and adds one superpower:

> Search any song in the world from inside your normal music app. Press play —
> it starts within seconds, and the file is silently acquired, tagged, and
> imported into your library as a normal local track forever after.

See `songarr-proxy-plan.md` for the full design and `DEVLOG.md` for progress
(M0–M4 implemented; M5 admin UI, M6 FLAC upgrade hook, M7 client matrix
pending).

## How it works

- Everything is transparently passed through to Navidrome — byte for byte.
- `search3`/`search2` responses get external catalog hits (Deezer, no API key)
  appended as *virtual tracks* (`sgr_…` ids), deduplicated against your
  library.
- Playing a virtual track resolves the best YouTube source (yt-dlp + scoring),
  live-transcodes to opus for your client, and tees the original-quality
  source into a staging area.
- Optional Yandex Music integration can feed Wave/search from your Yandex
  account. Yandex-origin tracks try Yandex audio first, then fall back to the
  normal YouTube resolver if the unofficial API cannot provide a source.
- A background worker remuxes (never lossy→lossy), tags, moves the file into
  `/music/_songarr/Artist/…`, triggers a Navidrome scan, remaps the virtual id
  to the real one, and replays any scrobbles/stars you made meanwhile.

## Quick start (development)

Prerequisites: Rust, Docker, `ffmpeg`/`ffprobe` on PATH.

```sh
# 1. Unit tests (fast, no services needed)
cargo test

# 2. Start the integration harness: a real Navidrome on 127.0.0.1:14533
#    seeded with 20 generated sine-wave tracks (admin / songarr-test)
tests/harness/up.sh

# 3. Integration suites (proxy runs in-process; Deezer and yt-dlp are mocked)
cargo test --test passthrough    -- --ignored                   # M1
cargo test --test virtual_search -- --ignored                   # M2
cargo test --test virtual_stream -- --ignored --test-threads=1  # M3
cargo test --test ingest         -- --ignored --test-threads=1  # M4

# 4. Tear down
tests/harness/down.sh
```

## Trying it for real (real Deezer + real YouTube)

With the harness Navidrome still up:

```sh
cat > /tmp/songarr.toml <<'EOF'
[server]
bind = "127.0.0.1:4534"
db_path = "/tmp/songarr-dev/songarr.db"

[navidrome]
base_url = "http://127.0.0.1:14533"
admin_user = "admin"
admin_password = "songarr-test"

[library]
music_dir = "tests/harness/data/music"   # the dir Navidrome scans
staging_dir = "/tmp/songarr-dev/staging"
EOF

SONGARR_CONFIG=/tmp/songarr.toml cargo run --release
```

Then point any Subsonic client (or just a browser for the web UI) at
`http://127.0.0.1:4534`, log in as `admin` / `songarr-test`, and search for a
song you know isn't in the library. Play it. Five-ish minutes later it's a
normal library track.

The same flow with curl:

```sh
AUTH='u=admin&p=songarr-test&v=1.16.1&c=curl&f=json'
# search → note the "sgr_…" id and its coverArt
curl "http://127.0.0.1:4534/rest/search3?$AUTH&query=daft+punk+one+more+time"
# play (writes audio to your speakers' favorite file)
curl "http://127.0.0.1:4534/rest/stream?$AUTH&id=sgr_XXXX" -o /tmp/song.ogg
# watch the import happen
sqlite3 /tmp/songarr-dev/songarr.db 'SELECT status FROM virtual_tracks;'
```

## Production deployment

See `docker/docker-compose.example.yml` — Navidrome + the proxy + gluetun
(VPN egress for yt-dlp via `ytdlp_proxy = "http://gluetun:8888"`). Copy
`config.example.toml` to your config volume as `songarr.toml`. Clients point
at port **4534** instead of Navidrome.

### Yandex Music

Yandex support uses the unofficial Python `yandex-music-api` helper packaged
in the Docker image. Generate a token on the host/container:

```sh
songarr-proxy yandex login
```

Then set `[yandex].enabled = true` and provide the token through
`SONGARR_YANDEX_ACCESS_TOKEN` (preferred) or `songarr.toml`. Tokens stay
server-side; the Wave PWA never receives them.

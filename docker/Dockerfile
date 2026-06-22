# Build context is the repository root:
#   docker build -f docker/Dockerfile -t songarr-proxy .
FROM node:22-bookworm-slim AS web-build
WORKDIR /build/web

COPY web/package*.json ./
RUN npm ci

COPY web ./
RUN npm run build

FROM rust:1-slim-bookworm AS build
WORKDIR /build

# Cache the dependency build separately from source changes. The discord-bot
# workspace member's manifest must be present for cargo to parse the workspace,
# but we only build the proxy (`-p songarr-proxy`), so the bot's heavy deps
# (songbird/opus) never compile into this image.
COPY Cargo.toml Cargo.lock ./
COPY discord-bot/Cargo.toml ./discord-bot/Cargo.toml
RUN mkdir -p src discord-bot/src \
    && echo 'fn main() {}' > src/main.rs \
    && echo 'fn main() {}' > discord-bot/src/main.rs \
    && cargo build --release -p songarr-proxy \
    && rm -rf src discord-bot/src

COPY src ./src
COPY migrations ./migrations
COPY --from=web-build /build/web/dist ./web/dist
RUN touch src/main.rs && cargo build --release -p songarr-proxy

FROM debian:bookworm-slim

# yt-dlp's release binary is a python3 zipapp; ffmpeg/ffprobe from the distro.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl python3 python3-pip ffmpeg \
    && pip3 install --no-cache-dir --break-system-packages yandex-music \
    && curl -fsSL https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp \
         -o /usr/local/bin/yt-dlp \
    && chmod a+rx /usr/local/bin/yt-dlp \
    && apt-get purge -y curl \
    && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /build/target/release/songarr-proxy /usr/local/bin/songarr-proxy
COPY scripts/songarr-yandex /usr/local/bin/songarr-yandex
RUN chmod a+rx /usr/local/bin/songarr-yandex

ENV SONGARR_CONFIG=/config/songarr.toml
VOLUME ["/config", "/music", "/staging"]
EXPOSE 4534

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
    CMD ["python3", "-c", "import urllib.request; urllib.request.urlopen('http://127.0.0.1:4534/healthz', timeout=4)"]

ENTRYPOINT ["/usr/local/bin/songarr-proxy"]

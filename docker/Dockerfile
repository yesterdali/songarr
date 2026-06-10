# Build context is the repository root:
#   docker build -f docker/Dockerfile -t songarr-proxy .
FROM rust:1-slim-bookworm AS build
WORKDIR /build

# Cache the dependency build separately from source changes.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim

# yt-dlp's release binary is a python3 zipapp; ffmpeg/ffprobe from the distro.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl python3 ffmpeg \
    && curl -fsSL https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp \
         -o /usr/local/bin/yt-dlp \
    && chmod a+rx /usr/local/bin/yt-dlp \
    && apt-get purge -y curl \
    && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /build/target/release/songarr-proxy /usr/local/bin/songarr-proxy

ENV SONGARR_CONFIG=/config/songarr.toml
VOLUME ["/config", "/music", "/staging"]
EXPOSE 4534

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
    CMD ["python3", "-c", "import urllib.request; urllib.request.urlopen('http://127.0.0.1:4534/healthz', timeout=4)"]

ENTRYPOINT ["/usr/local/bin/songarr-proxy"]

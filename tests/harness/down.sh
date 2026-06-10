#!/usr/bin/env bash
# Stop the integration Navidrome and wipe its state (seeded music is kept).
set -euo pipefail
cd "$(dirname "$0")"
docker compose -f compose.yml down -v

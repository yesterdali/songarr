#!/usr/bin/env bash
# Seed the test library and start the integration Navidrome on 127.0.0.1:14533.
# Then run: cargo test --test passthrough -- --ignored
set -euo pipefail
cd "$(dirname "$0")"

./seed.sh
docker compose -f compose.yml up -d

echo -n "waiting for navidrome"
for _ in $(seq 1 60); do
    if curl -fsS "http://127.0.0.1:14533/rest/ping?u=admin&p=songarr-test&v=1.16.1&c=harness&f=json" \
        | grep -q '"status":"ok"'; then
        echo " — up"
        exit 0
    fi
    echo -n "."
    sleep 1
done
echo " — FAILED: navidrome did not become healthy" >&2
docker compose -f compose.yml logs --tail 50 navidrome >&2
exit 1

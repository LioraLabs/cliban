#!/usr/bin/env bash
# Seed the fictional Pulse board used for README screenshots.
set -euo pipefail
DB=${1:?usage: seed-demo.sh /path/to/demo.db}
export CLIBAN_ACTOR=alex
c() { cliban --db "$DB" "$@" >/dev/null; }
c project add PULSE --name "Pulse" --description "Self-hosted uptime and status pages"
c milestone add --project PULSE --name "v1.0 hardening" --target 2026-08-21
add() { c issue add --project PULSE --title "$1" --priority "$2" --status "$3" --milestone "v1.0 hardening" --label "$4"; }
add "Cert expiry checks" high backlog feature
add "Status page themes" low backlog feature
add "Webhook notifications on state change" medium backlog feature
add "Flap detection: suppress alert storms" urgent in-progress bug
add "Prometheus metrics endpoint" medium in-progress feature
add "Migrate checks table to WAL mode" high in-review refactor
add "Docker healthcheck probe type" medium done feature
add "ICMP checks without root" high done feature
add "Retention policy for check history" medium backlog chore
c issue add --project PULSE --title "SMTP probe: STARTTLS handshake times out" --priority urgent --status blocked --label bug --blocked-by PULSE-4
add "Maintenance windows" low backlog feature
add "Slack + ntfy alert channels" high in-progress feature
add "Per-check timeout overrides" medium in-review refactor
add "HTTP keyword match probes" high done feature
add "SQLite persistence + WAL" urgent done refactor
add "Response-time percentiles on status page" medium done feature
echo "seeded $DB"

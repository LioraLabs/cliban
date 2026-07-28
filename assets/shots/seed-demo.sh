#!/usr/bin/env bash
# Seed the fictional demo boards used for README screenshots.
set -euo pipefail
DB=${1:?usage: seed-demo.sh /path/to/demo.db}
export CLIBAN_ACTOR=alex
c() { cliban --db "$DB" "$@" >/dev/null; }
c project add PULSE --name "Pulse" --description "Self-hosted uptime and status pages"
c milestone add --project PULSE --name "v1.0 hardening" --target 2026-08-21
add() { c issue add --project PULSE --title "$1" --priority "$2" --status "$3" --milestone "${5:-v1.0 hardening}" --label "$4"; }
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
c milestone add --project PULSE --name "v1.1 alerting" --target 2026-09-30 --description "Alert routing, silences, and escalation chains"
c milestone add --project PULSE --name "beta feedback"
c milestone edit --project PULSE --name "beta feedback" --status completed
add "Escalation chains with on-call rotations" high backlog feature "v1.1 alerting"
add "Alert silences with expiry" medium backlog feature "v1.1 alerting"
add "Dedup alerts across probe types" medium backlog refactor "v1.1 alerting"
c issue edit PULSE-4 --description-file - <<'DESC'
## Spec

Alerts currently fire on every failed check. A flapping endpoint (up, down,
up, down) sends a notification storm: 40 alerts in ten minutes for one bad
load balancer. Operators mute the channel, then miss the real outage.

Add hysteresis: a check must hold a state for N consecutive probes before
the transition alerts. Track flap frequency and suppress hosts that cross
the threshold, with a daily digest instead.

## Plan

### Task 1: transition hysteresis

- [x] **Step 1: add `stable_after` column with per-check override**
- [x] **Step 2: state machine only emits transition after N stable probes**
- [ ] **Step 3: surface pending-state on the status page**

### Task 2: flap suppression

- [ ] **Step 1: rolling transition counter per check**
- [ ] **Step 2: suppress + daily digest when threshold crossed**

## Activity Log

- 2026-07-26T14:02Z — root cause: alert fires per-check, not per-transition
- 2026-07-27T09:41Z — hysteresis window landed; probes green in staging
DESC
c project add TIDE --name "Tide" --description "Marine forecast API"
c project add FORGE --name "Forge" --description "Self-hosted CI runners"
c milestone add --project TIDE --name "public beta" --target 2026-08-07 --description "Rate limits, API keys, and the station map"
c milestone add --project TIDE --name "v2 stations" --target 2026-11-15
c milestone add --project FORGE --name "warm pools" --target 2026-08-30 --description "Pre-provisioned runner pools with sub-second pickup"
for i in 1 2 3 4 5 6 7 8; do st=done; [ $i -gt 6 ] && st=in-progress; c issue add --project TIDE --title "beta task $i" --status $st --milestone "public beta" --priority medium; done
for i in 1 2 3; do c issue add --project TIDE --title "station task $i" --status backlog --milestone "v2 stations" --priority low; done
for i in 1 2 3 4 5; do st=backlog; [ $i -le 2 ] && st=done; c issue add --project FORGE --title "pool task $i" --status $st --milestone "warm pools" --priority high; done
echo "seeded $DB"

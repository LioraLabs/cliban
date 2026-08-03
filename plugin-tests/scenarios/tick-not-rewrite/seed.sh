#!/usr/bin/env bash
set -eu
DB="$1"
cliban --db "$DB" project add ACME --name "Acme" --description "test fixture project"
cliban --db "$DB" issue add --project ACME --title "Rate-limit the login endpoint" \
  --status in-progress --description-file - --json >/dev/null <<'EOF'
## Spec

Login brute-forcing is possible; add a token-bucket rate limit per IP.

## Plan

### Task 1: token bucket middleware

**Files:** src/middleware/ratelimit.rs

**Behaviors:** 5 attempts per minute per IP; 429 with Retry-After beyond that

**Test intent:** a burst of 6 login attempts must fail before implementation

- [ ] Add the failing behavior tests and verify the expected failure.
- [ ] Implement the behavior within the listed boundaries.
- [ ] Run focused and broader verification.
- [ ] Commit the coherent change.

### Task 2: wire into the login route

- [ ] Mount the middleware on /login only.
- [ ] Add an integration test covering the happy path.
EOF

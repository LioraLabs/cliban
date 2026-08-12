#!/usr/bin/env bash
set -eu
DB="$1"
REPO=$(dirname -- "$DB")

git -C "$REPO" config user.email test@example.invalid
git -C "$REPO" config user.name "cliban plugin tests"
git -C "$REPO" config commit.gpgsign false
git -C "$REPO" config core.hooksPath /dev/null
printf '.fixture-worktrees/\n.worktrees/\nboard.db*\n' >"$REPO/.gitignore"
printf '# ACME\n' >"$REPO/README.md"
git -C "$REPO" add .gitignore README.md
git -C "$REPO" commit -qm base

cliban --db "$DB" project add ACME "Acme" --description "test fixture project" >/dev/null
cliban --db "$DB" milestone add "Release train" --project ACME >/dev/null
cliban --db "$DB" issue add "Document standalone setup" --project ACME \
  --label chore --description-file - >/dev/null <<'EOF'
## Spec

Add `standalone.md` containing the line `standalone ready`.
EOF
cliban --db "$DB" issue add "Document dispatched setup" --project ACME \
  --milestone "Release train" --label chore --description-file - >/dev/null <<'EOF'
## Spec

Add `dispatched.md` containing the line `dispatched ready`.
EOF

git -C "$REPO" branch milestone/release-train main
mkdir -p "$REPO/.fixture-worktrees"
git -C "$REPO" worktree add -q \
  "$REPO/.fixture-worktrees/milestone" milestone/release-train

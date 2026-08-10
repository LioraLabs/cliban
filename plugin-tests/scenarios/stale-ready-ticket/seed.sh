#!/usr/bin/env bash
set -eu
DB="$1"
REPO=$(dirname -- "$DB")

git -C "$REPO" config user.email test@example.invalid
git -C "$REPO" config user.name "cliban plugin tests"
git -C "$REPO" config commit.gpgsign false
git -C "$REPO" config core.hooksPath /dev/null
printf '.fixture-worktrees/\n' >"$REPO/.gitignore"
printf 'base\n' >"$REPO/base.txt"
git -C "$REPO" add .gitignore base.txt
git -C "$REPO" commit -qm base

cliban --db "$DB" project add ACME "Acme" --description "test fixture project"
cliban --db "$DB" milestone add "Release train" --project ACME
key=$(cliban --db "$DB" issue add "Ship the reviewed change" --project ACME \
  --milestone "Release train" --status in-review --label feature --json | jq -r .key)
branch=$(cliban --db "$DB" issue show "$key" --json | jq -r .git_branch_name)

git -C "$REPO" branch "$branch" main
git -C "$REPO" branch milestone/release-train main
mkdir -p "$REPO/.fixture-worktrees"
git -C "$REPO" worktree add -q "$REPO/.fixture-worktrees/milestone" milestone/release-train
printf 'later milestone work\n' >"$REPO/.fixture-worktrees/milestone/later.txt"
git -C "$REPO/.fixture-worktrees/milestone" add later.txt
git -C "$REPO/.fixture-worktrees/milestone" commit -qm "later milestone work"
mkdir -p "$REPO/.fixture-worktrees/milestone/.worktrees"
git -C "$REPO" worktree add -q \
  "$REPO/.fixture-worktrees/milestone/.worktrees/$branch" "$branch"

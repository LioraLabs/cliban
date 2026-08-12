#!/usr/bin/env bash
set -u
DB="$1"; REPO=$(dirname -- "$DB"); fail=0

check_ticket() {
  key=$1; file=$2
  issue=$(cliban --db "$DB" issue show "$key" --json)
  branch=$(printf '%s' "$issue" | jq -r .git_branch_name)
  status=$(printf '%s' "$issue" | jq -r .status)
  [ "$status" = in-review ] \
    || { echo "FAIL: $key is $status, not in-review"; fail=1; }
  plan=$(cliban --db "$DB" issue cat "$key" --section plan 2>/dev/null)
  printf '%s' "$plan" | grep -q '[^[:space:]]' \
    || { echo "FAIL: $key has no plan"; fail=1; }
  activity=$(cliban --db "$DB" issue cat "$key" --section activity 2>/dev/null)
  printf '%s' "$activity" | grep -qF "[cliban-flow] ticket start $key:" \
    || { echo "FAIL: $key did not start through the dispatcher"; fail=1; }
  ready=$(printf '%s\n' "$activity" | grep -F "[cliban-flow] ticket ready $key: ready (" | tail -1)
  [ -n "$ready" ] || { echo "FAIL: $key did not become ready through the dispatcher"; fail=1; return; }
  sha=$(git -C "$REPO" rev-parse "$branch" 2>/dev/null) \
    || { echo "FAIL: $key branch is missing"; fail=1; return; }
  recorded=$(printf '%s' "$ready" | sed -n "s/.*$branch@\([0-9a-f][0-9a-f]*\).*/\1/p")
  case $sha in
    "$recorded"*) [ -n "$recorded" ] \
      || { echo "FAIL: $key ready record has no branch SHA"; fail=1; } ;;
    *) echo "FAIL: $key ready record does not name its immutable branch SHA"; fail=1 ;;
  esac
  git -C "$REPO" show "$branch:$file" 2>/dev/null | grep -qx "${file%.md} ready" \
    || { echo "FAIL: $key branch does not contain its promised file"; fail=1; }
}

check_ticket ACME-1 standalone.md
check_ticket ACME-2 dispatched.md

exit $fail

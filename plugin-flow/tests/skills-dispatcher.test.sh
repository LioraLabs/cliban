#!/usr/bin/env bash
# CLI-83 — the workflow skills use the dispatcher as their only git protocol.
# CLI-93 — sanctioned teardown stays on that same surface.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
workflow=$ROOT/plugin-flow/skills/cliban-workflow/SKILL.md
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
milestone=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
recovery=$ROOT/plugin-flow/skills/recover-milestone/SKILL.md
manifest=$ROOT/plugin-flow/.claude-plugin/plugin.json
failed=0

has() { grep -Fq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }
lacks() { ! grep -Fq -- "$2" "$1" || { printf 'legacy %s in %s\n' "$2" "$1" >&2; failed=1; }; }

# CLI-86 — releases advertise the recovery protocol they install.
has "$manifest" '"version": "0.3.0"'
has "$manifest" 'recover interrupted milestones'

# CLI-96 — the installed skill resolves its sibling dispatcher outside cliban.
has "$workflow" '../../scripts/cliban-flow'
external=$(mktemp -d)
mkdir -p "$external/marketplace/.claude-plugin" "$external/adopter"
cp -R "$ROOT/plugin-flow" "$external/marketplace/plugin-flow"
cat >"$external/marketplace/.claude-plugin/marketplace.json" <<'EOF'
{
  "name": "cli96",
  "owner": {"name": "cliban tests"},
  "plugins": [{"name": "cliban-flow", "source": "./plugin-flow"}]
}
EOF
CLAUDE_CONFIG_DIR=$external/config claude plugin marketplace add "$external/marketplace" >/dev/null || failed=1
CLAUDE_CONFIG_DIR=$external/config claude plugin install cliban-flow@cli96 >/dev/null || failed=1
installed_workflow=$(find "$external/config/plugins/cache/cli96/cliban-flow" \
    -path '*/skills/cliban-workflow/SKILL.md' -print -quit)
[ -n "$installed_workflow" ] || failed=1
dispatcher=$(cd -- "$(dirname -- "$installed_workflow")/../.." && pwd)/scripts/cliban-flow
help=$(cd -- "$external/adopter" && "$dispatcher" help) || failed=1
case $help in *'milestone start'*) ;; *) failed=1 ;; esac
rm -rf -- "$external"
for command in 'milestone start' 'milestone finish' 'milestone abandon' 'ticket start' 'ticket status' 'ticket sync' 'ticket ready' 'ticket integrate' 'ticket abandon'; do
    has "$workflow" "\`$command\`"
done
has "$workflow" 'Dispatched ticket ready for integration'
has "$workflow" 'stop and say so'

has "$issue" 'ticket sync <KEY>'
has "$issue" 'ticket ready <KEY>'
has "$issue" 'resolve the conflicts'
has "$issue" 'resolution diff'

has "$milestone" 'milestone start "<milestone name>"'
has "$milestone" 'ticket start <KEY>'
has "$milestone" 'ticket integrate <KEY>'
has "$milestone" 'milestone finish "<milestone name>"'
has "$milestone" 'milestone abandon "<milestone name>"'
has "$milestone" 'own worktree'
has "$milestone" 'strict ancestry'
has "$milestone" 'squash'
lacks "$milestone" 'git checkout'
lacks "$milestone" 'git merge --no-ff'
lacks "$milestone" 'HEAD^2'
lacks "$milestone" '<build the project>'

# CLI-84 — recovery interprets the read-only survey without repairing or verifying.
has "$recovery" 'milestone status "<milestone name>"'
for state in 'Nearly finished' Abandoned 'Silent agent' 'Interrupted merge'; do
    has "$recovery" "$state"
done
for command in 'ticket status <KEY>' 'ticket sync <KEY>' 'ticket ready <KEY>' 'ticket start <KEY>'; do
    has "$recovery" "\`$command\`"
done
has "$recovery" 'one worktree at a time'
has "$workflow" 'recover-milestone'
has "$recovery" 'Do not execute repairs'
has "$recovery" 'Never run builds or tests during recovery'
has "$recovery" 'ticket abandon <KEY>'

exit "$failed"

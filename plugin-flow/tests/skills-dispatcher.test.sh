#!/usr/bin/env bash
# Structural facts about the skills, not their wording.
#
# This suite used to grep the skills for sentences. That pinned phrasing, not
# behavior: every rewrite failed it and no defect ever did. What survives here
# is what a grep of prose cannot fake — the installed plugin layout resolves its
# dispatcher, every dispatcher command the skills tell an agent to run exists,
# the orchestrator teaches the dispatcher instead of raw git, and the shipping
# version says what it changed. Skill *behavior* is tested by the scenario
# suite under plugin-tests/, against real agents on throwaway boards.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
SKILLS=$ROOT/plugin-flow/skills
workflow=$SKILLS/cliban-workflow/SKILL.md
milestone=$SKILLS/complete-milestone/SKILL.md
manifest=$ROOT/plugin-flow/.claude-plugin/plugin.json
failed=0

has() { grep -Fq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }
lacks() { ! grep -Fq -- "$2" "$1" || { printf 'legacy %s in %s\n' "$2" "$1" >&2; failed=1; }; }

# Releases advertise what they install. The version is read from the manifest
# rather than pinned here: a literal goes stale at every bump, which is how
# 0.8.0 shipped with no changelog entry at all.
version=$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' "$manifest" | head -1)
[ -n "$version" ] || { printf 'no version in %s\n' "$manifest" >&2; failed=1; }
grep -Eq "^## ($version|Unreleased)" "$ROOT/plugin-flow/CHANGELOG.md" ||
    { printf 'no changelog entry for %s or Unreleased\n' "$version" >&2; failed=1; }

# The installed skill resolves its sibling dispatcher outside the cliban repo.
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
rm -rf -- "$external"
case $help in *'milestone start'*) ;; *) failed=1 ;; esac

# Every `cliban-flow <noun> <verb>` a skill tells an agent to run exists in the
# dispatcher's own help. Derived, so adding a command to a skill without adding
# it to the dispatcher fails here instead of at 2am inside a dispatched agent.
while read -r noun verb; do
    case $help in
        *"$noun $verb"*) ;;
        *) printf 'skills invoke "cliban-flow %s %s", which help does not list\n' \
            "$noun" "$verb" >&2; failed=1 ;;
    esac
done < <(grep -rhoE 'cliban-flow (milestone|ticket) [a-z]+' "$SKILLS" |
    sed 's/^cliban-flow //' | sort -u)

# The orchestrator teaches the dispatcher, never hand-rolled git plumbing.
for plumbing in 'git checkout' 'git merge --no-ff' 'HEAD^2' '<build the project>'; do
    lacks "$milestone" "$plumbing"
done

exit "$failed"

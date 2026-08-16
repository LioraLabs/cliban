#!/usr/bin/env bash
# every relative markdown link in the workflow skills resolves to a real file.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
failed=0
while IFS=: read -r file target; do
    case "$target" in http://*|https://*|'#'*) continue ;; esac
    [ -e "$(dirname -- "$file")/${target%%#*}" ] ||
        { echo "dead link in $file: $target" >&2; failed=1; }
done < <(grep -rHo ']([^)]*)' --include='*.md' "$ROOT/plugin-flow/skills" |
    sed 's/:](/:/; s/)$//')

exit "$failed"

#!/usr/bin/env bash
# Regenerate assets/board.png: seed a demo board, capture the TUI from a
# real tmux session, render to styled HTML, screenshot with chromium.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"; tmux kill-session -t cliban-shot 2>/dev/null || true' EXIT
bash "$HERE/seed-demo.sh" "$WORK/demo.db"
tmux new-session -d -s cliban-shot -x 132 -y 25 "cliban --db $WORK/demo.db tui"
sleep 2; tmux send-keys -t cliban-shot j; sleep 1
tmux capture-pane -e -p -t cliban-shot > "$WORK/board.ans"
python3 "$HERE/ansi2html.py" "$WORK/board.ans" "$WORK/board.html" "cliban — Pulse"
chromium --headless=new --disable-gpu --screenshot="$HERE/../board.png" \
  --window-size=1560,780 --force-device-scale-factor=2 --hide-scrollbars \
  "file://$WORK/board.html" 2>/dev/null
echo "wrote $HERE/../board.png"

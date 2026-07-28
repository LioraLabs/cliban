#!/usr/bin/env bash
# Regenerate assets/{board,milestones,projects,activity,editor}.png from a seeded demo board.
# Needs: tmux, chromium, imagemagick, python3; nvim for the editor shot.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"; tmux kill-session -t cliban-shot 2>/dev/null || true' EXIT
# CLIBAN_BIN lets a checkout shoot its own build; an absolute path also
# survives an already-running tmux server whose PATH predates this shell.
BIN=$(command -v "${CLIBAN_BIN:-cliban}")
CLIBAN_BIN="$BIN" bash "$HERE/seed-demo.sh" "$WORK/demo.db"

run() { tmux new-session -d -s cliban-shot -x "$1" -y "$2" "env EDITOR=nvim $BIN --db $WORK/demo.db tui"; sleep 2; }
snap() { tmux capture-pane -e -p -t cliban-shot > "$1"; }
kill_() { tmux kill-session -t cliban-shot 2>/dev/null || true; }

# The board, scoped to the curated Pulse project so the filler tasks that
# feed the milestone rollups stay out of frame.
run 132 25
tmux send-keys -t cliban-shot p; sleep 1
tmux send-keys -t cliban-shot -l "pulse"; sleep 1
tmux send-keys -t cliban-shot Enter; sleep 1
tmux send-keys -t cliban-shot j; sleep 1
snap "$WORK/board.ans"; kill_
bash "$HERE/shoot-one.sh" "$WORK/board.ans" "$HERE/../board.png" "cliban — Pulse"

# The milestone page, all statuses, focused on a mid-flight milestone.
# Arrows, not j/k: on the page every letter types into the filter.
run 132 22; tmux send-keys -t cliban-shot m; sleep 1
tmux send-keys -t cliban-shot Tab Tab Tab; sleep 1; tmux send-keys -t cliban-shot Down Down; sleep 1
snap "$WORK/milestones.ans"; kill_
bash "$HERE/shoot-one.sh" "$WORK/milestones.ans" "$HERE/../milestones.png" "cliban — milestones"

# The project page, focused on the second row.
run 132 22; tmux send-keys -t cliban-shot p; sleep 1; tmux send-keys -t cliban-shot Down; sleep 1
snap "$WORK/projects.ans"; kill_
bash "$HERE/shoot-one.sh" "$WORK/projects.ans" "$HERE/../projects.png" "cliban — projects"

# The activity page, cursor on the reopen so the detail pane shows a note.
run 132 22; tmux send-keys -t cliban-shot a; sleep 1
snap "$WORK/activity.ans"; kill_
bash "$HERE/shoot-one.sh" "$WORK/activity.ans" "$HERE/../activity.png" "cliban — activity"

# Editing an issue in $EDITOR (TUI 'e'), diagnostics silenced for the shot.
if command -v nvim >/dev/null; then
  run 132 33; tmux send-keys -t cliban-shot l; sleep 1
  tmux send-keys -t cliban-shot e; sleep 6; tmux send-keys -t cliban-shot Escape; sleep 1
  tmux send-keys -t cliban-shot ":lua pcall(vim.diagnostic.enable, false)" Enter; sleep 1
  tmux send-keys -t cliban-shot ":set nospell cmdheight=0" Enter; sleep 1
  tmux send-keys -t cliban-shot ":echo ''" Enter; sleep 1
  snap "$WORK/editor.ans"; kill_
  bash "$HERE/shoot-one.sh" "$WORK/editor.ans" "$HERE/../editor.png" "cliban issue edit — nvim"
else
  echo "nvim not found; skipping editor shot"
fi

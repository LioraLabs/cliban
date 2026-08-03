#!/usr/bin/env bash
# Animate the TUI into assets/tour.gif from the seeded demo board.
#
# Same pipeline as shoot.sh — tmux drives the real binary, capture-pane -e
# grabs ANSI, ansi2html.py + headless chromium render each frame — plus an
# assembly pass: every snap records how long the frame stays on screen, and
# consecutive identical captures collapse into one longer delay, so a pause
# costs bytes only once. Needs: tmux, chromium, imagemagick, python3.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"; tmux kill-session -t cliban-film 2>/dev/null || true' EXIT
BIN=$(command -v "${CLIBAN_BIN:-cliban}")
CLIBAN_BIN="$BIN" bash "$HERE/seed-demo.sh" "$WORK/demo.db"
printf '1970-01-01T00:00:00Z' > "$WORK/seen"

S=cliban-film
TITLE="cliban"
OUT="${1:-$HERE/../tour.gif}"
FRAME=0
declare -a DELAYS

start() {
  tmux new-session -d -s $S -x "$1" -y "$2" \
    "env EDITOR=nvim CLIBAN_ACTOR=alex CLIBAN_TUI_SEEN_FILE=$WORK/seen $BIN --db $WORK/demo.db tui"
  sleep 2
}

# snap <centiseconds>: capture the pane as the next frame, shown for that
# long on playback. A capture identical to the previous frame just extends
# the previous frame's delay.
snap() {
  local cs=$1 f
  f=$(printf '%s/f-%03d.ans' "$WORK" $FRAME)
  tmux capture-pane -e -p -t $S > "$f"
  if [ $FRAME -gt 0 ] && cmp -s "$f" "$(printf '%s/f-%03d.ans' "$WORK" $((FRAME-1)))"; then
    rm "$f"
    DELAYS[FRAME-1]=$(( DELAYS[FRAME-1] + cs ))
  else
    DELAYS[FRAME]=$cs
    FRAME=$((FRAME+1))
  fi
}

key() { tmux send-keys -t $S "$@"; sleep 0.4; }

# ---- the scene: a day on the board ----------------------------------------
# Open unscoped, scope to Pulse by typing on the project page, walk to the
# urgent in-progress card, promote it to in-review, admire the milestone
# rollup, land on the activity feed. One loop ≈ 16 seconds.

start 132 25
snap 130                                   # the unscoped board
key p;                       snap 90       # project page
for ch in p u l s e; do                    # type the filter, one frame a key
  tmux send-keys -t $S -l "$ch"; sleep 0.25; snap 18
done
key Enter;                   snap 130      # board, scoped to Pulse
key l;                       snap 60       # cursor onto the urgent flap-detection card
key L;                       snap 70       # two column hops: in-progress -> blocked
key L;                       snap 140      # -> in-review (columns are b/i/b/r/d)
key m;                       snap 110      # milestone page: progress + target
key Down;                    snap 100      # focus the mid-flight milestone
key Escape;                  snap 60       # back to the board (letters type into
key a;                       snap 240      # page filters!) then the activity feed
tmux kill-session -t $S 2>/dev/null || true

echo "captured $FRAME unique frames"

# ---- render ---------------------------------------------------------------
# Measure once from the first frame (all frames share the terminal size),
# then shoot every frame at identical window dimensions — a GIF needs every
# frame the same size, so no per-frame trim.
MARGIN=${MARGIN:-90}
python3 "$HERE/ansi2html.py" --measure "$WORK/f-000.ans" "$WORK/m.html" "$TITLE"
chromium --headless=new --disable-gpu --screenshot="$WORK/m.png" \
  --window-size=2400,1600 --default-background-color=00000000 \
  --hide-scrollbars "file://$WORK/m.html" 2>/dev/null
read -r WIN_W WIN_H < <(magick "$WORK/m.png" -trim -format "%w %h" info:) || true

for f in "$WORK"/f-*.ans; do
  python3 "$HERE/ansi2html.py" "$f" "$f.html" "$TITLE"
  chromium --headless=new --disable-gpu --screenshot="$f.png" \
    --window-size=$((WIN_W + 2*MARGIN)),$((WIN_H + 2*MARGIN)) \
    --force-device-scale-factor=1 --hide-scrollbars "file://$f.html" 2>/dev/null
done

# ---- assemble -------------------------------------------------------------
args=(-loop 0)
i=0
for f in "$WORK"/f-*.ans.png; do
  args+=(-delay "${DELAYS[i]}" "$f")
  i=$((i+1))
done
magick "${args[@]}" -layers Optimize "$OUT"
echo "wrote $OUT ($FRAME frames, $(du -h "$OUT" | cut -f1))"

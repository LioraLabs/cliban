#!/usr/bin/env bash
# Animate the TUI into assets/*.gif from the seeded demo board.
#
#   film.sh [tour|edit|agent|all]     (default: all)
#
# Same pipeline as shoot.sh — tmux drives the real binary, capture-pane -e
# grabs ANSI, ansi2html.py + headless chromium render each frame — plus an
# assembly pass: every snap records how long the frame stays on screen, and
# consecutive identical captures collapse into one longer delay, so a pause
# costs bytes only once. Needs: tmux, chromium, imagemagick, python3; nvim
# for the edit scene.
#
# Scene-writing notes, learned the hard way:
#   - On the full-screen pages (m/p/a) letters type into the filter; Escape
#     back to the board before pressing a page key.
#   - Columns are backlog/in-progress/blocked/in-review/done: "promote" a
#     card with H/L and it walks THROUGH blocked. Two hops, two events.
#   - Each scene gets a fresh DB copy, so mutations never leak between
#     scenes or into the stills.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"; tmux kill-session -t cliban-film 2>/dev/null || true' EXIT
BIN=$(command -v "${CLIBAN_BIN:-cliban}")
CLIBAN_BIN="$BIN" bash "$HERE/seed-demo.sh" "$WORK/seed.db"
printf '1970-01-01T00:00:00Z' > "$WORK/seen"

S=cliban-film
TITLE="cliban"
MARGIN=${MARGIN:-90}
FRAME=0
declare -a DELAYS

# The seed is in WAL mode, so most of its data lives in the -wal sidecar;
# copy the whole family or the snapshot is torn ("database disk image is
# malformed").
fresh_db() {
  rm -f "$WORK"/demo.db*
  local ext
  for ext in "" "-wal" "-shm"; do
    [ -f "$WORK/seed.db$ext" ] && cp "$WORK/seed.db$ext" "$WORK/demo.db$ext"
  done
}

start_tui() {
  tmux new-session -d -s $S -x "$1" -y "$2" \
    "env EDITOR=nvim CLIBAN_ACTOR=alex CLIBAN_TUI_SEEN_FILE=$WORK/seen $BIN --db $WORK/demo.db tui"
  sleep 2
}

start_shell() {
  tmux new-session -d -s $S -x "$1" -y "$2" \
    "env CLIBAN_DB=$WORK/demo.db CLIBAN_ACTOR=claude PATH=$(dirname "$BIN"):/usr/bin:/bin PS1='$ ' bash --norc --noprofile"
  sleep 1
}

stop() { tmux kill-session -t $S 2>/dev/null || true; }

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

# type_ <text> <cs-per-chunk>: send literally, a few characters at a time,
# snapping between chunks for a typed feel.
type_() {
  local text=$1 cs=${2:-14} i
  for ((i=0; i<${#text}; i+=5)); do
    # `--` so a chunk that happens to start with '-' isn't parsed as flags.
    tmux send-keys -t $S -l -- "${text:i:5}"
    sleep 0.15
    snap "$cs"
  done
}

# render_gif <out.gif>: shoot every captured frame at identical dimensions
# and assemble with per-frame delays. Dimensions are the max over EVERY
# frame's measurement, not the first frame's: a TUI paints the whole screen
# so any frame would do, but a shell scene grows downward, and measuring
# frame zero crops everything after the first Enter.
render_gif() {
  local out=$1 f i args w h
  WIN_W=0; WIN_H=0
  for f in "$WORK"/f-*.ans; do
    python3 "$HERE/ansi2html.py" --measure "$f" "$WORK/m.html" "$TITLE" >/dev/null
    chromium --headless=new --disable-gpu --screenshot="$WORK/m.png" \
      --window-size=2400,2000 --default-background-color=00000000 \
      --hide-scrollbars "file://$WORK/m.html" 2>/dev/null
    read -r w h < <(magick "$WORK/m.png" -trim -format "%w %h" info:) || true
    [ "$w" -gt "$WIN_W" ] && WIN_W=$w
    [ "$h" -gt "$WIN_H" ] && WIN_H=$h
  done
  for f in "$WORK"/f-*.ans; do
    python3 "$HERE/ansi2html.py" "$f" "$f.html" "$TITLE" >/dev/null
    chromium --headless=new --disable-gpu --screenshot="$f.png" \
      --window-size=$((WIN_W + 2*MARGIN)),$((WIN_H + 2*MARGIN)) \
      --force-device-scale-factor=1 --hide-scrollbars "file://$f.html" 2>/dev/null
  done
  args=(-loop 0)
  i=0
  for f in "$WORK"/f-*.ans.png; do
    args+=(-delay "${DELAYS[i]}" "$f")
    i=$((i+1))
  done
  magick "${args[@]}" -layers Optimize "$out"
  echo "wrote $out ($FRAME frames, $(du -h "$out" | cut -f1))"
  rm -f "$WORK"/f-* "$WORK"/m.*
  FRAME=0
  DELAYS=()
}

# ---- tour: a lap around the board -----------------------------------------
scene_tour() {
  fresh_db
  start_tui 132 25
  snap 130                                 # the unscoped board
  key p;                       snap 90     # project page
  for ch in p u l s e; do                  # type the filter, one frame a key
    tmux send-keys -t $S -l "$ch"; sleep 0.25; snap 18
  done
  key Enter;                   snap 130    # board, scoped to Pulse
  key l;                       snap 60     # cursor onto the urgent flap card
  key L;                       snap 70     # two column hops: -> blocked
  key L;                       snap 140    # -> in-review
  key m;                       snap 110    # milestone page: progress + target
  key Down;                    snap 100    # focus the mid-flight milestone
  key Escape;                  snap 60     # back to the board...
  key a;                       snap 240    # ...then the attributed timeline
  stop
  render_gif "$HERE/../tour.gif"
}

# ---- edit: 'e' straight into $EDITOR and back -----------------------------
scene_edit() {
  fresh_db
  start_tui 132 33
  # Scope to Pulse off camera so the scene opens on the curated board.
  tmux send-keys -t $S p; sleep 0.5
  tmux send-keys -t $S -l pulse; sleep 0.5
  tmux send-keys -t $S Enter; sleep 1
  snap 120                                 # the board
  key l;                       snap 70     # select the flap-detection card
  key e; sleep 6                           # nvim opens on frontmatter + markdown
  key Escape
  tmux send-keys -t $S ":lua pcall(vim.diagnostic.enable, false)" Enter; sleep 0.5
  tmux send-keys -t $S ":set nospell noshowcmd shortmess+=F" Enter; sleep 0.5
  tmux send-keys -t $S ":echo ''" Enter; sleep 0.5
  snap 170                                 # the issue as a buffer
  tmux send-keys -t $S C-d; sleep 0.5
  snap 130                                 # scroll: the plan, ticked boxes and all
  tmux send-keys -t $S gg; sleep 0.4; snap 50
  # Anchored: bare /status lands on the "# Statuses:" hint line above the
  # frontmatter, whose edits are ignored on save.
  type_ "/^status:" 12; key Enter; snap 50
  type_ ":s/in-progress/in-review/" 12
  key Enter;                   snap 110    # the field flips in the buffer
  type_ ":wq" 16; key Enter; sleep 1.5
  snap 260                                 # back on the board: card moved columns
  stop
  render_gif "$HERE/../edit.gif"
}

# ---- agent: the CLI loop a session actually lives -------------------------
scene_agent() {
  fresh_db
  start_shell 110 30
  snap 60
  type_ "cliban issue show PULSE-4 --section plan" 8
  key Enter;                   snap 220    # the plan: ticked steps ARE progress
  type_ "cliban issue tick PULSE-4 --task 1 --step 3" 8
  key Enter;                   snap 150
  type_ "cliban issue log PULSE-4 \"pending-state needs its own color; reused the blocked hue\"" 6
  key Enter;                   snap 150
  # --limit keeps the seed's created-flood from scrolling the story away;
  # newest-first puts the tick and log on top.
  type_ "cliban activity --project PULSE --limit 6" 8
  key Enter;                   snap 300    # the attributed trail, tick + log on top
  stop
  render_gif "$HERE/../agent.gif"
}

case "${1:-all}" in
  tour)  scene_tour ;;
  edit)  scene_edit ;;
  agent) scene_agent ;;
  all)   scene_tour; scene_edit; scene_agent ;;
  *) echo "usage: film.sh [tour|edit|agent|all]" >&2; exit 2 ;;
esac

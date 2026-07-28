#!/usr/bin/env bash
# shoot-one.sh capture.ans out.png "title"  -> tight-margin framed shot
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ANS=$1 OUT=$2 TITLE=$3 MARGIN=${MARGIN:-90}
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
python3 "$HERE/ansi2html.py" --measure "$ANS" "$W/m.html" "$TITLE"
chromium --headless=new --disable-gpu --screenshot="$W/m.png" --window-size=2400,1600 \
  --default-background-color=00000000 --hide-scrollbars "file://$W/m.html" 2>/dev/null
read -r WIN_W WIN_H < <(magick "$W/m.png" -trim -format "%w %h" info:) || true
python3 "$HERE/ansi2html.py" "$ANS" "$W/f.html" "$TITLE"
chromium --headless=new --disable-gpu --screenshot="$OUT" \
  --window-size=$((WIN_W + 2*MARGIN)),$((WIN_H + 2*MARGIN)) \
  --force-device-scale-factor=2 --hide-scrollbars "file://$W/f.html" 2>/dev/null
echo "wrote $OUT (${WIN_W}x${WIN_H} + ${MARGIN}px margin)"

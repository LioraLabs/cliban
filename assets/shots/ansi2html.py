#!/usr/bin/env python3
"""ANSI (SGR-only, e.g. tmux capture-pane -e) -> styled HTML terminal shot.

usage: ansi2html.py capture.ans out.html "window title" [cols]
"""
import html, re, sys

# Base-16 palette: tuned dark theme (near-Catppuccin Macchiato values).
BASE = ["#1e2030", "#ed8796", "#a6da95", "#eed49f", "#8aadf4", "#c6a0f6",
        "#8bd5ca", "#cad3f5", "#5b6078", "#ed8796", "#a6da95", "#eed49f",
        "#8aadf4", "#c6a0f6", "#8bd5ca", "#ffffff"]
FG_DEF, BG_DEF = "#cad3f5", "#24273a"

def xterm256(n):
    if n < 16:
        return BASE[n]
    if n < 232:
        n -= 16
        r, g, b = n // 36, (n // 6) % 6, n % 6
        conv = lambda v: 0 if v == 0 else 55 + v * 40
        return "#%02x%02x%02x" % (conv(r), conv(g), conv(b))
    v = 8 + (n - 232) * 10
    return "#%02x%02x%02x" % (v, v, v)

def render(text):
    fg, bg, bold, dim, ital, rev = None, None, False, False, False, False
    out, buf = [], []

    def flush():
        if not buf:
            return
        f = fg or FG_DEF
        b = bg
        if rev:
            f, b = (b or BG_DEF), (fg or FG_DEF)
        st = "color:%s;" % f
        if b:
            st += "background:%s;" % b
        if bold:
            st += "font-weight:600;"
        if dim:
            st += "opacity:.55;"
        if ital:
            st += "font-style:italic;"
        out.append('<span style="%s">%s</span>' % (st, html.escape("".join(buf)).replace(" ", " ")))
        buf.clear()

    i = 0
    for line in text.split("\n"):
        pos = 0
        for m in re.finditer(r"\x1b\[([0-9;:]*)m", line):
            buf.append(line[pos:m.start()])
            flush()
            pos = m.end()
            toks = m.group(1).split(";")
            # colon-form params (nvim: 4:3 undercurl, 58:2::r:g:b underline
            # color) are self-delimited; consume them without styling.
            ps = []
            for t in toks:
                if ":" in t:
                    continue
                ps.append(int(t) if t else 0)
            ps = ps or ([0] if not any(":" in t for t in toks) else ps)
            j = 0
            while j < len(ps):
                p = ps[j]
                if p == 0:
                    fg = bg = None
                    bold = dim = ital = rev = False
                elif p == 1: bold = True
                elif p == 2: dim = True
                elif p == 3: ital = True
                elif p == 7: rev = True
                elif p in (22,): bold = dim = False
                elif p == 23: ital = False
                elif p == 27: rev = False
                elif 30 <= p <= 37: fg = BASE[p - 30]
                elif 90 <= p <= 97: fg = BASE[p - 90 + 8]
                elif 40 <= p <= 47: bg = BASE[p - 40]
                elif 100 <= p <= 107: bg = BASE[p - 100 + 8]
                elif p == 39: fg = None
                elif p == 49: bg = None
                elif p in (38, 48) and j + 1 < len(ps):
                    tgt = "fg" if p == 38 else "bg"
                    if ps[j + 1] == 5 and j + 2 < len(ps):
                        c = xterm256(ps[j + 2]); j += 2
                    elif ps[j + 1] == 2 and j + 4 < len(ps):
                        c = "#%02x%02x%02x" % (ps[j+2], ps[j+3], ps[j+4]); j += 4
                    else:
                        c = None
                    if c:
                        if tgt == "fg": fg = c
                        else: bg = c
                j += 1
        buf.append(line[pos:])
        flush()
        out.append("\n")
    return "".join(out)

TPL = """<!doctype html><meta charset="utf-8"><style>
html,body{margin:0;height:100%%}
body{display:flex;align-items:center;justify-content:center;%(bodybg)s}
.win{border-radius:12px;overflow:hidden;%(shadow)s
  border:1px solid rgba(255,255,255,.09)}
.bar{background:#1e2030;display:flex;align-items:center;padding:10px 14px;gap:8px}
.dot{width:12px;height:12px;border-radius:50%%}
.title{flex:1;text-align:center;color:#8087a2;
  font:500 12px/1 'JetBrainsMono Nerd Font','JetBrains Mono',monospace;
  margin-right:52px}
pre{margin:0;padding:14px 18px;background:%(bg)s;
  font:13px/1.35 'JetBrainsMono Nerd Font','JetBrains Mono',monospace;color:%(fg)s}
</style><body><div class="win"><div class="bar">
<div class="dot" style="background:#ff5f57"></div>
<div class="dot" style="background:#febc2e"></div>
<div class="dot" style="background:#28c840"></div>
<div class="title">%(title)s</div></div><pre>%(body)s</pre></div></body>
"""

measure = "--measure" in sys.argv
argv = [a for a in sys.argv if a != "--measure"]
src, dst, title = argv[1], argv[2], argv[3]
text = open(src).read().rstrip("\n")
style = {
    "bodybg": "" if measure else """
  background:radial-gradient(120% 130% at 20% 0%,#3b4261 0%,#1a1b26 55%,#0f0f17 100%)""",
    "shadow": "" if measure else """
  box-shadow:0 30px 80px rgba(0,0,0,.65),0 4px 16px rgba(0,0,0,.5);""",
}
open(dst, "w").write(TPL % {"body": render(text), "title": html.escape(title),
                            "bg": BG_DEF, "fg": FG_DEF, **style})
print("wrote", dst)

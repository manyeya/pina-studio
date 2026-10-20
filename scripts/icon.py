"""Pina Studio's app icon: its step sequencer, the lit steps spelling p, each row in a track's colour
(drums, percussion, bass, keys, synth: crates/app/src/looks.rs, family_color), accented steps
dotted white and the playing step outlined, as the Steps tab draws them; on a dark squircle
(Apple's grid: 824 of 1024, a superellipse), as SVG. `cargo run -p pina --example icon` renders it
to pina.icns.

    python3 scripts/icon.py   # writes crates/app/icon/pina.svg
"""
import colorsys, math, os

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "app", "icon")
def squircle(cx, cy, size, n=5.0, steps=360):
    a = size / 2
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        pts.append(f"{cx + a*math.copysign(abs(c)**(2/n), c):.2f},{cy + a*math.copysign(abs(s)**(2/n), s):.2f}")
    return "M" + " L".join(pts) + " Z"

def hsl(h, s, l):
    r, g, b = colorsys.hls_to_rgb(h / 360, l, s)
    return "#%02x%02x%02x" % (round(r * 255), round(g * 255), round(b * 255))

# A row a track, top to bottom, in looks.rs's family colours.
ROWS = [hsl(6, 0.92, 0.66), hsl(29, 0.92, 0.62), hsl(256, 0.86, 0.72), hsl(163, 0.66, 0.52), hsl(202, 0.92, 0.62)]
OFF = "#1c1c23"
X = [245, 383, 521, 659]
Y = [176, 314, 452, 590, 728]
CELL, RX = 120, 22
# The steps on, and how hard (as velocity sets a lit step's strength in the Steps tab).
LIT = {(0, 0): 1.0, (1, 0): 0.9, (2, 0): 0.8, (3, 0): 0.94, (0, 1): 0.86, (3, 1): 1.0,
       (0, 2): 1.0, (1, 2): 0.84, (2, 2): 0.92, (3, 2): 0.82, (0, 3): 0.92, (0, 4): 0.84}
ACCENTED = [(0, 0), (3, 1), (0, 2)]
PLAYING = 1

def cell(c, r, fill, extra=""):
    return f'<rect x="{X[c]}" y="{Y[r]}" width="{CELL}" height="{CELL}" rx="{RX}" fill="{fill}"{extra}/>'

glyph = "".join(cell(c, r, OFF) for c in range(4) for r in range(5) if (c, r) not in LIT)
# The loudest steps glow.
glyph += '<g opacity="0.5" filter="url(#glow)">' + "".join(
    cell(c, r, ROWS[r]) for (c, r), v in LIT.items() if v >= 1.0) + "</g>"
for (c, r), v in LIT.items():
    glyph += cell(c, r, OFF) + cell(c, r, ROWS[r], f' fill-opacity="{v}"')
for c, r in ACCENTED:
    glyph += f'<circle cx="{X[c] + 92}" cy="{Y[r] + 28}" r="13" fill="#ffffff"/>'
for r in range(5):
    strength = 0.6 if (PLAYING, r) in LIT else 0.16
    glyph += (f'<rect x="{X[PLAYING] + 3}" y="{Y[r] + 3}" width="{CELL - 6}" height="{CELL - 6}" rx="{RX - 3}" '
              f'fill="none" stroke="#ffffff" stroke-opacity="{strength}" stroke-width="6"/>')

def write(name, svg):
    open(os.path.join(OUT, name), "w").write(svg)

sq = squircle(512, 512, 824)
icon = f'''<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><defs>
  <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#121216"/>
    <stop offset="1" stop-color="#0a0a0d"/>
  </linearGradient>
  <linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffffff" stop-opacity="0.10"/>
    <stop offset="0.4" stop-color="#ffffff" stop-opacity="0"/>
  </linearGradient>
  <filter id="glow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="14"/></filter>
  <filter id="blur8" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="8"/></filter>
  <clipPath id="sq"><path d="{sq}"/></clipPath>
</defs>
<path d="{sq}" transform="translate(0 10)" fill="#000" opacity="0.32" filter="url(#blur8)"/>
<g clip-path="url(#sq)"><rect width="1024" height="1024" fill="url(#bg)"/>
{glyph}
<rect width="1024" height="1024" fill="url(#sheen)"/></g>
<path d="{sq}" fill="none" stroke="#ffffff" stroke-opacity="0.10" stroke-width="3"/>
</svg>'''
write("pina.svg", icon)

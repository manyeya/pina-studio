"""Pina Studio's app icon: a metronome, its arm swung, on a dark squircle (Apple's grid: 824 of 1024,
a superellipse), as SVG. `cargo run -p pina --example icon` renders it to pina.icns.

    python3 scripts/icon.py   # writes crates/app/icon/pina.svg
"""
import math, os

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "app", "icon")
def squircle(cx, cy, size, n=5.0, steps=360):
    a = size / 2
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        pts.append(f"{cx + a*math.copysign(abs(c)**(2/n), c):.2f},{cy + a*math.copysign(abs(s)**(2/n), s):.2f}")
    return "M" + " L".join(pts) + " Z"

DEFS = '''
  <linearGradient id="bg" x1="0" y1="0" x2="0.35" y2="1">
    <stop offset="0" stop-color="#2a3050"/>
    <stop offset="0.5" stop-color="#131628"/>
    <stop offset="1" stop-color="#07080d"/>
  </linearGradient>
  <radialGradient id="glow" cx="0.62" cy="0.40" r="0.55">
    <stop offset="0" stop-color="#5b8cff" stop-opacity="0.42"/>
    <stop offset="0.6" stop-color="#5b8cff" stop-opacity="0.08"/>
    <stop offset="1" stop-color="#5b8cff" stop-opacity="0"/>
  </radialGradient>
  <linearGradient id="body" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffffff" stop-opacity="0.24"/>
    <stop offset="0.6" stop-color="#ffffff" stop-opacity="0.08"/>
    <stop offset="1" stop-color="#ffffff" stop-opacity="0.14"/>
  </linearGradient>
  <linearGradient id="rim" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffffff" stop-opacity="0.85"/>
    <stop offset="0.45" stop-color="#ffffff" stop-opacity="0.18"/>
    <stop offset="1" stop-color="#ffffff" stop-opacity="0.38"/>
  </linearGradient>
  <linearGradient id="arm" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#cfdcff"/>
    <stop offset="0.35" stop-color="#7ea2ff"/>
    <stop offset="1" stop-color="#3a5de0"/>
  </linearGradient>
  <linearGradient id="weight" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffffff"/>
    <stop offset="1" stop-color="#cdd8f7"/>
  </linearGradient>
  <filter id="blur24" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="24"/></filter>
  <filter id="blur8" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="8"/></filter>
'''
BODY = "M440 214 L584 214 Q610 214 616 240 L722 768 Q728 800 694 800 L330 800 Q296 800 302 768 L408 240 Q414 214 440 214 Z"
PIVOT = (512, 700)
def arm(angle, opacity, glow):
    g = f'<g transform="rotate({angle} {PIVOT[0]} {PIVOT[1]})" opacity="{opacity}">'
    if glow:
        g += '<rect x="484" y="262" width="56" height="456" rx="28" fill="#5b8cff" filter="url(#blur24)" opacity="0.85"/>'
    g += '<rect x="490" y="268" width="44" height="452" rx="22" fill="url(#arm)"/>'
    g += '<rect x="452" y="356" width="120" height="86" rx="26" fill="url(#weight)"/>'
    g += '<rect x="464" y="364" width="96" height="20" rx="10" fill="#ffffff" opacity="0.9"/>'
    return g + '</g>'
GLYPH = f'''
<path d="{BODY}" fill="#000" opacity="0.35" filter="url(#blur24)" transform="translate(0 18)"/>
<path d="{BODY}" fill="url(#body)"/>
<path d="{BODY}" fill="none" stroke="url(#rim)" stroke-width="9"/>
'''
# The scale: ticks on an arc above the pivot.
ticks = ''
for a in [-12, -6, 0, 6, 12]:
    r1, r2 = 392, (424 if a == 0 else 412)
    x1, y1 = PIVOT[0] + r1*math.sin(math.radians(a)), PIVOT[1] - r1*math.cos(math.radians(a))
    x2, y2 = PIVOT[0] + r2*math.sin(math.radians(a)), PIVOT[1] - r2*math.cos(math.radians(a))
    ticks += f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" stroke="#ffffff" stroke-opacity="0.45" stroke-width="8" stroke-linecap="round"/>'
GLYPH += ticks
GLYPH += arm(-24, 0.18, False)   # where it swung from
GLYPH += arm(24, 1.0, True)
GLYPH += f'<circle cx="{PIVOT[0]}" cy="{PIVOT[1]}" r="38" fill="url(#weight)"/><circle cx="{PIVOT[0]}" cy="{PIVOT[1]}" r="14" fill="#3a5de0"/>'
GLYPH += '<rect x="352" y="746" width="320" height="16" rx="8" fill="#ffffff" opacity="0.22"/>'

def write(name, svg):
    open(os.path.join(OUT, name), "w").write(svg)

sq = squircle(512, 512, 824)
grid = f'''<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><defs>{DEFS}
  <clipPath id="sq"><path d="{sq}"/></clipPath>
</defs>
<path d="{sq}" transform="translate(0 10)" fill="#000" opacity="0.32" filter="url(#blur8)"/>
<g clip-path="url(#sq)"><rect width="1024" height="1024" fill="url(#bg)"/><rect width="1024" height="1024" fill="url(#glow)"/>
<g transform="translate(512 512) scale(0.76) translate(-512 -508)">{GLYPH}</g></g>
<path d="{sq}" fill="none" stroke="#ffffff" stroke-opacity="0.12" stroke-width="3"/>
</svg>'''
write("pina.svg", grid)

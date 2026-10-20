"""The picture behind Pina Studio's disk image window (640 by 400, and twice that for Retina): its
dark, its name and version, a glow and rings round where "Install Pina Studio" sits, a plate under its
name (mid grey, so the Finder's label reads in light mode and dark), "Double-click to install",
and the EQ curve over a spectrum along the bottom, as in the installer. Unsigned builds say how
to open it anyway.

    python3 scripts/dmg-background.py <folder> <version> [--unsigned]

writes background.png and background@2x.png there. Drawn four times over and scaled down, so
its curves are smooth.
"""
import math, os, sys

from PIL import Image, ImageDraw, ImageFilter, ImageFont

W, H = 640, 400
# Where the installer's icon sits (its centre) and how big: scripts/dmg-settings.py says the same.
ICON = (320, 168)
ICON_SIZE = 128
K = 4  # drawn at 4x, scaled down

BG = (12, 12, 15)
TEXT = (244, 244, 246)
TEXT_2 = (212, 212, 218)
TEXT_3 = (138, 138, 149)
ACCENT = (91, 140, 255)
CURVE = (255, 209, 102)
PLATE = (110, 110, 120)

FONTS = os.path.join(os.path.dirname(__file__), "..", "crates", "app", "assets", "fonts")


def font(name, size):
    return ImageFont.truetype(os.path.join(FONTS, name), int(size * K))


def s(v):
    return v * K


def draw(version, unsigned):
    img = Image.new("RGB", (W * K, H * K), BG)
    # A soft glow round the icon, and a fainter one from the top left.
    glow = Image.new("RGBA", img.size, (0, 0, 0, 0))
    g = ImageDraw.Draw(glow)
    cx, cy = ICON
    g.ellipse([s(cx - 120), s(cy - 120), s(cx + 120), s(cy + 120)], fill=ACCENT + (70,))
    g.ellipse([s(-140), s(-180), s(220), s(120)], fill=ACCENT + (26,))
    glow = glow.filter(ImageFilter.GaussianBlur(s(60)))
    img.paste(glow, (0, 0), glow)
    d = ImageDraw.Draw(img, "RGBA")
    # Rings round it, fading out.
    for i, r in enumerate([92, 116, 142]):
        a = [42, 26, 14][i]
        d.ellipse([s(cx - r), s(cy - r), s(cx + r), s(cy + r)], outline=ACCENT + (a,), width=s(1))
    # The spectrum and the curve along the bottom.
    base, top = H, 300
    pts = []
    for i in range(241):
        x = W * i / 240
        u = x / W
        y = top + 52 - 26 * max(0.0, 1 - abs(u - 0.4) * 1.7) + 5 * math.sin(u * 37) * math.cos(u * 13)
        pts.append((s(x), s(y)))
    d.polygon([(0, s(base))] + pts + [(s(W), s(base))], fill=ACCENT + (24,))

    def bell(u, c, w, g):
        return g * math.exp(-(((u - c) / w) ** 2))

    curve = []
    for i in range(321):
        x = W * i / 320
        u = x / W
        y = top + 34 - 30 * (bell(u, 0.2, 0.08, 0.9) + bell(u, 0.55, 0.12, -0.5) + bell(u, 0.84, 0.07, 0.7))
        y -= 12 / (1 + math.exp(-(u - 0.96) * 30))
        curve.append((s(x), s(y)))
    # The line and its glow each drawn whole on a layer of their own (overlapping see-through
    # segments would show as hatching), the glow blurred.
    for width, blur, opacity in [(8, 10, 0.35), (2, 0, 0.95)]:
        layer = Image.new("L", img.size, 0)
        ImageDraw.Draw(layer).line(curve, fill=255, width=s(width), joint="curve")
        if blur:
            layer = layer.filter(ImageFilter.GaussianBlur(s(blur) / K))
        img.paste(Image.new("RGB", img.size, CURVE), (0, 0), layer.point(lambda v: int(v * opacity)))
    d = ImageDraw.Draw(img, "RGBA")
    # Grid lines, faint.
    for k in range(1, 8):
        x = W * k / 8
        d.line([(s(x), s(top - 10)), (s(x), s(H))], fill=(255, 255, 255, 8), width=s(1))
    # Pina Studio, and its version.
    bold = font("Geist-Bold.ttf", 20)
    d.text((s(28), s(24)), "Pina Studio", font=bold, fill=TEXT)
    tw = d.textlength("Pina Studio", font=bold)
    d.text((s(28) + tw + s(8), s(31)), version, font=font("GeistMono-Regular.ttf", 11), fill=TEXT_3)
    # The plate the Finder's label sits on.
    py = cy + ICON_SIZE / 2 + 12
    d.rounded_rectangle(
        [s(cx - 78), s(py - 14), s(cx + 78), s(py + 14)],
        radius=s(14),
        fill=PLATE + (255,),
        outline=(255, 255, 255, 30),
        width=s(1),
    )
    # What to do.
    say = "Double-click to install"
    f = font("Geist-Medium.ttf", 13)
    d.text((s(cx) - d.textlength(say, font=f) / 2, s(py + 26)), say, font=f, fill=TEXT_2)
    if unsigned:
        note = "If macOS won't open it: System Settings › Privacy & Security › Open Anyway"
        f = font("Geist-Regular.ttf", 10.5)
        d.text((s(W / 2) - d.textlength(note, font=f) / 2, s(H - 22)), note, font=f, fill=TEXT_3)
    return img


def main():
    out, version = sys.argv[1], sys.argv[2]
    unsigned = "--unsigned" in sys.argv
    big = draw(version, unsigned)
    big.resize((W * 2, H * 2), Image.LANCZOS).save(os.path.join(out, "background@2x.png"))
    big.resize((W, H), Image.LANCZOS).save(os.path.join(out, "background.png"))


if __name__ == "__main__":
    main()

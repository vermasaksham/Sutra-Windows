"""
Sutra's icon: a thread tied in a knot.

सूत्र means "thread", and the app's whole argument is that notes are threads
tied to each other and to the literature. A trefoil is the simplest knot that
cannot be untied — three crossings, three-fold symmetry — so it stays legible
when Windows draws it at 16 pixels, where anything with a serif or a gradient
turns to mush.

Two things this went wrong on first, both worth keeping written down:

The stroke is stamped as overlapping discs rather than drawn with `line(...,
joint="curve")`. That call renders a wide curve as a fan of polygons and leaves
seams between them, which downsample into visible striations along the thread.

The gaps that make a strand pass *under* are cut out of an alpha mask, not
painted in the tile's colour. Painting them meant sampling one pixel of a
gradient and smearing it across a whole window, which left a visible halo
around every crossing.
"""

import math
from PIL import Image, ImageDraw

S = 1024          # final size
SS = 4            # supersample
N = 3000          # curve samples
W = S * SS

GROUND_TOP = (194, 83, 52)     # persimmon, lit
GROUND_BOT = (150, 60, 38)     # persimmon, shadowed
THREAD = (250, 248, 245)       # the app's warm paper

STROKE = int(0.076 * W)        # thread thickness
GAP = int(0.022 * W)           # the break that shows a strand passing under
RADIUS = int(0.22 * W)         # corner radius of the tile


def trefoil(t):
    """The classic trefoil projection, scaled to the tile."""
    x = math.sin(t) + 2 * math.sin(2 * t)
    y = math.cos(t) - 2 * math.cos(2 * t)
    k = W * 0.148                      # 3-unit radius -> comfortable margin
    return (W / 2 + x * k, W / 2 + y * k)


def segments_cross(p1, p2, p3, p4):
    """Do segments p1p2 and p3p4 cross? Returns the crossing point or None."""
    d = (p2[0] - p1[0]) * (p4[1] - p3[1]) - (p2[1] - p1[1]) * (p4[0] - p3[0])
    if abs(d) < 1e-9:
        return None
    a = ((p3[0] - p1[0]) * (p4[1] - p3[1]) - (p3[1] - p1[1]) * (p4[0] - p3[0])) / d
    b = ((p3[0] - p1[0]) * (p2[1] - p1[1]) - (p3[1] - p1[1]) * (p2[0] - p1[0])) / d
    if 0 <= a <= 1 and 0 <= b <= 1:
        return (p1[0] + a * (p2[0] - p1[0]), p1[1] + a * (p2[1] - p1[1]))
    return None


def stamp(draw, points, value, width):
    """A smooth round-capped stroke: one disc per sample, no seams."""
    r = width / 2
    for x, y in points:
        draw.ellipse([x - r, y - r, x + r, y + r], fill=value)


pts = [trefoil(2 * math.pi * i / N) for i in range(N)]
length = sum(math.dist(pts[i], pts[(i + 1) % N]) for i in range(N))

# A gap only has to clear the strand passing beneath it: its width plus the
# break either side. Longer, and the knot stops reading as one continuous
# thread and starts looking like six separate bars.
span = max(2, round((STROKE + 2 * GAP) * 0.62 / (length / N)))

crossings = []
skip = N // 20
for i in range(N):
    for j in range(i + skip, N - (skip if i < skip else 0)):
        hit = segments_cross(pts[i], pts[(i + 1) % N], pts[j], pts[(j + 1) % N])
        if hit and all(math.dist(hit, c[2]) > W * 0.05 for c in crossings):
            crossings.append((i, j, hit))
print(f"crossings: {len(crossings)}, gap span: {span} samples")

# The thread as a mask. 255 is thread, 0 is tile, and the tile is composited
# through it afterwards — so a gap is genuinely the background, gradient and
# all, rather than a guess at what colour the background was there.
mask = Image.new("L", (W, W), 0)
md = ImageDraw.Draw(mask)
stamp(md, pts, 255, STROKE)
for k, (i, j, _) in enumerate(crossings):
    over = i if k % 2 == 0 else j          # alternate, or it is not a knot
    window = [pts[(over + d) % N] for d in range(-span, span + 1)]
    stamp(md, window, 0, STROKE + 2 * GAP)
    stamp(md, window, 255, STROKE)

gradient = Image.new("RGB", (1, W))
gp = gradient.load()
for y in range(W):
    f = y / (W - 1)
    gp[0, y] = tuple(
        round(GROUND_TOP[i] + (GROUND_BOT[i] - GROUND_TOP[i]) * f) for i in range(3)
    )
tile = gradient.resize((W, W))
tile.paste(Image.new("RGB", (W, W), THREAD), (0, 0), mask)

corners = Image.new("L", (W, W), 0)
ImageDraw.Draw(corners).rounded_rectangle([0, 0, W - 1, W - 1], RADIUS, fill=255)
out = Image.new("RGBA", (W, W), (0, 0, 0, 0))
out.paste(tile, (0, 0), corners)

out.resize((S, S), Image.LANCZOS).save("app-icon.png")
print("wrote app-icon.png")

# Regenerate the shipped icon set with:
#     python3 tools/draw-icon.py && npx tauri icon app-icon.png
# then delete src-tauri/icons/android and .../ios, which the generator writes
# for platforms this app does not target.

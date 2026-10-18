"""Builds the logo files in this folder from wordmark-source.png.

Run it after changing wordmark-source.png: `python assets/branding/build.py`
(needs Pillow and NumPy). It cuts the wordmark out of its white background,
makes the dark-background version, redraws the hexagon mark as strokes (the
letters cover parts of it in the source) and makes the app icons.
"""
import itertools
import os
import numpy as np
from PIL import Image, ImageDraw

OUT = os.path.dirname(os.path.abspath(__file__)) + os.sep
src = np.asarray(Image.open(OUT + "wordmark-source.png").convert("RGB")).astype(np.float32)

# The logo's palette, and the same colours for dark backgrounds.
PALETTE = {
    "blue": ((0, 74, 173), (77, 134, 232)),
    "dgrey": ((84, 84, 84), (238, 238, 242)),
    "lgrey": ((170, 170, 170), (150, 152, 162)),
    "orange": ((241, 137, 45), (241, 137, 45)),
    "yellow": ((249, 191, 59), (249, 191, 59)),
    "lyellow": ((255, 207, 75), (255, 207, 75)),
}
names = list(PALETTE)
light = np.array([PALETTE[n][0] for n in names], np.float32)
dark = np.array([PALETTE[n][1] for n in names], np.float32)
W = np.float32(255)

h, w, _ = src.shape
pix = src.reshape(-1, 3)

# 1. Each colour mixed with white: p = a*C + (1-a)*white.
d = W - pix  # (N,3)
dc = W - light  # (K,3)
a = np.clip((d @ dc.T) / (dc * dc).sum(1), 0, 1)  # (N,K)
fit = a[:, :, None] * light[None] + (1 - a[:, :, None]) * W
err = np.abs(fit - pix[:, None]).sum(2)  # (N,K)

# Prefer colours that appear pure nearby (radius 2), so an anti-aliased dark
# outline isn't mistaken for the light grey fill.
pure = (np.abs(pix[:, None] - light[None]).sum(2) < 8).reshape(h, w, len(names))
near = np.zeros_like(pure, dtype=np.int32)
for dy, dx in itertools.product(range(-2, 3), repeat=2):
    near += np.roll(np.roll(pure, dy, 0), dx, 1)
near = near.reshape(-1, len(names))
has_near = near.sum(1) > 0
score = err - np.where(has_near[:, None], np.minimum(near, 25) * 0.6, 0) + np.where(
    has_near[:, None] & (near == 0), 40, 0)
k = score.argmin(1)
idx = np.arange(len(pix))
alpha = a[idx, k]
e1 = err[idx, k]

# 2. Pixels that are a mix of two colours (opaque edges between them).
best_pair = np.full(len(pix), 1e9, np.float32)
pair_i = np.zeros(len(pix), int)
pair_j = np.zeros(len(pix), int)
pair_t = np.zeros(len(pix), np.float32)
for i, j in itertools.combinations(range(len(names)), 2):
    c1, c2 = light[i], light[j]
    v = c1 - c2
    t = np.clip(((pix - c2) @ v) / (v @ v), 0, 1)
    e = np.abs(t[:, None] * c1 + (1 - t[:, None]) * c2 - pix).sum(1)
    better = e < best_pair
    best_pair[better], pair_i[better], pair_j[better], pair_t[better] = e[better], i, j, t[better]

white = pix.min(1) >= 252
use_pair = (best_pair + 6 < e1) & ~white


# The pale circles: see-through yellow, never more than about half opaque nearby.
al2 = alpha.reshape(h, w).copy()
al2[white.reshape(h, w)] = 0
nearmax = al2.copy()
for dy, dx in itertools.product(range(-3, 4), repeat=2):
    nearmax = np.maximum(nearmax, np.roll(np.roll(al2, dy, 0), dx, 1))
pale = (nearmax.reshape(-1) < 0.6) & (k == names.index("yellow")) & ~use_pair


def render(colours, pale_boost=1.0):
    rgb = colours[k].copy()
    rgb[use_pair] = (pair_t[use_pair, None] * colours[pair_i[use_pair]]
                     + (1 - pair_t[use_pair, None]) * colours[pair_j[use_pair]])
    al = np.where(use_pair, 1.0, alpha)
    al[pale] = np.minimum(al[pale] * pale_boost, 1)
    al[white] = 0
    rgba = np.concatenate([rgb, al[:, None] * 255], 1).reshape(h, w, 4)
    return Image.fromarray(np.clip(rgba + 0.5, 0, 255).astype(np.uint8), "RGBA")


def trim(im, pad=12):
    x0, y0, x1, y1 = im.getchannel("A").point(lambda v: 255 if v > 8 else 0).getbbox()
    return im.crop((x0 - pad, y0 - pad, x1 + pad, y1 + pad))


for colours, name, boost in ((light, "wordmark.png", 1.0), (dark, "wordmark-dark.png", 1.7)):
    im = trim(render(colours, boost))
    im = im.resize((840, round(im.height * 840 / im.width)), Image.LANCZOS)
    im.save(OUT + name, optimize=True)

# 3. The hexagon mark, redrawn as strokes (the letters cover parts of it).
STROKE = 23
STROKES = [
    [(258, 243), (515, 287), (621, 494), (548, 757), (285, 826), (76, 677), (62, 417), (258, 243)],
    [(173, 330), (218, 452), (82, 672)],
    [(258, 243), (345, 518), (617, 513)],
    [(190, 738), (345, 518)],
    [(385, 282), (426, 410), (573, 405)],
    [(285, 826), (430, 635), (584, 631)],
]
BOX = (30, 224, 650, 844)  # a square around the mark in the source image


def mark(size, colour, scale=4, inset=0.0):
    big = size * scale
    x0, y0, x1, _ = BOX
    k_ = big * (1 - 2 * inset) / (x1 - x0)
    off = big * inset
    im = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    dr = ImageDraw.Draw(im)
    sw = STROKE * k_
    for s in STROKES:
        pts = [((x - x0) * k_ + off, (y - y0) * k_ + off) for x, y in s]
        dr.line(pts, fill=colour, width=round(sw), joint="curve")
        for px_, py_ in pts:
            dr.ellipse([px_ - sw / 2, py_ - sw / 2, px_ + sw / 2, py_ + sw / 2], fill=colour)
    return im.resize((size, size), Image.LANCZOS)


mark(1024, (0, 74, 173)).save(OUT + "mark.png", optimize=True)
# White, so the editor can tint it to suit the theme.
mark(40, (255, 255, 255)).save(OUT + "mark-white-40.png", optimize=True)

# The vector master.
x0, y0, x1, _ = BOX
paths = "\n".join(
    '  <polyline points="%s"/>' % " ".join("%d,%d" % (x - x0, y - y0) for x, y in s) for s in STROKES)
open(OUT + "mark.svg", "w").write(
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {x1 - x0} {x1 - x0}">\n'
    f'<g fill="none" stroke="#004aad" stroke-width="{STROKE}" stroke-linecap="round" stroke-linejoin="round">\n'
    f"{paths}\n</g>\n</svg>\n")


# 4. App icon: the mark on a white rounded tile, readable on light and dark taskbars.
def icon(size):
    s = size * 4
    tile = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    ImageDraw.Draw(tile).rounded_rectangle([s * 0.03, s * 0.03, s * 0.97, s * 0.97], radius=s * 0.22,
                                           fill=(255, 255, 255, 255))
    tile = tile.resize((size, size), Image.LANCZOS)
    tile.alpha_composite(mark(size, (0, 74, 173), inset=0.14))
    return tile


icon(1024).save(OUT + "icon.png", optimize=True)
icon(256).save(OUT + "icon-256.png", optimize=True)
icon(32).save(OUT + "favicon-32.png", optimize=True)
icon(256).save(OUT + "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])

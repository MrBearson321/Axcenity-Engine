"""Builds the logo files in this folder from mark-source.png and wordmark-source.png.

Run after changing a source: `python assets/branding/build.py` (needs Pillow
and NumPy). The sources are black ink on white. The white is keyed out by
brightness, so the logo files are ink on a transparent background, in black
and in white. Only the app icon has a plate: a rounded charcoal tile.
"""

import os
from collections import deque

import numpy as np
from PIL import Image, ImageDraw

OUT = os.path.dirname(os.path.abspath(__file__)) + os.sep

BLACK = (0, 0, 0)
WHITE = (255, 255, 255)
CHARCOAL = (24, 24, 27)
# Brightness at or below INK is solid ink; at or above PAPER it is background.
# Between them is the anti-aliased edge. PAPER sits under the sources' faint
# paper texture, so none of it survives.
INK = 40
PAPER = 215
# Specks of ink smaller than this many pixels are noise, not logo.
MIN_SPECK = 64


def key(name: str) -> np.ndarray:
    """The source's ink as alpha (0 to 1), with the background removed."""
    gray = np.asarray(Image.open(OUT + name).convert("L")).astype(np.float32)
    alpha = np.clip((PAPER - gray) / (PAPER - INK), 0.0, 1.0)
    return despeckle(alpha)


def despeckle(alpha: np.ndarray) -> np.ndarray:
    """Clears small islands of ink (8-connected), keeping the real shapes."""
    h, w = alpha.shape
    ink = alpha > 0.0
    seen = np.zeros_like(ink)
    for y0, x0 in zip(*np.nonzero(ink)):
        if seen[y0, x0]:
            continue
        seen[y0, x0] = True
        island = [(y0, x0)]
        queue = deque(island)
        while queue:
            y, x = queue.popleft()
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    ny, nx = y + dy, x + dx
                    if 0 <= ny < h and 0 <= nx < w and ink[ny, nx] and not seen[ny, nx]:
                        seen[ny, nx] = True
                        island.append((ny, nx))
                        queue.append((ny, nx))
        if len(island) < MIN_SPECK:
            ys, xs = zip(*island)
            alpha[list(ys), list(xs)] = 0.0
    return alpha


def crop(alpha: np.ndarray, pad_frac: float) -> np.ndarray:
    """Crops to the ink, plus `pad_frac` of its longer side as clear margin."""
    rows, cols = np.nonzero(alpha)
    if rows.size == 0:
        raise SystemExit("no ink in the source")
    y0, y1 = rows.min(), rows.max() + 1
    x0, x1 = cols.min(), cols.max() + 1
    pad = round(max(x1 - x0, y1 - y0) * pad_frac)
    return np.pad(alpha[y0:y1, x0:x1], pad)


def square(alpha: np.ndarray) -> np.ndarray:
    """Centers the ink in a square of clear background."""
    h, w = alpha.shape
    side = max(h, w)
    out = np.zeros((side, side), np.float32)
    y, x = (side - h) // 2, (side - w) // 2
    out[y : y + h, x : x + w] = alpha
    return out


def ink(alpha: np.ndarray, colour: tuple[int, int, int]) -> Image.Image:
    """`colour` everywhere, with the ink as alpha: no fringe at any size."""
    h, w = alpha.shape
    rgba = np.empty((h, w, 4), np.uint8)
    rgba[..., :3] = colour
    rgba[..., 3] = np.clip(alpha * 255.0 + 0.5, 0, 255).astype(np.uint8)
    return Image.fromarray(rgba, "RGBA")


def resize_width(im: Image.Image, width: int) -> Image.Image:
    return im.resize((width, round(im.height * width / im.width)), Image.LANCZOS)


def tile(mark: Image.Image, side: int) -> Image.Image:
    """The white A on a rounded charcoal tile with see-through corners."""
    big = 1024
    canvas = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ImageDraw.Draw(canvas).rounded_rectangle(
        (0, 0, big - 1, big - 1), radius=round(big * 0.22), fill=CHARCOAL + (255,)
    )
    inner = round(big * 0.66)
    fitted = mark.resize((inner, inner), Image.LANCZOS)
    origin = (big - inner) // 2
    canvas.alpha_composite(fitted, (origin, origin))
    return canvas if side == big else canvas.resize((side, side), Image.LANCZOS)


def save(im: Image.Image, name: str) -> None:
    im.save(OUT + name, optimize=True)


mark = square(crop(key("mark-source.png"), 0.0))
mark_black = resize_width(ink(mark, BLACK), 1024)
mark_white = resize_width(ink(mark, WHITE), 1024)
save(mark_black, "mark.png")
save(mark_white, "mark-white.png")
save(resize_width(mark_white, 48), "mark-white-small.png")

word = crop(key("wordmark-source.png"), 0.02)
save(resize_width(ink(word, BLACK), 960), "wordmark.png")
save(resize_width(ink(word, WHITE), 960), "wordmark-white.png")

save(tile(mark_white, 1024), "icon.png")
save(tile(mark_white, 256), "icon-256.png")
save(tile(mark_white, 32), "favicon-32.png")
tile(mark_white, 256).save(
    OUT + "icon.ico",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)

# Files from the opaque-plate builds, no longer made.
for old in ("mark.svg", "wordmark-dark.png", "wordmark-dark-small.png", "wordmark-small.png"):
    if os.path.exists(OUT + old):
        os.remove(OUT + old)

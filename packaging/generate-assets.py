"""Builds the installer and disk-image pictures from the engine branding.

Run from the repository root: `python packaging/generate-assets.py`
Needs Pillow. Uses Inter from `assets/fonts` and the mark/wordmark PNGs.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[1]
BRAND = ROOT / "assets" / "branding"
FONTS = ROOT / "assets" / "fonts"
OUT = Path(__file__).resolve().parent / "assets"

BLUE = (0, 74, 173)
BLUE_DARK = (8, 28, 64)
BLUE_MID = (0, 58, 140)
ORANGE = (241, 137, 45)
YELLOW = (249, 191, 59)
YELLOW_LIGHT = (255, 207, 75)
WHITE = (255, 255, 255)
INK = (84, 84, 84)
INK_SOFT = (120, 124, 132)
PAGE = (247, 248, 252)
NAVY = (14, 26, 43)


def font(name: str, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(FONTS / name), size)


def load(name: str) -> Image.Image:
    return Image.open(BRAND / name).convert("RGBA")


def paste(dst: Image.Image, src: Image.Image, xy: tuple[int, int]) -> None:
    dst.alpha_composite(src, xy)


def fit(im: Image.Image, size: int) -> Image.Image:
    return im.resize((size, size), Image.LANCZOS)


def text_size(draw: ImageDraw.ImageDraw, text: str, fnt: ImageFont.FreeTypeFont) -> tuple[int, int]:
    x0, y0, x1, y1 = draw.textbbox((0, 0), text, font=fnt)
    return x1 - x0, y1 - y0


def centered_text(
    draw: ImageDraw.ImageDraw,
    text: str,
    fnt: ImageFont.FreeTypeFont,
    cx: int,
    y: int,
    fill: tuple[int, int, int],
) -> int:
    w, h = text_size(draw, text, fnt)
    draw.text((cx - w // 2, y), text, font=fnt, fill=fill)
    return h


def dots(im: Image.Image, origin: tuple[int, int], scale: float = 1.0) -> None:
    """A few of the wordmark's yellow/orange dots, as decoration."""
    draw = ImageDraw.Draw(im)
    cx, cy = origin
    spots = (
        (0, 0, 7, YELLOW),
        (16, -6, 4, ORANGE),
        (-14, 8, 3, YELLOW_LIGHT),
        (10, 12, 3, YELLOW),
        (-8, -10, 2, ORANGE),
    )
    for dx, dy, r, colour in spots:
        x = cx + round(dx * scale)
        y = cy + round(dy * scale)
        rr = max(1, round(r * scale))
        draw.ellipse((x - rr, y - rr, x + rr, y + rr), fill=colour + (255,))


def header() -> Image.Image:
    """NSIS header: 150×57, light bar with the mark and name."""
    im = Image.new("RGBA", (150, 57), WHITE + (255,))
    mark = fit(load("mark.png"), 36)
    paste(im, mark, (10, 10))
    draw = ImageDraw.Draw(im)
    title = font("Inter-SemiBold.ttf", 15)
    draw.text((54, 12), "Axcenity", font=title, fill=BLUE)
    sub = font("Inter-Regular.ttf", 11)
    draw.text((54, 30), "Engine", font=sub, fill=ORANGE)
    return im.convert("RGB")


def sidebar() -> Image.Image:
    """NSIS Welcome/Finish bitmap: 164×314, branded navy panel."""
    im = Image.new("RGBA", (164, 314), BLUE_DARK + (255,))
    draw = ImageDraw.Draw(im)
    for y in range(314):
        t = y / 313
        r = round(BLUE_DARK[0] * (1 - t) + BLUE_MID[0] * t)
        g = round(BLUE_DARK[1] * (1 - t) + BLUE_MID[1] * t)
        b = round(BLUE_DARK[2] * (1 - t) + BLUE_MID[2] * t)
        draw.line((0, y, 163, y), fill=(r, g, b, 255))
    draw.rectangle((0, 306, 163, 313), fill=ORANGE + (255,))

    tile = Image.new("RGBA", (96, 96), (0, 0, 0, 0))
    ImageDraw.Draw(tile).rounded_rectangle((0, 0, 95, 95), radius=22, fill=WHITE + (255,))
    tile.alpha_composite(fit(load("mark.png"), 72), (12, 12))
    paste(im, tile, (34, 36))

    semibold = font("Inter-SemiBold.ttf", 20)
    regular = font("Inter-Regular.ttf", 12)
    caption = font("Inter-Medium.ttf", 11)
    y = 148
    y += centered_text(draw, "Axcenity", semibold, 82, y, WHITE) + 2
    y += centered_text(draw, "Editor", semibold, 82, y, ORANGE) + 16

    badge_w, badge_h = 72, 22
    bx, by = 82 - badge_w // 2, y
    draw.rounded_rectangle((bx, by, bx + badge_w, by + badge_h), radius=11, fill=ORANGE + (255,))
    centered_text(draw, "Alpha", caption, 82, by + 3, WHITE)

    dots(im, (28, 255), scale=1.1)
    dots(im, (136, 238), scale=0.8)
    centered_text(draw, "3D game engine", regular, 82, 282, (210, 218, 232))
    return im.convert("RGB")


def dmg_background() -> Image.Image:
    """Finder window background: drag the app onto Applications."""
    w, h = 720, 438
    im = Image.new("RGBA", (w, h), PAGE + (255,))
    draw = ImageDraw.Draw(im)
    draw.rectangle((0, 0, w // 2, h), fill=NAVY + (255,))
    draw.rectangle((0, 0, w, 6), fill=ORANGE + (255,))

    wordmark = load("wordmark-dark.png")
    # Keep the top of the left panel for the wordmark; the .app icon sits below.
    target_w = 260
    target_h = round(wordmark.height * target_w / wordmark.width)
    wordmark = wordmark.resize((target_w, target_h), Image.LANCZOS)
    paste(im, wordmark, ((w // 2 - target_w) // 2, 28))

    subtitle = font("Inter-Regular.ttf", 15)
    centered_text(draw, "Alpha release", subtitle, w // 4, 28 + target_h + 8, (186, 196, 214))

    # Empty rounded well on the right: the Applications alias sits in it.
    zone = (w // 2 + 78, 128, w - 78, 300)
    draw.rounded_rectangle(zone, radius=28, fill=(255, 255, 255, 255), outline=BLUE + (255,), width=3)
    inset = (zone[0] + 10, zone[1] + 10, zone[2] - 10, zone[3] - 10)
    draw.rounded_rectangle(inset, radius=22, outline=(170, 184, 210, 255), width=2)

    # Arrow between the two icon wells.
    ax = w // 2
    ay = 220
    draw.polygon(
        [(ax - 28, ay - 16), (ax + 10, ay), (ax - 28, ay + 16)],
        fill=ORANGE + (255,),
    )
    draw.rectangle((ax - 52, ay - 6, ax - 20, ay + 6), fill=ORANGE + (255,))

    footer = font("Inter-Regular.ttf", 14)
    centered_text(
        draw,
        "Drag Axcenity Editor to Applications to install",
        footer,
        w // 2,
        392,
        INK,
    )
    return im.convert("RGB")


def wizard_banner() -> Image.Image:
    """Optional larger banner (548×312-ish is unused; keep a square icon tile)."""
    size = 256
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(im).rounded_rectangle((8, 8, size - 9, size - 9), radius=56, fill=WHITE + (255,))
    im.alpha_composite(fit(load("mark.png"), 200), (28, 28))
    shadow = Image.new("RGBA", (size + 16, size + 16), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle((12, 14, size + 4, size + 6), radius=56, fill=(0, 0, 0, 40))
    shadow = shadow.filter(ImageFilter.GaussianBlur(6))
    out = Image.new("RGBA", (size + 16, size + 16), (0, 0, 0, 0))
    out.alpha_composite(shadow, (0, 0))
    out.alpha_composite(im, (8, 4))
    return out


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    header_im = header()
    header_im.save(OUT / "nsis-header.bmp")
    header_im.save(OUT / "nsis-header.png", optimize=True)
    sidebar_im = sidebar()
    sidebar_im.save(OUT / "nsis-sidebar.bmp")
    sidebar_im.save(OUT / "nsis-sidebar.png", optimize=True)
    dmg_background().save(OUT / "dmg-background.png", optimize=True)
    wizard_banner().save(OUT / "icon-tile.png", optimize=True)
    print(f"wrote installer pictures in {OUT}")


if __name__ == "__main__":
    main()

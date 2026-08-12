"""Generate Tauri icons: PNG (32, 128, 128@2x, 512) + multi-resolution .ico.

Style: VPN shield with checkmark, dark slate gradient background,
sky-blue accent. Matches the classquiz minimal/shadcn aesthetic.
"""
from __future__ import annotations
import os
import struct
from pathlib import Path

from PIL import Image, ImageDraw

ICON_DIR = Path(r"C:\Users\Алексей\.minimax-agent\projects\singbox-client\src-tauri\icons")
ICON_DIR.mkdir(parents=True, exist_ok=True)

# Palette
BG_TOP = (15, 23, 42)      # slate-950
BG_BOT = (30, 41, 59)      # slate-800
SHIELD = (56, 189, 248)    # sky-400
SHIELD_EDGE = (125, 211, 252)  # sky-300
CHECK = (15, 23, 42)       # slate-950 (inside shield)


def _draw_gradient_background(draw: ImageDraw.ImageDraw, size: int) -> None:
    """Diagonal gradient top-left -> bottom-right."""
    for y in range(size):
        for x in range(size):
            t = (x + y) / (2 * (size - 1))
            r = int(BG_TOP[0] * (1 - t) + BG_BOT[0] * t)
            g = int(BG_TOP[1] * (1 - t) + BG_BOT[1] * t)
            b = int(BG_TOP[2] * (1 - t) + BG_BOT[2] * t)
            draw.point((x, y), fill=(r, g, b))


def _apply_rounded_corners(img: Image.Image, radius: int) -> Image.Image:
    mask = Image.new("L", img.size, 0)
    md = ImageDraw.Draw(mask)
    md.rounded_rectangle([(0, 0), img.size], radius=radius, fill=255)
    out = Image.new("RGBA", img.size, (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    return out


def render_icon(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bg = Image.new("RGBA", (size, size), (0, 0, 0, 255))
    _draw_gradient_background(ImageDraw.Draw(bg), size)
    bg = _apply_rounded_corners(bg, int(size * 0.22))
    img.paste(bg, (0, 0), bg)

    draw = ImageDraw.Draw(img)

    # Shield
    sw = int(size * 0.55)
    sh = int(size * 0.65)
    sx = (size - sw) // 2
    sy = (size - sh) // 2

    # Build shield polygon: top is flat-ish arc, bottom point
    arc_h = int(sh * 0.45)
    shield = [
        (sx, sy + arc_h // 2),
        (sx + int(sw * 0.05), sy),
        (sx + sw - int(sw * 0.05), sy),
        (sx + sw, sy + arc_h // 2),
        (sx + sw, sy + int(sh * 0.55)),
        (sx + sw // 2, sy + sh),
        (sx, sy + int(sh * 0.55)),
    ]
    draw.polygon(shield, fill=SHIELD, outline=SHIELD_EDGE)

    # Checkmark
    pen_w = max(2, size // 14)
    cx = sx + int(sw * 0.28)
    cy = sy + int(sh * 0.50)
    draw.line(
        [(cx, cy), (cx + sw * 0.16, cy + sh * 0.12)],
        fill=CHECK, width=pen_w,
    )
    draw.line(
        [(cx + sw * 0.16, cy + sh * 0.12), (cx + sw * 0.42, cy - sh * 0.18)],
        fill=CHECK, width=pen_w,
    )

    return img


def main() -> None:
    sizes_png = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }
    for name, sz in sizes_png.items():
        img = render_icon(sz)
        img.save(ICON_DIR / name, "PNG")
        print(f"  {name} ({sz}x{sz})")

    # ICO: 16, 32, 48, 64, 128, 256
    ico_sizes = [16, 32, 48, 64, 128, 256]
    base = render_icon(256)
    base.save(ICON_DIR / "icon.ico", format="ICO", sizes=[(s, s) for s in ico_sizes])
    print(f"  icon.ico (multi-res: {ico_sizes})")


if __name__ == "__main__":
    main()

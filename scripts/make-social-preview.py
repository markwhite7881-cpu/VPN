#!/usr/bin/env python3
"""
Generate a 1280x640 GitHub social preview banner for the Cloakwire
GitHub repository.

Layout:
  - Dark navy gradient background
  - Logo (hooded-cloak-with-wifi) on the left
  - Wordmark "Cloakwire" + tagline on the right
  - Small subtle platform tags at the bottom right
"""

import io
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont, ImageFilter


REPO = Path(__file__).resolve().parent.parent
LOGO_PATH = REPO / "src-tauri" / "icons" / "icon.png"
OUT_PATH = REPO / "dist-release" / "social-preview.png"
OUT_PATH.parent.mkdir(parents=True, exist_ok=True)

W, H = 1280, 640

# Colors (matching the app's dark slate theme).
BG_TOP = (10, 14, 26)       # #0a0e1a
BG_BOTTOM = (15, 23, 42)    # #0f172a
ACCENT = (34, 211, 238)     # #22d3ee (cyan-400)
TEXT_PRIMARY = (248, 250, 252)   # #f8fafc
TEXT_SECONDARY = (148, 163, 184) # #94a3b8
TEXT_MUTED = (71, 85, 105)       # #475569
CHIP_BORDER = (51, 65, 85)       # #334155
CHIP_BG = (30, 41, 59)           # #1e293b


def make_gradient(w: int, h: int) -> Image.Image:
    """Vertical gradient from BG_TOP to BG_BOTTOM."""
    img = Image.new("RGB", (w, h), BG_TOP)
    for y in range(h):
        t = y / max(1, h - 1)
        r = int(BG_TOP[0] + (BG_BOTTOM[0] - BG_TOP[0]) * t)
        g = int(BG_TOP[1] + (BG_BOTTOM[1] - BG_TOP[1]) * t)
        b = int(BG_TOP[2] + (BG_BOTTOM[2] - BG_TOP[2]) * t)
        for x in range(w):
            img.putpixel((x, y), (r, g, b))
    return img


def load_font(size: int, weight: str = "regular") -> ImageFont.FreeTypeFont:
    """Try common Windows fonts, fall back to default."""
    candidates = {
        "bold": [
            r"C:\Windows\Fonts\segoeuib.ttf",  # Segoe UI Bold
            r"C:\Windows\Fonts\arialbd.ttf",    # Arial Bold
            r"C:\Windows\Fonts\calibrib.ttf",   # Calibri Bold
        ],
        "regular": [
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
            r"C:\Windows\Fonts\calibri.ttf",
        ],
    }
    for path in candidates.get(weight, []):
        if Path(path).exists():
            try:
                return ImageFont.truetype(path, size)
            except Exception:
                continue
    return ImageFont.load_default()


def make_glow(img: Image.Image, center, radius: int, color, intensity: int = 80) -> None:
    """Draw a soft radial glow onto the image."""
    glow_size = radius * 2
    glow = Image.new("RGBA", (glow_size, glow_size), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    cx, cy = radius, radius
    for r in range(radius, 0, -2):
        a = max(0, int(intensity * (1 - r / radius) ** 2))
        gd.ellipse((cx - r, cy - r, cx + r, cy + r), fill=color + (a,))
    glow = glow.filter(ImageFilter.GaussianBlur(radius // 4))
    img.paste(glow, (center[0] - radius, center[1] - radius), glow)


def draw_chip(d: ImageDraw.ImageDraw, x: int, y: int, text: str,
              font: ImageFont.FreeTypeFont) -> int:
    """Draw a small platform tag. Returns the right edge x."""
    bbox = d.textbbox((0, 0), text, font=font)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]
    pad_x, pad_y = 14, 8
    box_w = tw + pad_x * 2
    box_h = th + pad_y * 2
    # Rounded rectangle.
    d.rounded_rectangle((x, y, x + box_w, y + box_h), radius=8,
                        outline=CHIP_BORDER, width=1, fill=CHIP_BG)
    d.text((x + pad_x, y + pad_y - 1), text, fill=TEXT_SECONDARY, font=font)
    return x + box_w + 10


def main() -> None:
    img = make_gradient(W, H).convert("RGBA")
    draw = ImageDraw.Draw(img)

    # --- Background subtle glow behind the logo ---
    make_glow(img, (340, H // 2), 260, ACCENT, intensity=70)

    # --- Logo (left side) ---
    logo = Image.open(LOGO_PATH).convert("RGBA")
    # Downscale to a friendly height.
    target_h = 360
    ratio = target_h / logo.height
    target_w = int(logo.width * ratio)
    logo = logo.resize((target_w, target_h), Image.LANCZOS)
    # Slight white tint: keep the white parts of the logo, but make
    # sure they read as bright on the dark bg. The rembg'd logo
    # already has white shapes; we just position it.
    logo_x = 130 - target_w // 2 + 100  # biased slightly left-of-center
    logo_y = (H - target_h) // 2
    img.alpha_composite(logo, (logo_x, logo_y))

    # --- Accent line under wordmark ---
    word_x = 660
    word_y = 230
    # Draw a thin 80px cyan bar above the wordmark.
    draw.rectangle((word_x, word_y - 30, word_x + 80, word_y - 26), fill=ACCENT)

    # --- Wordmark "Cloakwire" ---
    f_word = load_font(96, "bold")
    draw.text((word_x, word_y), "Cloakwire", fill=TEXT_PRIMARY, font=f_word)

    # --- Tagline ---
    f_tag = load_font(28, "regular")
    draw.text((word_x, word_y + 130),
              "Privacy-first GUI VPN client",
              fill=TEXT_SECONDARY, font=f_tag)
    draw.text((word_x, word_y + 168),
              "built on top of sing-box",
              fill=TEXT_SECONDARY, font=f_tag)

    # --- Platform chips (bottom right) ---
    f_chip = load_font(18, "regular")
    chips = ["Tauri 2", "Rust", "React", "Windows", "Open Source"]
    chip_y = H - 70
    # Right-align: start from the right edge with padding.
    pad_right = 60
    # Pre-measure to know total width.
    widths = []
    for text in chips:
        bbox = draw.textbbox((0, 0), text, font=f_chip)
        widths.append(bbox[2] - bbox[0] + 14 * 2 + 10)  # box + gap
    total = sum(widths) - 10
    cx = W - pad_right - total
    for text, w in zip(chips, widths):
        cx = draw_chip(draw, cx, chip_y, text, f_chip)

    img.convert("RGB").save(OUT_PATH, "PNG", optimize=True)
    print(f"wrote {OUT_PATH} ({OUT_PATH.stat().st_size} bytes)")


if __name__ == "__main__":
    main()

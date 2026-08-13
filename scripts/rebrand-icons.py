#!/usr/bin/env python3
"""
One-shot icon rebrand script for Cloakwire.

Input:  the user-provided logo PNG (with black background).
Output: the full set of Tauri icon files, all with the background
        removed. The icon set matches what Tauri 2 expects:
          - 32x32.png
          - 128x128.png
          - 128x128@2x.png  (256x256)
          - icon.png          (512x512, used in installer)
          - icon.ico          (multi-resolution .ico for Windows)

Strategy:
  1. Load the source PNG (the user's "Cloakwire" logo).
  2. Remove the black background with rembg (an ONNX model that
     gives a real alpha channel — much nicer than a naive black-
     to-transparent key).
  3. Save the cleaned logo at 1024x1024 (our source for everything
     else). All the smaller icons are downscales of this, which
     keeps the silhouette sharp at every size.
  4. Render the .ico multi-resolution. Tauri-builder's
     `bundle.icon` config wants at minimum 32x32, 128x128 and
     256x256; we render 16, 32, 48, 64, 128, 256 to cover all
     shell sizes Windows might pick.

Usage: py scripts/rebrand-icons.py <input.png> <output_dir>
"""

import sys
from pathlib import Path

from PIL import Image
from rembg import remove


def make_transparent(input_path: Path) -> Image.Image:
    """Return the logo with the background removed, as RGBA."""
    import io
    raw = Image.open(input_path).convert("RGBA")
    # rembg expects PNG bytes (a real file format), not raw
    # RGBA pixels — passing `raw.tobytes()` confuses PIL's
    # `Image.open()` downstream. We re-encode to PNG first.
    buf = io.BytesIO()
    raw.save(buf, format="PNG")
    out_bytes = remove(buf.getvalue())
    return Image.open(io.BytesIO(out_bytes)).convert("RGBA")


def make_icon(transparent: Image.Image, size: int) -> Image.Image:
    """Render a single icon size, padded so the cloak+signal fits
    cleanly into a square with a small breathing margin."""
    # LANCZOS gives the cleanest downscale for high-contrast logos.
    # We do a 5% inset so the icon's corners stay transparent
    # (Windows would otherwise round them awkwardly).
    inset = int(size * 0.05)
    inner = size - 2 * inset
    return transparent.resize((inner, inner), Image.LANCZOS)


def make_ico(transparent: Image.Image, sizes, dest: Path) -> None:
    """Save a multi-resolution .ico with each size baked in.

    PIL's `ico.save(sizes=...)` resizes the input image to each
    requested size automatically, so we just pass the master and
    let it bake all variants. (Trying to pass pre-resized images
    via `append_images` only kept the first frame.)
    """
    transparent.save(
        dest,
        format="ICO",
        sizes=[(s, s) for s in sizes],
    )


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)

    src = Path(sys.argv[1])
    out_dir = Path(sys.argv[2])
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"reading {src}")
    transparent = make_transparent(src)
    # We also save a clean 1024x1024 master copy so future
    # icon work (or marketing assets) can use it without
    # re-running rembg.
    master = transparent.resize((1024, 1024), Image.LANCZOS)
    master.save(out_dir / "icon.png")

    for name, size in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
    ]:
        make_icon(transparent, size).save(out_dir / name)
        print(f"wrote {name} ({size}x{size})")

    ico_sizes = [16, 32, 48, 64, 128, 256]
    make_ico(transparent, ico_sizes, out_dir / "icon.ico")
    print(f"wrote icon.ico (sizes: {ico_sizes})")


if __name__ == "__main__":
    main()

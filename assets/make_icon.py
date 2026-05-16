"""
Build the Windows .ico from the official FH6 logo source PNG.

Pads the portrait-orientation source to a square canvas (transparent
background) so Windows renders it correctly at every icon size.
"""

from PIL import Image
import os

OUT_DIR = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(OUT_DIR, "fh6_source.png")
PNG = os.path.join(OUT_DIR, "icon.png")
ICO = os.path.join(OUT_DIR, "icon.ico")

CANVAS = 1024
PADDING = 0.06  # 6% margin


def build():
    src = Image.open(SRC).convert("RGBA")
    sw, sh = src.size

    target = int(CANVAS * (1.0 - 2 * PADDING))
    scale = min(target / sw, target / sh)
    new_w, new_h = int(sw * scale), int(sh * scale)
    src = src.resize((new_w, new_h), Image.LANCZOS)

    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    x = (CANVAS - new_w) // 2
    y = (CANVAS - new_h) // 2
    canvas.paste(src, (x, y), src)

    canvas.save(PNG)

    sizes = [16, 24, 32, 48, 64, 128, 256]
    frames = [canvas.resize((s, s), Image.LANCZOS) for s in sizes]
    frames[0].save(
        ICO,
        format="ICO",
        sizes=[(s, s) for s in sizes],
        append_images=frames[1:],
    )

    print(f"wrote {PNG}")
    print(f"wrote {ICO}")


if __name__ == "__main__":
    build()

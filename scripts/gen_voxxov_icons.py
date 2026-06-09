"""Generate Voxxov app icon. Run once to (re)generate icons in-place.

Design: bold white "V" on solid #2c66d0 rounded square, with a small
sound-wave line under the V to suggest voice input. RGBA with clean
alpha so rounded corners composite correctly on any taskbar.

Sizes match `tauri.conf.json -> bundle.icon` (32, 128, 128@2x=256).
The .ico is multi-size (16..256) so Windows picks the best match per DPI.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw

ICON_DIR = Path(__file__).parent.parent / "apps/desktop/src-tauri/icons"
BG = (44, 102, 208, 255)  # #2c66d0 — light-mode accent
FG = (255, 255, 255, 255)


def make_icon(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # Rounded square background
    r = max(1, size // 5)
    d.rounded_rectangle([(0, 0), (size - 1, size - 1)], radius=r, fill=BG)
    # Bold "V" — two thick diagonals meeting at the bottom
    margin = size // 5
    thickness = max(2, size // 9)
    bottom = size - margin - max(2, size // 12)  # leave room for wave
    d.line([(margin, margin), (size // 2, bottom)], fill=FG, width=thickness)
    d.line([(size // 2, bottom), (size - margin, margin)], fill=FG, width=thickness)
    # Small sound-wave line under the V
    wave_y = size - max(2, size // 8)
    wave_w = size // 3
    wave_amp = max(1, size // 24)
    x0 = (size - wave_w) // 2
    pts = []
    for i in range(5):
        x = x0 + i * (wave_w // 4)
        y = wave_y - wave_amp if i % 2 == 0 else wave_y + wave_amp
        pts.append((x, y))
    d.line(pts, fill=FG, width=max(1, size // 40))
    return img


def main() -> int:
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    targets = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }
    for name, size in targets.items():
        make_icon(size).save(ICON_DIR / name, format="PNG")
        print(f"wrote {name} ({size}x{size})")
    master = make_icon(512)
    ico = ICON_DIR / "icon.ico"
    master.save(
        ico,
        format="ICO",
        sizes=[
            (16, 16),
            (24, 24),
            (32, 32),
            (48, 48),
            (64, 64),
            (128, 128),
            (256, 256),
        ],
    )
    print(f"wrote {ico.name} (multi-size)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

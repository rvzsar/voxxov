"""Convert Tauri icons to RGBA. Run once to regenerate icons in-place.

Tauri 2.x's generate_context!() macro validates icons in tauri.conf.json
and rejects PNG without an alpha channel. Our source icons are RGB.
"""
import sys
from pathlib import Path
from PIL import Image

ICON_DIR = Path(__file__).parent.parent / "apps/desktop/src-tauri/icons"

def main() -> int:
    pngs = sorted(ICON_DIR.glob("*.png"))
    if not pngs:
        print(f"no PNGs found in {ICON_DIR}", file=sys.stderr)
        return 1
    for p in pngs:
        img = Image.open(p)
        if img.mode != "RGBA":
            rgba = img.convert("RGBA")
            rgba.save(p, format="PNG")
            print(f"converted {p.name} ({img.mode} -> RGBA)")
        else:
            print(f"skipped {p.name} (already RGBA)")
    # Regenerate icon.ico from the largest PNG with multi-size entries.
    largest = max(pngs, key=lambda p: Image.open(p).width)
    img = Image.open(largest).convert("RGBA")
    ico_path = ICON_DIR / "icon.ico"
    img.save(
        ico_path,
        format="ICO",
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print(f"regenerated {ico_path.name} from {largest.name}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())

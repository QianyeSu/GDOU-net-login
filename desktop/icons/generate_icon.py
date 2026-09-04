from pathlib import Path

from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "icon.png"
PNG_SIZES = {
    "icon-preview.png": 512,
    "icon.png": 512,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "64x64.png": 64,
    "32x32.png": 32,
}


def circular_badge() -> Image.Image:
    source = Image.open(SOURCE).convert("RGBA")
    mask = Image.new("L", source.size, 0)
    ImageDraw.Draw(mask).ellipse((0, 0, source.width - 1, source.height - 1), fill=255)
    source.putalpha(mask)
    return source


def main() -> None:
    icon = circular_badge()
    for filename, size in PNG_SIZES.items():
        output = icon if size == icon.width else icon.resize((size, size), Image.Resampling.LANCZOS)
        output.save(ROOT / filename)
    icon.save(
        ROOT / "icon.ico",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )


if __name__ == "__main__":
    main()

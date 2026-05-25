from pathlib import Path
from PIL import Image, ImageChops, ImageDraw, ImageFilter


ROOT = Path(__file__).resolve().parent
SIZE = 1024
RADIUS = 240


def vertical_gradient(size: int, top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    image = Image.new("RGBA", (size, size))
    pixels = image.load()
    for y in range(size):
        t = y / (size - 1)
        color = tuple(int(top[i] * (1 - t) + bottom[i] * t) for i in range(3)) + (255,)
        for x in range(size):
            pixels[x, y] = color
    return image


def add_soft_glow(base: Image.Image) -> None:
    glow = Image.new("RGBA", base.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(glow)
    draw.ellipse((100, 70, 820, 760), fill=(255, 255, 255, 70))
    glow = glow.filter(ImageFilter.GaussianBlur(90))
    base.alpha_composite(glow)


def draw_wifi(image: Image.Image) -> None:
    symbol = Image.new("RGBA", image.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(symbol)
    line = (255, 255, 255, 255)
    center_x = SIZE // 2 - 10
    center_y = SIZE // 2 + 30

    for bbox, width in (
        ((250, 250, 770, 770), 34),
        ((320, 320, 700, 700), 34),
        ((390, 390, 630, 630), 34),
    ):
        draw.arc(bbox, start=214, end=326, fill=line, width=width)

    draw.ellipse((center_x - 44, center_y + 118, center_x + 44, center_y + 206), fill=line)

    image.alpha_composite(symbol)


def add_shadow(image: Image.Image, mask: Image.Image) -> Image.Image:
    shadow = Image.new("RGBA", image.size, (0, 0, 0, 0))
    shadow_mask = mask.filter(ImageFilter.GaussianBlur(28))
    shadow_layer = Image.new("RGBA", image.size, (28, 86, 140, 110))
    shadow.paste(shadow_layer, (0, 22), shadow_mask)
    canvas = Image.new("RGBA", image.size, (0, 0, 0, 0))
    canvas.alpha_composite(shadow)
    canvas.alpha_composite(image)
    return canvas


def build_icon() -> Image.Image:
    gradient = vertical_gradient(SIZE, (220, 243, 255), (107, 185, 240))
    mask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(mask).rounded_rectangle((70, 70, 954, 954), radius=RADIUS, fill=255)

    clipped = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    clipped.paste(gradient, (0, 0), mask)
    add_soft_glow(clipped)

    outline = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    outline_draw = ImageDraw.Draw(outline)
    outline_draw.rounded_rectangle((72, 72, 952, 952), radius=RADIUS, outline=(255, 255, 255, 92), width=4)
    clipped.alpha_composite(outline)

    draw_wifi(clipped)

    return add_shadow(clipped, mask)


def main() -> None:
    icon = build_icon()
    png_path = ROOT / "icon-preview.png"
    ico_path = ROOT / "icon.ico"

    icon.save(png_path)
    icon.save(ico_path, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])


if __name__ == "__main__":
    main()

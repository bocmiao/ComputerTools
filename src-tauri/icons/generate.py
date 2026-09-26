"""生成应用图标：绿色急救箱 + 白色十字（ISO 7010 急救标志的配色，不用受保护的红十字）。

用法：pip install pillow && python src-tauri/icons/generate.py
"""

from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
BODY = (34, 150, 84, 255)  # #229654
HANDLE = (20, 99, 55, 255)  # #146337
SHADE = (27, 124, 69, 255)  # 箱体下半部分稍深
CROSS = (255, 255, 255, 255)


def draw(size: int) -> Image.Image:
    """在 4 倍画布上画，再缩小，边缘更平滑。"""
    s = size * 4
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    def box(x0, y0, x1, y1):
        return [round(x0 * s), round(y0 * s), round(x1 * s), round(y1 * s)]

    # 提手
    d.rounded_rectangle(box(0.34, 0.10, 0.66, 0.32), radius=round(0.07 * s), outline=HANDLE, width=round(0.065 * s))
    # 箱体
    d.rounded_rectangle(box(0.08, 0.24, 0.92, 0.90), radius=round(0.13 * s), fill=BODY)
    d.rounded_rectangle(box(0.08, 0.62, 0.92, 0.90), radius=round(0.13 * s), fill=SHADE)
    d.rectangle(box(0.08, 0.62, 0.92, 0.72), fill=SHADE)
    # 十字
    cx, cy, arm, half = 0.50, 0.57, 0.22, 0.075
    d.rounded_rectangle(box(cx - half, cy - arm, cx + half, cy + arm), radius=round(0.02 * s), fill=CROSS)
    d.rounded_rectangle(box(cx - arm, cy - half, cx + arm, cy + half), radius=round(0.02 * s), fill=CROSS)
    return img.resize((size, size), Image.Resampling.LANCZOS)


def main() -> None:
    big = draw(1024)
    big.save(HERE / "icon.png")
    for name, px in [("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256)]:
        draw(px).save(HERE / name)
    # 每个尺寸单独画，小图标更清楚
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    frames = [draw(px) for px in sizes]
    frames[-1].save(HERE / "icon.ico", format="ICO", sizes=[(px, px) for px in sizes], append_images=frames[:-1])


if __name__ == "__main__":
    main()

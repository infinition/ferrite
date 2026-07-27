#!/usr/bin/env python3
"""Generates the Ferrite icon set.

Motif: a hex nut, a nod to iron oxide, drawn in orange on a rounded charcoal
square. The shape stays readable at 16 pixels because it relies on a solid
silhouette and a wide central hole rather than fine detail.

Everything is drawn at 1024 pixels then downscaled, which provides the
antialiasing for free.

Usage: python tools/make_icon.py
"""

import math
import os

from PIL import Image, ImageDraw

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(BASE, "assets")

SUPERSAMPLE = 1024
BACKGROUND = (34, 37, 42, 255)
ORANGE_HOT = (247, 147, 58, 255)
ORANGE_DEEP = (198, 90, 20, 255)
HOLE = (26, 28, 32, 255)

ICO_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
WINDOW_ICON_EDGE = 64


def hexagon(center, radius, rotation=0.0):
    cx, cy = center
    points = []
    for index in range(6):
        angle = math.radians(60 * index + rotation)
        points.append((cx + radius * math.cos(angle), cy + radius * math.sin(angle)))
    return points


def vertical_gradient(size, top, bottom):
    gradient = Image.new("RGBA", (1, size))
    for y in range(size):
        ratio = y / max(1, size - 1)
        gradient.putpixel((0, y), tuple(
            int(top[i] + (bottom[i] - top[i]) * ratio) for i in range(4)
        ))
    return gradient.resize((size, size))


def build(size=SUPERSAMPLE):
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    # Background: rounded charcoal square.
    margin = size * 0.02
    draw.rounded_rectangle(
        [margin, margin, size - margin, size - margin],
        radius=size * 0.22, fill=BACKGROUND,
    )

    center = (size / 2, size / 2)

    # The nut: a hexagonal silhouette filled with an orange gradient.
    outer = hexagon(center, size * 0.34, rotation=90)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).polygon(outer, fill=255)
    image.paste(vertical_gradient(size, ORANGE_HOT, ORANGE_DEEP), (0, 0), mask)

    # Central hole, which shows the background through and carries the contrast
    # at small sizes.
    draw.ellipse(
        [center[0] - size * 0.145, center[1] - size * 0.145,
         center[0] + size * 0.145, center[1] + size * 0.145],
        fill=HOLE,
    )

    # Light rim on the whole outline, to lift the nut off the background.
    draw.line(outer + [outer[0]], fill=(255, 201, 148, 110), width=int(size * 0.014))

    return image


def main():
    os.makedirs(ASSETS, exist_ok=True)
    master = build()

    master.resize((256, 256), Image.LANCZOS).save(os.path.join(ASSETS, "icon-256.png"))
    master.resize((256, 256), Image.LANCZOS).save(
        os.path.join(ASSETS, "icon.ico"), sizes=ICO_SIZES
    )

    for edge in (32, 180):
        master.resize((edge, edge), Image.LANCZOS).save(
            os.path.join(ASSETS, "icon-%d.png" % edge)
        )

    # Window icon as raw RGBA: that is the format tao expects, and embedding it
    # as is avoids linking a PNG decoder into the binary.
    window_icon = master.resize(
        (WINDOW_ICON_EDGE, WINDOW_ICON_EDGE), Image.LANCZOS
    ).convert("RGBA")
    with open(os.path.join(ASSETS, "icon-64.rgba"), "wb") as handle:
        handle.write(window_icon.tobytes())

    print("icons written to", ASSETS)
    for name in sorted(os.listdir(ASSETS)):
        if name.startswith("icon"):
            path = os.path.join(ASSETS, name)
            print("  %-16s %7d bytes" % (name, os.path.getsize(path)))


if __name__ == "__main__":
    main()

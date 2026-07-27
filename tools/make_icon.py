#!/usr/bin/env python3
"""Genere l'icone de Ferrite.

Motif: un ecrou hexagonal, clin d'oeil a l'oxyde de fer, dessine en orange sur
un carre arrondi gris anthracite. La forme reste lisible a 16 pixels parce
qu'elle repose sur une silhouette pleine et un trou central large.

Le dessin est fait a 1024 pixels puis reduit, ce qui donne l'antialiasing.
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

    # Fond: carre arrondi anthracite.
    margin = size * 0.02
    draw.rounded_rectangle(
        [margin, margin, size - margin, size - margin],
        radius=size * 0.22, fill=BACKGROUND,
    )

    center = (size / 2, size / 2)

    # Ecrou: silhouette hexagonale remplie d'un degrade orange.
    outer = hexagon(center, size * 0.34, rotation=90)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).polygon(outer, fill=255)
    image.paste(vertical_gradient(size, ORANGE_HOT, ORANGE_DEEP), (0, 0), mask)

    # Trou central, qui laisse voir le fond et cree le contraste a petite taille.
    draw.ellipse(
        [center[0] - size * 0.145, center[1] - size * 0.145,
         center[0] + size * 0.145, center[1] + size * 0.145],
        fill=HOLE,
    )

    # Liseré clair sur tout le contour, pour detacher l'ecrou du fond.
    draw.line(outer + [outer[0]], fill=(255, 201, 148, 110), width=int(size * 0.014))

    return image


def main():
    os.makedirs(ASSETS, exist_ok=True)
    master = build()

    png = master.resize((256, 256), Image.LANCZOS)
    png.save(os.path.join(ASSETS, "icon-256.png"))

    sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    master.resize((256, 256), Image.LANCZOS).save(
        os.path.join(ASSETS, "icon.ico"), sizes=sizes
    )

    for edge in (32, 180):
        master.resize((edge, edge), Image.LANCZOS).save(
            os.path.join(ASSETS, "icon-%d.png" % edge)
        )

    # Icone de fenetre en RGBA brut: tao attend ce format, et l'embarquer tel
    # quel evite d'ajouter un decodeur PNG au binaire.
    window_icon = master.resize((64, 64), Image.LANCZOS).convert("RGBA")
    with open(os.path.join(ASSETS, "icon-64.rgba"), "wb") as handle:
        handle.write(window_icon.tobytes())

    print("icone generee dans", ASSETS)
    for name in sorted(os.listdir(ASSETS)):
        if name.startswith("icon"):
            path = os.path.join(ASSETS, name)
            print("  %-16s %6d octets" % (name, os.path.getsize(path)))


if __name__ == "__main__":
    main()

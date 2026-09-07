#!/usr/bin/env python3
"""Regenerate the shared PNG and Windows ICO from the project SVG."""

from io import BytesIO
from pathlib import Path

import cairo
import gi
from PIL import Image

gi.require_version("Rsvg", "2.0")
from gi.repository import Rsvg


def render_icon(source, size):
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, size, size)
    viewport = Rsvg.Rectangle()
    viewport.width = viewport.height = size
    source.render_document(cairo.Context(surface), viewport)
    png = BytesIO()
    surface.write_to_png(png)
    png.seek(0)
    return Image.open(png).convert("RGBA")


def main():
    assets = Path(__file__).resolve().parent.parent / "assets"
    source = Rsvg.Handle.new_from_file(str(assets / "NoiseHoiHoi.svg"))
    sizes = (16, 24, 32, 48, 64, 128, 256)
    images = [render_icon(source, size) for size in sizes]
    images[-1].save(assets / "NoiseHoiHoi.png")
    # DIB entries retain alpha and work with both the Windows shell and NSIS.
    images[-1].save(
        assets / "NoiseHoiHoi.ico",
        format="ICO",
        sizes=[(size, size) for size in sizes],
        append_images=images[:-1],
        bitmap_format="bmp",
    )


if __name__ == "__main__":
    main()

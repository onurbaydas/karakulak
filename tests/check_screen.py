"""Checks that a QEMU screendump shows a user interface, not a blank screen.

Usage: check_screen.py screen.ppm screen.png
Writes a PNG copy for people to look at, exits 1 if the screen looks empty.
"""

import struct
import sys
import zlib


def read_ppm(path):
    with open(path, "rb") as f:
        data = f.read()
    # Header: "P6\n<width> <height>\n<maxval>\n", then RGB bytes.
    fields = data.split(maxsplit=4)
    if fields[0] != b"P6":
        raise ValueError("not a binary PPM")
    width, height = int(fields[1]), int(fields[2])
    pixels = fields[4][: width * height * 3]
    return width, height, pixels


def write_png(path, width, height, pixels):
    rows = b"".join(b"\0" + pixels[y * width * 3 : (y + 1) * width * 3] for y in range(height))

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


def main():
    width, height, pixels = read_ppm(sys.argv[1])
    write_png(sys.argv[2], width, height, pixels)
    # Sample every 7th pixel; a drawn interface has many distinct colors.
    colors = {pixels[i : i + 3] for i in range(0, len(pixels) - 2, 21)}
    print(f"screen {width}x{height}, {len(colors)} distinct colors")
    if len(colors) < 20:
        print("the screen looks empty")
        sys.exit(1)


main()

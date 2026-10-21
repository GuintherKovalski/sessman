#!/usr/bin/env python3
"""Draws assets/icon.png, scope's logo: a grey flower of life in a gapped ring on a navy tile,
a glowing blue dot at its center. No dependencies. Usage: python3 -I scripts/icon.py [out.png]"""
import math, struct, sys, zlib

SIZE = 512
C = SIZE / 2
R = 80  # radius of each circle of the flower
LINE = 5
FLOWER_LINE = 2  # the circles of the flower, thinner than the rings around it
GLOW = (0, 170, 255)
CORE = (90, 225, 255)

# The flower's circles: every point of the hexagonal lattice up to 3R from the center (37),
# drawn only inside the border at 2R.
A, B = (R * math.cos(math.pi / 6), R * math.sin(math.pi / 6)), (0.0, R)
CENTERS = [
    (i * A[0] + j * B[0], i * A[1] + j * B[1])
    for i in range(-4, 5)
    for j in range(-4, 5)
    if math.hypot(i * A[0] + j * B[0], i * A[1] + j * B[1]) <= 3 * R + 0.01
]

def rounded_rect(x, y, half, r):
    qx, qy = abs(x - C) - half + r, abs(y - C) - half + r
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r

def symbol(x, y):
    dx, dy = x - C, y - C
    r = math.hypot(dx, dy)
    flower = min(abs(math.hypot(dx - cx, dy - cy) - R) for cx, cy in CENTERS) - FLOWER_LINE / 2
    flower = max(flower, r - 2 * R)  # clipped to the border circle
    borders = min(abs(r - 2 * R), abs(r - 2.1 * R)) - LINE / 2
    # The outer ring breaks on the four axes.
    outer = max(abs(r - 2.28 * R) - LINE / 2, 7 - min(abs(dx), abs(dy)))
    return flower, min(borders, outer)

def cover(d):
    return max(0.0, min(1.0, 0.5 - d))

def mix(a, b, t):
    return [p * (1 - t) + q * t for p, q in zip(a, b)]

def pixel(x, y):
    x, y = x + 0.5, y + 0.5
    tile = cover(rounded_rect(x, y, 206, 92))
    if tile == 0:
        return (0, 0, 0, 0)
    r = math.hypot(x - C, y - C)
    rgb = mix((26, 33, 46), (10, 13, 20), min(1.0, r / 300))
    t = (y - 50) / 412
    flower, rings = symbol(x, y)
    rgb = mix(rgb, mix((168, 172, 180), (100, 104, 112), t), cover(rings))
    rgb = mix(rgb, mix((240, 242, 246), (200, 204, 210), t), cover(flower))
    glow = 0.9 * math.exp(-((r / 20) ** 2)) + 0.3 * math.exp(-((r / 50) ** 2))
    rgb = [min(255, c + g * glow) for c, g in zip(rgb, GLOW)]
    rgb = mix(rgb, mix(CORE, (225, 252, 255), max(0.0, 1 - r / 7)), cover(r - 11))
    return tuple(round(c) for c in rgb) + (round(255 * tile),)

def png(path):
    rows = b"".join(b"\0" + bytes(v for x in range(SIZE) for v in pixel(x, y)) for y in range(SIZE))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(rows, 9)))
        f.write(chunk(b"IEND", b""))

png(sys.argv[1] if len(sys.argv) > 1 else "assets/icon.png")

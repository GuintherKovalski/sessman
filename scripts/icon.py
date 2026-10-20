#!/usr/bin/env python3
"""Draws assets/icon.png, scope's logo: grey reticle rings on a charcoal tile around a glowing
blue center dot. No dependencies. Usage: python3 -I scripts/icon.py [out.png]"""
import math, struct, sys, zlib

SIZE = 512
C = SIZE / 2
GLOW = (40, 150, 230)
CORE = (130, 220, 255)

def rounded_rect(x, y, half, r):
    qx, qy = abs(x - C) - half + r, abs(y - C) - half + r
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r

def segment(x, y, ax, ay, bx, by, w):
    px, py, dx, dy = x - ax, y - ay, bx - ax, by - ay
    t = max(0, min(1, (px * dx + py * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - dx * t, py - dy * t) - w / 2

def reticle(x, y):
    dx, dy, r = x - C, y - C, math.hypot(x - C, y - C)
    rings = min(abs(r - radius) - 3.5 for radius in (88, 122, 156))
    # Each ring breaks where the crosshair passes.
    rings = max(rings, 15 - min(abs(dx), abs(dy)))
    lines = min(
        segment(x, y, C, C - 182, C, C - 58, 6),
        segment(x, y, C, C + 58, C, C + 182, 6),
        segment(x, y, C - 182, C, C - 58, C, 6),
        segment(x, y, C + 58, C, C + 182, C, 6),
    )
    return min(rings, lines)

def cover(d):
    return max(0.0, min(1.0, 0.5 - d))

def mix(a, b, t):
    return [p * (1 - t) + q * t for p, q in zip(a, b)]

def pixel(x, y):
    x, y = x + 0.5, y + 0.5
    tile = cover(rounded_rect(x, y, 206, 92))
    if tile == 0:
        return (0, 0, 0, 0)
    t = (y - 50) / 412
    rgb = mix((36, 38, 43), (14, 15, 18), t)
    rgb = mix(rgb, mix((190, 194, 200), (105, 109, 116), t), cover(reticle(x, y)))
    r = math.hypot(x - C, y - C)
    glow = 0.85 * math.exp(-((r / 34) ** 2)) + 0.25 * math.exp(-((r / 70) ** 2))
    rgb = [min(255, c + g * glow) for c, g in zip(rgb, GLOW)]
    rgb = mix(rgb, mix(CORE, (235, 250, 255), max(0.0, 1 - r / 9)), cover(r - 17))
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

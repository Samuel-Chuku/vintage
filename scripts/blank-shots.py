"""Flag blank screenshots: python3 scripts/blank-shots.py <dir>

Decodes each PNG (8-bit RGB/RGBA, no dependencies) and samples ~60 rows; a capture with at most two
distinct sampled rows is almost certainly blank (background only).
"""
import os
import struct
import sys
import zlib


def unfilter(raw, w, h, bpp):
    stride = w * bpp
    out = bytearray()
    prev = bytearray(stride)
    i = 0
    for _ in range(h):
        f = raw[i]
        line = bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        out += line
        prev = line
    return out


def distinct_rows(path):
    d = open(path, "rb").read()
    w, h = struct.unpack(">II", d[16:24])
    ct = d[25]
    bpp = {2: 3, 6: 4}[ct]
    idat, i = b"", 8
    while i < len(d):
        n = struct.unpack(">I", d[i:i + 4])[0]
        if d[i + 4:i + 8] == b"IDAT":
            idat += d[i + 8:i + 8 + n]
        i += 12 + n
    px = unfilter(zlib.decompress(idat), w, h, bpp)
    rows = set()
    for y in range(0, h, max(1, h // 60)):
        rows.add(bytes(px[y * w * bpp:(y + 1) * w * bpp:bpp * 7]))
    return w, h, len(rows)


d = sys.argv[1]
for f in sorted(os.listdir(d)):
    if f.endswith(".png"):
        w, h, k = distinct_rows(os.path.join(d, f))
        print(f"{f:44s} {w}x{h:<5d} {'BLANK' if k <= 2 else 'content'}")

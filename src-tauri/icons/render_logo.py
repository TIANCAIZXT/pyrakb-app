#!/usr/bin/env python3
import math
from PIL import Image, ImageDraw

W = 1024

def cubic(p0, p1, p2, p3, n=24):
    pts = []
    for i in range(n + 1):
        t = i / n
        mt = 1 - t
        x = mt**3*p0[0] + 3*mt**2*t*p1[0] + 3*mt*t**2*p2[0] + t**3*p3[0]
        y = mt**3*p0[1] + 3*mt**2*t*p1[1] + 3*mt*t**2*p2[1] + t**3*p3[1]
        pts.append((x, y))
    return pts

def poly(*segs):
    pts = []
    for seg in segs:
        pts.extend(seg)
    return pts

# ---- background gradient + rounded mask ----
grad = Image.new("RGBA", (W, W))
dg = ImageDraw.Draw(grad)
top = (0x3B, 0x78, 0xF0)
bot = (0x2E, 0x6B, 0xE6)
for y in range(W):
    t = y / (W - 1)
    r = int(top[0] + (bot[0]-top[0])*t)
    g = int(top[1] + (bot[1]-top[1])*t)
    b = int(top[2] + (bot[2]-top[2])*t)
    dg.line([(0, y), (W, y)], fill=(r, g, b, 255))

mask = Image.new("L", (W, W), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, W-1, W-1], radius=224, fill=255)
img = Image.composite(grad, Image.new("RGBA", (W, W)), mask)
d = ImageDraw.Draw(img)

# ---- book pages ----
left = poly(
    cubic((512, 368), (416, 328), (296, 320), (224, 352)),
    [(224, 688)],
    cubic((224, 688), (304, 656), (424, 672), (512, 720)),
)
right = poly(
    cubic((512, 368), (608, 328), (728, 320), (800, 352)),
    [(800, 688)],
    cubic((800, 688), (720, 656), (600, 672), (512, 720)),
)
d.polygon(left, fill=(255, 255, 255, 255))
d.polygon(right, fill=(0xEA, 0xF1, 0xFE, 255))

# spine
d.line([(512, 368), (512, 720)], fill=(0x2E, 0x6B, 0xE6, int(0.22*255)), width=12)

# text lines (left page)
blue = (0x2E, 0x6B, 0xE6, int(0.85*255))
for (x1, y1, x2, y2) in [(272, 440, 432, 432), (272, 504, 448, 496), (272, 568, 400, 564)]:
    d.line([(x1, y1), (x2, y2)], fill=blue, width=16)

# node network (right page)
node = (0x2E, 0x6B, 0xE6, int(0.9*255))
for (x1, y1, x2, y2) in [(512, 448, 572, 512), (572, 512, 512, 568),
                         (512, 568, 452, 512), (452, 512, 512, 448)]:
    d.line([(x1, y1), (x2, y2)], fill=node, width=12)
for (cx, cy) in [(512, 448), (572, 512), (512, 568), (452, 512)]:
    d.ellipse([cx-20, cy-20, cx+20, cy+20], fill=(0x2E, 0x6B, 0xE6, 255))

img.save("logo_1024.png")
print("logo_1024.png saved", img.size)

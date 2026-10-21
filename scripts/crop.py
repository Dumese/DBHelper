"""裁剪原型图区域用于逐像素比对。"""
import os
import sys
from PIL import Image

src_path = sys.argv[1] if len(sys.argv) > 1 else r"C:\Users\Wayne\WorkBuddy\2026-10-20-18-40-22\p01-connection-manager.jpg"
out = sys.argv[2] if len(sys.argv) > 2 else r"D:\Code\dbhelper\.shots\proto_p01"
os.makedirs(out, exist_ok=True)
src = Image.open(src_path)
print("src", src.size)
regions = {
    "title": (0, 0, 2880, 90),
    "tree": (0, 90, 520, 1150),
    "card": (700, 380, 2000, 1160),
    "right": (2100, 90, 2880, 800),
    "status": (0, 1700, 2880, 1800),
}
for k, (l, t, r, b) in regions.items():
    if r > src.size[0]:
        r = src.size[0]
    if b > src.size[1]:
        b = src.size[1]
    src.crop((l, t, r, b)).save(os.path.join(out, k + ".png"))
    print(k, (r - l, b - t))

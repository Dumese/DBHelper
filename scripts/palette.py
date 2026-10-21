"""定位原型图中的彩色特征（图标/选中边框/链接色）并取色。"""
from collections import Counter
from PIL import Image


def clusters(path, min_sat=35, top=14):
    img = Image.open(path).convert("RGB")
    w, h = img.size
    buckets = {}
    for y in range(0, h, 2):
        for x in range(0, w, 2):
            r, g, b = img.getpixel((x, y))
            sat = max(r, g, b) - min(r, g, b)
            if sat >= min_sat:
                key = (r // 24, g // 24, b // 24)
                rec = buckets.setdefault(key, [0, 0, 0, 0, []])
                rec[0] += 1
                rec[1] += r
                rec[2] += g
                rec[3] += b
                if len(rec[4]) < 400:
                    rec[4].append((x, y))
    out = sorted(buckets.values(), key=lambda v: -v[0])[:top]
    print(f"== {path.split(chr(92))[-1]} 彩色簇 ==")
    for n, sr, sg, sb, pts in out:
        cx = sum(p[0] for p in pts) // len(pts)
        cy = sum(p[1] for p in pts) // len(pts)
        print(
            f"  #{sr//n:02X}{sg//n:02X}{sb//n:02X}  n={n:5d}  中心~({cx},{cy})"
        )


def flat(path, name, x, y, r=3):
    img = Image.open(path).convert("RGB")
    c = Counter()
    for dy in range(-r, r + 1):
        for dx in range(-r, r + 1):
            c[img.getpixel((x + dx, y + dy))] += 1
    p = c.most_common(1)[0][0]
    print(f"{name:22s} #{p[0]:02X}{p[1]:02X}{p[2]:02X}")


base = r"D:\Code\dbhelper\.shots\proto_p01"
clusters(base + r"\card.png")
clusters(base + r"\tree.png", min_sat=25)
print()
# 扁平背景（校正后坐标：先前目测坐标 × 1.204）
flat(base + r"\card.png", "page_bg", 40, 40)
flat(base + r"\card.png", "card_inner_bg", 900, 350)
flat(base + r"\tree.png", "tree_bg", 300, 900)
flat(base + r"\right.png", "right_bg", 400, 750)
flat(base + r"\status.png", "statusbar_bg", 700, 60)
flat(base + r"\title.png", "titlebar_bg", 700, 45)

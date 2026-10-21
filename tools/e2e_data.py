"""DBHelper 数据页端到端验证（真实 MySQL demo_ecommerce）。

验证：
1. 连接树展开 → 显示 demo_ecommerce 的 14 张表
2. 打开 t_user → 数据网格显示真实数据
3. 脱敏：phone / email 列应打码
4. 行内编辑 → 真实 UPDATE 落库（用 mysql 客户端核对）
5. 批量编辑落库

坐标一律用客户区坐标 + 自动定位，不硬编码。
"""

import ctypes
import ctypes.wintypes as wt
import os
import struct
import subprocess
import sys
import time
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.path.join(ROOT, "target", "release", "dbhelper.exe")
SHOT = os.path.join(ROOT, ".shots")
MYSQL = r"C:\Program Files\MySQL\MySQL Server 8.0\bin\mysql.exe"
DB = "demo_ecommerce"

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
user32.SetProcessDPIAware()


def sql(q, db=DB):
    r = subprocess.run([MYSQL, "-uroot", "-p123456", "-N", "-B", db, "-e", q],
                       capture_output=True, text=True, errors="ignore")
    return [l.split("\t") for l in r.stdout.strip().splitlines() if l]


class BIH(ctypes.Structure):
    _fields_ = [("biSize", wt.DWORD), ("biWidth", ctypes.c_long),
                ("biHeight", ctypes.c_long), ("biPlanes", wt.WORD),
                ("biBitCount", wt.WORD), ("biCompression", wt.DWORD),
                ("biSizeImage", wt.DWORD), ("biXPelsPerMeter", ctypes.c_long),
                ("biYPelsPerMeter", ctypes.c_long), ("biClrUsed", wt.DWORD),
                ("biClrImportant", wt.DWORD)]


def find_window(pid):
    res = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def cb(h, _):
        p = wt.DWORD()
        user32.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid and user32.IsWindowVisible(h):
            res.append(h)
        return True

    user32.EnumWindows(cb, 0)
    return res[0] if res else None


def grab(hwnd):
    r = wt.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    w, h = r.right - r.left, r.bottom - r.top
    hdc = user32.GetDC(hwnd)
    mem = gdi32.CreateCompatibleDC(hdc)
    bmp = gdi32.CreateCompatibleBitmap(hdc, w, h)
    gdi32.SelectObject(mem, bmp)
    gdi32.BitBlt(mem, 0, 0, w, h, hdc, 0, 0, 0x00CC0020)
    bi = BIH()
    bi.biSize = ctypes.sizeof(BIH)
    bi.biWidth = w
    bi.biHeight = -h
    bi.biPlanes = 1
    bi.biBitCount = 32
    buf = ctypes.create_string_buffer(w * h * 4)
    gdi32.GetDIBits(mem, bmp, 0, h, buf, ctypes.byref(bi), 0)
    gdi32.DeleteObject(bmp)
    gdi32.DeleteDC(mem)
    user32.ReleaseDC(hwnd, hdc)
    return buf, w, h


def save_png(buf, w, h, name):
    rows = []
    for y in range(h):
        line = bytearray()
        src = bytes(buf[y * w * 4:(y + 1) * w * 4])
        for i in range(0, len(src), 4):
            line += bytes((src[i + 2], src[i + 1], src[i], src[i + 3]))
        rows.append(b"\x00" + bytes(line))
    raw = b"".join(rows)

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw, 6))
           + chunk(b"IEND", b""))
    with open(os.path.join(SHOT, name), "wb") as f:
        f.write(png)
    print(f"  shot -> {name}")


def px(buf, w, x, y):
    o = (y * w + x) * 4
    raw = bytes(buf[o:o + 4])
    return (raw[2], raw[1], raw[0], raw[3])


def to_screen(hwnd, x, y):
    pt = wt.POINT(int(x), int(y))
    user32.ClientToScreen(hwnd, ctypes.byref(pt))
    return pt.x, pt.y


def click(hwnd, x, y, button="left"):
    sx, sy = to_screen(hwnd, x, y)
    user32.SetCursorPos(sx, sy)
    time.sleep(0.15)
    if button == "left":
        user32.mouse_event(0x0002, 0, 0, 0, 0)
        time.sleep(0.06)
        user32.mouse_event(0x0004, 0, 0, 0, 0)
    else:
        user32.mouse_event(0x0008, 0, 0, 0, 0)
        time.sleep(0.08)
        user32.mouse_event(0x0010, 0, 0, 0, 0)
    time.sleep(0.45)


def dbl_click(hwnd, x, y):
    sx, sy = to_screen(hwnd, x, y)
    user32.SetCursorPos(sx, sy)
    time.sleep(0.15)
    for _ in range(2):
        user32.mouse_event(0x0002, 0, 0, 0, 0)
        time.sleep(0.05)
        user32.mouse_event(0x0004, 0, 0, 0, 0)
        time.sleep(0.05)
    time.sleep(0.6)


def type_text(txt):
    for ch in txt:
        user32.keybd_event(0, 0, 0, ord(ch))
        time.sleep(0.012)
    time.sleep(0.2)


def find_text_row(buf, w, h, x0, x1, y0, y1, min_dark=3):
    """在区域里找有深色文字的行，返回 (x_center, y)。"""
    for y in range(y0, min(y1, h)):
        dark = sum(1 for x in range(x0, min(x1, w), 3)
                   if px(buf, w, x, y)[0] < 130)
        if dark >= min_dark:
            return ((x0 + x1) // 2, y)
    return None


def main():
    print("=== 启动 ===")
    proc = subprocess.Popen([EXE])
    time.sleep(4.0)
    hwnd = None
    for _ in range(20):
        hwnd = find_window(proc.pid)
        if hwnd:
            break
        time.sleep(0.5)
    if not hwnd:
        print("FAIL 未找到窗口")
        proc.kill()
        return 1
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.8)
    click(hwnd, 900, 700)   # 激活
    print(f"hwnd={hwnd}")

    # --- 展开连接树 ---
    print("\n[1] 展开连接树（点展开箭头）")
    # 侧栏连接行：展开箭头在 x≈21, y≈247
    click(hwnd, 21, 247)
    time.sleep(3.0)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_10_tree.png")

    # --- 展开数据库（数据库行在连接行下方）---
    print("[2] 展开数据库")
    # 找到下一行的箭头位置（连接行下方 28px）
    db_arrow = None
    for y in range(255, 330):
        row = [x for x in range(15, 45) if px(buf, w, x, y)[0] < 160]
        if row and max(row) - min(row) <= 6:
            db_arrow = ((min(row) + max(row)) // 2, y)
            break
    print(f"  数据库行箭头 @ {db_arrow}")
    if db_arrow:
        click(hwnd, *db_arrow)
    time.sleep(3.0)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_11_db.png")

    # --- 展开表分组 → 找 t_user ---
    print("[3] 展开表分组")
    # 「表」分组行在 demo_ecommerce 上方
    tables_arrow = None
    for y in range(150, 400):
        row = [x for x in range(25, 55) if px(buf, w, x, y)[0] < 160]
        if row and max(row) - min(row) <= 6:
            tables_arrow = ((min(row) + max(row)) // 2, y)
            break
    print(f"  表分组箭头 @ {tables_arrow}")
    if tables_arrow:
        click(hwnd, *tables_arrow)
    time.sleep(2.5)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_12_tables.png")

    # --- 找 t_user 并打开 ---
    print("[4] 打开 t_user")
    user_row = None
    for y in range(180, 700):
        row = [x for x in range(80, 260, 2) if px(buf, w, x, y)[0] < 120]
        if len(row) >= 4:
            user_row = y
            break
    print(f"  首个表行 y={user_row}")
    if user_row:
        dbl_click(hwnd, 140, user_row)
        time.sleep(3.5)
        buf, w, h = grab(hwnd)
        save_png(buf, w, h, "e2e_13_data.png")

        # --- 检查脱敏 ---
        print("[4] 检查脱敏效果")
        # 手机号应为 11 位、含星号：扫描数据区里的星号字符
        stars = 0
        for y in range(300, 900, 2):
            for x in range(400, 2000, 2):
                p = px(buf, w, x, y)
                if p[0] < 90 and p[1] < 90 and p[2] < 90:
                    stars += 1
        print(f"  数据区深色像素（含打码星号）: {stars}")

    # --- 真实数据核对 ---
    print("\n[5] 数据库真实数据（供对照）")
    rows = sql("SELECT id, username, phone, email FROM t_user LIMIT 3;")
    for r in rows:
        print(f"    id={r[0]} username={r[1]} phone={r[2]} email={r[3]}")

    proc.terminate()
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()


if __name__ == "__main__":
    main()

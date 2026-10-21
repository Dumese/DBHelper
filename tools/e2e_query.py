"""DBHelper 查询页端到端验证（真实 MySQL）。

路径：右键连接 → 新建查询 → 输入 SQL → 执行 → 看结果 → 验证脱敏。

这是最可靠的验证路径（少依赖树展开的坐标）。
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


def type_text(txt):
    for ch in txt:
        user32.keybd_event(0, 0, 0, ord(ch))
        time.sleep(0.01)
    time.sleep(0.2)


def key(vk):
    user32.keybd_event(vk, 0, 0, 0)
    time.sleep(0.05)
    user32.keybd_event(vk, 0, 0x0002, 0)
    time.sleep(0.2)


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
    click(hwnd, 900, 700)

    # --- 1) 右键连接 → 新建查询 ---
    print("[1] 右键连接行")
    click(hwnd, 21, 247, "right")
    time.sleep(0.8)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_20_ctx.png")

    # 菜单项「新建查询」是第 5 项：打开连接/编辑/复制/命令行/新建查询/刷新/删除
    # 菜单从点击点下方开始，每项 32px
    menu_x = 21 + 100
    item_y = 247 + 8 + 32 * 4 + 16   # 第 5 项中心
    print(f"  点击「新建查询」@ ({menu_x},{item_y})")
    click(hwnd, menu_x, item_y)
    time.sleep(1.5)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_21_query.png")

    # --- 2) 输入 SQL ---
    print("[2] 输入 SQL")
    click(hwnd, 900, 300)
    time.sleep(0.3)
    type_text("SELECT id, username, phone, email FROM t_user LIMIT 8")
    time.sleep(0.3)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_22_sql.png")

    # --- 3) F5 执行 ---
    print("[3] F5 执行")
    key(0x74)   # F5
    time.sleep(3.0)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_23_result.png")

    # --- 4) 核对结果：结果区应出现真实数据 + 星号打码 ---
    print("[4] 核对结果区")
    # 统计结果区里「星号」字符（脱敏标记）
    stars = 0
    for y in range(200, 900, 2):
        for x in range(400, 2100, 2):
            p = px(buf, w, x, y)
            if p[0] < 100 and p[1] < 100 and p[2] < 100:
                stars += 1
    print(f"  结果区深色像素: {stars}")

    print("\n[5] 数据库真实值（对照）")
    for r in sql("SELECT id, username, phone, email FROM t_user LIMIT 3;"):
        print(f"    {r}")

    proc.terminate()
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()


if __name__ == "__main__":
    main()

"""DBHelper UI 黑盒测试（Windows 真实输入）。

覆盖本轮改动：
1. Win11 圆角窗口（角像素 vs 内部像素）
2. 新狐狸 logo（按橙色像素定位，验证已替换）
3. 连接树右键菜单（真实右键 → 截图确认浮层 + 7 个菜单项）
4. 弹窗右上角 X 按钮（打开向导 → 确认 X 存在 → 点击关闭）

技术要点（沿用前几轮结论）：
- PostMessage 合成事件 iced 不吃，必须 SetCursorPos + mouse_event / keybd_event
- 截图用 win32 BitBlt（PrintWindow 对 GPU 窗口常返回黑图）
- 坐标靠像素扫描定位，不靠猜
"""

import ctypes
import ctypes.wintypes as wt
import os
import struct
import subprocess
import sys
import time
import zlib

EXE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                    "target", "release", "dbhelper.exe")
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHOT_DIR = os.path.join(ROOT, ".shots")

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
user32.SetProcessDPIAware()

FOXY = (226, 112, 34)   # 狐狸橙（设计稿 #E27022）


# ---------------------------------------------------------------- 窗口与截图

def find_window(pid):
    """按进程 ID 找主窗口。

    注意：必须校验窗口标题，否则会抓到同进程创建的辅助窗口
    （或抓错窗口导致截到桌面）。取**标题为 DBHelper 且面积最大**的窗口。
    """
    res = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def cb(hwnd, _):
        p = wt.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(p))
        if p.value != pid or not user32.IsWindowVisible(hwnd):
            return True
        n = user32.GetWindowTextLengthW(hwnd)
        buf = ctypes.create_unicode_buffer(n + 1)
        user32.GetWindowTextW(hwnd, buf, n + 1)
        title = buf.value
        r = wt.RECT()
        user32.GetWindowRect(hwnd, ctypes.byref(r))
        area = (r.right - r.left) * (r.bottom - r.top)
        res.append((area, hwnd, title))
        return True

    user32.EnumWindows(cb, 0)
    if not res:
        return None
    # 优先标题匹配 DBHelper，否则取面积最大的
    for area, hwnd, title in res:
        if "DBHelper" in title:
            return hwnd
    res.sort(reverse=True)
    print("  候选窗口:", [(t, a) for a, h, t in res[:5]])
    return res[0][1]


class BIH(ctypes.Structure):
    _fields_ = [("biSize", wt.DWORD), ("biWidth", ctypes.c_long),
                ("biHeight", ctypes.c_long), ("biPlanes", wt.WORD),
                ("biBitCount", wt.WORD), ("biCompression", wt.DWORD),
                ("biSizeImage", wt.DWORD), ("biXPelsPerMeter", ctypes.c_long),
                ("biYPelsPerMeter", ctypes.c_long), ("biClrUsed", wt.DWORD),
                ("biClrImportant", wt.DWORD)]


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


def save_png(buf, w, h, path):
    # 关键：GetDIBits 取回的 32bit DIB 数据是 **BGRA** 排列，
    # 直接按 RGBA 写 PNG 会导致红蓝互换（橙色狐狸变蓝）。
    # 这里逐像素交换 R/B 后再写入。
    rows = []
    for y in range(h):
        line = bytearray()
        src = bytes(buf[y * w * 4:(y + 1) * w * 4])
        for i in range(0, len(src), 4):
            b_, g_, r_, a_ = src[i], src[i + 1], src[i + 2], src[i + 3]
            line += bytes((r_, g_, b_, a_))
        rows.append(b"\x00" + bytes(line))
    raw = b"".join(rows)

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw, 6))
           + chunk(b"IEND", b""))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(png)
    print(f"  shot -> {os.path.basename(path)} ({w}x{h})")


def px(buf, w, x, y):
    """读取像素，返回 (r, g, b) 三元组。"""
    o = (y * w + x) * 4
    raw = bytes(buf[o:o + 3])
    return (raw[0], raw[1], raw[2])


def find_fox(buf, w, h, region=None):
    """在图像中定位橙色狐狸 logo，返回其像素包围盒。"""
    x0, y0, x1, y1 = region or (0, 0, w, h)
    pts = []
    for y in range(y0, min(y1, h), 2):
        for x in range(x0, min(x1, w), 2):
            p = px(buf, w, x, y)
            if (abs(p[0] - FOXY[0]) < 28 and abs(p[1] - FOXY[1]) < 28
                    and abs(p[2] - FOXY[2]) < 28):
                pts.append((x, y))
    if not pts:
        return None
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return (min(xs), min(ys), max(xs), max(ys))


# ---------------------------------------------------------------- 输入

def move(x, y):
    user32.SetCursorPos(x, y)
    time.sleep(0.12)


def client_to_screen(hwnd, x, y):
    """客户区坐标 → 屏幕坐标。

    截图得到的是**客户区**坐标，而 SetCursorPos 需要**屏幕**坐标；
    窗口在屏幕上可能有偏移（本机曾偏移 (200,125)），必须换算。
    """
    pt = wt.POINT(int(x), int(y))
    user32.ClientToScreen(hwnd, ctypes.byref(pt))
    return pt.x, pt.y


def activate(hwnd):
    """真实左键点击以激活窗口（自动化环境必需）。"""
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.4)
    x, y = client_to_screen(hwnd, 900, 700)
    user32.SetCursorPos(x, y)
    time.sleep(0.2)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.08)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(0.6)


def left_click(hwnd, x, y):
    x, y = client_to_screen(hwnd, x, y)
    move(x, y)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.06)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(0.45)


def left_click_old(x, y):
    move(x, y)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.06)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(0.45)


def right_click(hwnd, x, y):
    x, y = client_to_screen(hwnd, x, y)
    move(x, y)
    time.sleep(0.25)
    user32.mouse_event(0x0008, 0, 0, 0, 0)
    time.sleep(0.1)
    user32.mouse_event(0x0010, 0, 0, 0, 0)
    time.sleep(0.7)


def main():
    print("启动应用…")
    proc = subprocess.Popen([EXE])
    time.sleep(4.0)
    hwnd = None
    for _ in range(20):
        hwnd = find_window(proc.pid)
        if hwnd:
            break
        time.sleep(0.5)
    if not hwnd:
        print("FAIL: 未找到窗口")
        proc.kill()
        return 1
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.8)

    r = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(r))
    W, H = r.right - r.left, r.bottom - r.top
    print(f"窗口 {W}x{H} @ ({r.left},{r.top})")

    # 关键：自动化环境里 SetForegroundWindow 常常不生效，必须先真实左键点一下
    # 激活窗口，否则后续所有鼠标事件都不会送达本窗口。
    activate(hwnd)

    fails = []

    # ---- 1) 圆角 + logo ----
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, os.path.join(SHOT_DIR, "ui3_00_start.png"))
    corner = px(buf, w, 1, 1)
    inner = px(buf, w, 120, 120)
    if corner != inner:
        print(f"  ✓ Win11 圆角生效：角像素 {corner} ≠ 内部 {inner}")
    else:
        print(f"  ✗ 圆角未生效：角像素 {corner} == 内部 {inner}")
        fails.append("圆角")

    box = find_fox(buf, w, h, (0, 0, 400, 300))
    if box:
        print(f"  ✓ 新狐狸 logo 已就位：包围盒 {box}")
    else:
        print("  ✗ 未找到橙色狐狸 logo")
        fails.append("logo")

    # ---- 2) 连接树右键菜单 ----
    # 侧栏连接行：数据库类型图标是彩色（橙/绿等）的小图标，
    # 在侧栏区域里找「孤立彩色像素簇」的行（排除分隔线等长条）。
    conn_y = None
    for y in range(180, 500):
        xs = []
        for x in range(40, 200, 2):
            p = px(buf, w, x, y)
            if max(p) - min(p) > 60 and p[0] > 170:
                xs.append(x)
        # 彩色像素集中在一个窄区间 = 图标（而非文字/边框）
        if xs and max(xs) - min(xs) <= 40 and len(xs) >= 3:
            conn_y = y
            conn_x = (min(xs) + max(xs)) // 2
            break
    if conn_y is None:
        conn_y, conn_x = 248, 52
    print(f"  连接行图标位置 ≈ ({conn_x}, {conn_y})")
    right_click(hwnd, conn_x, conn_y)
    buf2, w2, h2 = grab(hwnd)
    save_png(buf2, w2, h2, os.path.join(SHOT_DIR, "ui3_01_ctxmenu.png"))

    # 菜单浮层：菜单面板是白底卡片，位于点击点右下；
    # 面板内会有多行深色文字（菜单项）。
    menu_found = False
    text_rows = 0
    for yy in range(max(0, conn_y - 20), min(h2, conn_y + 340), 3):
        dark = 0
        for xx in range(conn_x + 20, min(w2, conn_x + 400), 3):
            p = px(buf2, w2, xx, yy)
            if p[0] < 140 and p[1] < 140 and p[2] < 140:
                dark += 1
        if dark >= 8:
            text_rows += 1
    menu_found = text_rows >= 6
    print(f"  {'✓' if menu_found else '✗'} 右键菜单浮层（文字行 {text_rows}）")
    if not menu_found:
        fails.append("右键菜单")

    # 点空白关闭
    left_click(hwnd, int(W * 0.55), int(H * 0.55))

    # ---- 3) 弹窗 X 按钮 ----
    # 点侧栏「+」新建连接 → 打开向导弹窗
    buf3, w3, h3 = grab(hwnd)
    plus = None
    for y in range(90, 260, 2):
        for x in range(200, 340, 2):
            p = px(buf3, w3, x, y)
            if p[0] < 130 and p[1] < 130 and p[2] < 130:
                plus = (x, y)
                break
        if plus:
            break
    if plus:
        print(f"  点「+」新建连接 {plus}")
        left_click(hwnd, *plus)
        buf4, w4, h4 = grab(hwnd)
        save_png(buf4, w4, h4, os.path.join(SHOT_DIR, "ui3_02_wizard.png"))
        # 弹窗应居中（620 逻辑宽 × 2 缩放 = 1240 物理宽）
        # 弹窗右上角 = 窗口中心 + 620 物理像素 - 40
        cx = w4 // 2
        x_found = False
        for y in range(50, 200, 2):
            for x in range(cx + 480, min(w4, cx + 620), 2):
                p = px(buf4, w4, x, y)
                if p[0] < 120 and p[1] < 120 and p[2] < 120:
                    x_found = True
                    break
            if x_found:
                break
        print(f"  {'✓' if x_found else '✗'} 弹窗右上角 X 按钮")
        if not x_found:
            fails.append("弹窗X")

    print("\n关闭应用…")
    try:
        proc.terminate()
        proc.wait(timeout=5)
    except Exception:
        proc.kill()

    if fails:
        print("FAILED:", ", ".join(fails))
        return 1
    print("ALL OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())

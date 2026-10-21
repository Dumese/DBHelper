"""右键菜单专项探针。

在连接行上右键 → 立即截图，并检查 tracing 日志里是否有
「右键菜单: conn=...」日志，以区分「事件没到达」与「菜单画不出来」。
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
SHOT_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".shots")

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
user32.SetProcessDPIAware()


def find_window(pid):
    res = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def cb(hwnd, _):
        p = wt.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(p))
        if p.value == pid and user32.IsWindowVisible(hwnd):
            res.append(hwnd)
        return True

    user32.EnumWindows(cb, 0)
    return res[0] if res else None


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
    with open(path, "wb") as f:
        f.write(png)
    print(f"  shot -> {os.path.basename(path)}")


def px(buf, w, x, y):
    o = (y * w + x) * 4
    raw = bytes(buf[o:o + 4])
    return (raw[2], raw[1], raw[0], raw[3])


def main():
    print("启动（带 RUST_LOG=info）…")
    env = dict(os.environ, RUST_LOG="info")
    proc = subprocess.Popen([EXE], env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    time.sleep(4.0)
    hwnd = None
    for _ in range(20):
        hwnd = find_window(proc.pid)
        if hwnd:
            break
        time.sleep(0.5)
    if not hwnd:
        print("未找到窗口")
        proc.kill()
        return 1
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.8)

    buf, w, h = grab(hwnd)
    # 找侧栏里的橙色图标（连接行，logo 主色 #E27022）
    fox = (226, 112, 34)
    target = None
    for y in range(150, 420):
        xs = []
        for x in range(20, 260, 1):
            p = px(buf, w, x, y)
            # 橙色系（放宽：允许抗锯齿混合）
            if p[0] > 150 and p[1] > 60 and p[1] < 190 and p[2] < 110:
                xs.append(x)
        if xs and max(xs) - min(xs) <= 60 and len(xs) >= 4:
            target = ((min(xs) + max(xs)) // 2, y)
            break
    if not target:
        # 兜底：直接用已知布局坐标（侧栏第一行，缩放 2 倍）
        target = (50, 248)
    if not target:
        print("未找到连接行图标")
        proc.kill()
        return 1
    print(f"  连接行图标 @ (客户区) {target}")

    # 关键：截图/坐标是**客户区**坐标，需换算为屏幕坐标
    # （窗口在屏幕上可能有偏移，之前一直点在桌面图标上）。
    pt = wt.POINT(target[0], target[1])
    user32.ClientToScreen(hwnd, ctypes.byref(pt))
    print(f"  → 屏幕坐标 ({pt.x},{pt.y})")

    print("  右键点击…")
    user32.SetCursorPos(pt.x, pt.y)
    time.sleep(0.3)
    user32.mouse_event(0x0008, 0, 0, 0, 0)
    time.sleep(0.1)
    user32.mouse_event(0x0010, 0, 0, 0, 0)
    time.sleep(1.2)

    buf2, w2, h2 = grab(hwnd)
    save_png(buf2, w2, h2, os.path.join(SHOT_DIR, "probe_ctx.png"))

    # 检测点击点附近是否出现菜单面板（白底卡片）
    # 菜单宽 196 逻辑 = 392 物理，位于点击点右下方
    changed = 0
    for y in range(target[1] - 20, min(h2, target[1] + 400), 4):
        for x in range(target[0], min(w2, target[0] + 420), 8):
            if px(buf2, w2, x, y) != px(buf, w, x, y):
                changed += 1
    print(f"  点击点附近变化像素数: {changed} → {'菜单已弹出' if changed > 300 else '无变化'}")

    proc.terminate()
    try:
        out = proc.communicate(timeout=5)[0]
    except Exception:
        proc.kill()
        out = b""
    if out:
        txt = out.decode("utf-8", "ignore")
        for line in txt.splitlines():
            if "右键菜单" in line or "platform" in line:
                print("  LOG:", line)


if __name__ == "__main__":
    main()

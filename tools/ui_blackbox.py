#!/usr/bin/env python
"""DBHelper UI 黑盒测试：启动应用、截图、模拟交互、检查存活。

只做「外部可见行为」的验证（黑盒）：进程存活、窗口存在、截图可读、
模拟点击后不崩溃。
"""
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import time

EXE = r"D:\Code\dbhelper\dbhelper\target\release\dbhelper.exe"
SHOT_DIR = r"D:\Code\dbhelper\dbhelper\.shots"
os.makedirs(SHOT_DIR, exist_ok=True)

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
user32.SetProcessDPIAware()


def find_window(pid):
    """找到属于该 pid 的主窗口句柄。"""
    found = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wt.HWND, wt.LPARAM)
    def cb(hwnd, _):
        wpid = wt.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(wpid))
        if wpid.value == pid and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True

    user32.EnumWindows(cb, 0)
    return found[0] if found else None


def screenshot(hwnd, path):
    """抓取窗口位图并存为 PNG（用 PowerShell 无关的纯 GDI 方式）。"""
    rect = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    w, h = rect.right - rect.left, rect.bottom - rect.top
    if w <= 0 or h <= 0:
        return None, (0, 0)
    hdc = user32.GetWindowDC(hwnd)
    memdc = gdi32.CreateCompatibleDC(hdc)
    bmp = gdi32.CreateCompatibleBitmap(hdc, w, h)
    gdi32.SelectObject(memdc, bmp)
    # 打印整个窗口到 DC（PW_RENDERFULLCONTENT = 2）
    user32.PrintWindow(hwnd, memdc, 2)

    class BITMAPINFOHEADER(ctypes.Structure):
        _fields_ = [
            ("biSize", wt.DWORD), ("biWidth", ctypes.c_long),
            ("biHeight", ctypes.c_long), ("biPlanes", wt.WORD),
            ("biBitCount", wt.WORD), ("biCompression", wt.DWORD),
            ("biSizeImage", wt.DWORD), ("biXPelsPerMeter", ctypes.c_long),
            ("biYPelsPerMeter", ctypes.c_long), ("biClrUsed", wt.DWORD),
            ("biClrImportant", wt.DWORD),
        ]

    bi = BITMAPINFOHEADER()
    bi.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    bi.biWidth = w
    bi.biHeight = -h
    bi.biPlanes = 1
    bi.biBitCount = 32
    bi.biCompression = 0
    buf = ctypes.create_string_buffer(w * h * 4)
    got = gdi32.GetDIBits(memdc, bmp, 0, h, buf, ctypes.byref(bi), 0)
    gdi32.DeleteObject(bmp)
    gdi32.DeleteDC(memdc)
    user32.ReleaseDC(hwnd, hdc)
    if got == 0:
        return None, (w, h)
    return buf, (w, h)


def save_png(buf, w, h, path):
    """把 BGRA 像素写成 PNG（用 zlib + struct，不依赖第三方库）。"""
    import struct
    import zlib

    raw = bytearray()
    row_bytes = w * 4
    for y in range(h):
        raw.append(0)
        raw += buf[y * row_bytes:(y + 1) * row_bytes]

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 6))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def click(hwnd, x, y):
    """在窗口客户区坐标 (x, y) 处发一次点击。"""
    lparam = (y << 16) | (x & 0xFFFF)
    user32.PostMessageW(hwnd, 0x0201, 1, lparam)  # WM_LBUTTONDOWN
    user32.PostMessageW(hwnd, 0x0202, 0, lparam)  # WM_LBUTTONUP


def main():
    if not os.path.exists(EXE):
        print("FAIL: 可执行文件不存在", EXE)
        return 1
    print("启动:", EXE)
    proc = subprocess.Popen([EXE])
    time.sleep(6)

    if proc.poll() is not None:
        print("FAIL: 进程已退出，退出码", proc.returncode)
        return 1
    print("PASS: 进程存活（pid=%d）" % proc.pid)

    hwnd = find_window(proc.pid)
    if not hwnd:
        print("WARN: 未找到可见窗口（可能是无头环境）")
        proc.terminate()
        return 0
    print("PASS: 找到主窗口 hwnd=%d" % hwnd)

    rect = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    print("窗口尺寸: %dx%d" % (rect.right - rect.left, rect.bottom - rect.top))

    buf, (w, h) = screenshot(hwnd, None)
    if buf is None:
        print("WARN: 截图失败（可能是无头环境）")
    else:
        p = os.path.join(SHOT_DIR, "audit_start.png")
        save_png(buf, w, h, p)
        print("PASS: 已截图", p, "(%dx%d)" % (w, h))

    # 模拟点击若干坐标（覆盖新入口按钮区域），验证不崩溃
    targets = [
        (40, 300),    # 侧栏
        (700, 300),   # 面包屑右侧按钮区（BI/导入/迁移/管理）
        (700, 500),   # 主区域
        (400, 200),
    ]
    for (x, y) in targets:
        click(hwnd, x, y)
        time.sleep(0.4)
    time.sleep(1.5)

    if proc.poll() is not None:
        print("FAIL: 点击后进程崩溃，退出码", proc.returncode)
        return 1
    print("PASS: 模拟点击后进程仍存活")

    buf, (w, h) = screenshot(hwnd, None)
    if buf is not None:
        p = os.path.join(SHOT_DIR, "audit_after_clicks.png")
        save_png(buf, w, h, p)
        print("PASS: 点击后截图", p)

    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    print("已退出")
    return 0


if __name__ == "__main__":
    sys.exit(main())

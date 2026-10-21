"""启动 DBHelper、用 PrintWindow 截图（可截被遮挡窗口），然后退出。

规避工具会话退出即杀死子进程的问题：启动与截图在同一进程内完成。
输出为 PNG，便于直接查看。
"""

import ctypes
import os
import struct
import subprocess
import sys
import time
import zlib
from ctypes import wintypes

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
user32.SetProcessDPIAware()

PW_RENDERFULLCONTENT = 0x00000002
SRCCOPY = 0x00CC0020
BI_RGB = 0


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


class BITMAPINFO(ctypes.Structure):
    _fields_ = [("bmiHeader", BITMAPINFOHEADER), ("bmiColors", wintypes.DWORD * 3)]


def find_window(pid):
    result = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def enum_proc(hwnd, _lparam):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            rect = wintypes.RECT()
            user32.GetWindowRect(hwnd, ctypes.byref(rect))
            w = rect.right - rect.left
            h = rect.bottom - rect.top
            if w > 300 and h > 200:
                result.append((hwnd, rect))
        return True

    user32.EnumWindows(enum_proc, 0)
    return result[0] if result else None


def capture(hwnd, rect, path):
    w = rect.right - rect.left
    h = rect.bottom - rect.top
    hdc_screen = user32.GetDC(0)
    hdc = gdi32.CreateCompatibleDC(hdc_screen)

    bmi = BITMAPINFO()
    bmi.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    bmi.bmiHeader.biWidth = w
    bmi.bmiHeader.biHeight = -h  # top-down
    bmi.bmiHeader.biPlanes = 1
    bmi.bmiHeader.biBitCount = 32
    bmi.bmiHeader.biCompression = BI_RGB

    bits = ctypes.c_void_p()
    hbmp = gdi32.CreateDIBSection(hdc, ctypes.byref(bmi), 0, ctypes.byref(bits), None, 0)
    gdi32.SelectObject(hdc, hbmp)

    ok = user32.PrintWindow(hwnd, hdc, PW_RENDERFULLCONTENT)
    if not ok:
        gdi32.BitBlt(hdc, 0, 0, w, h, hdc_screen, rect.left, rect.top, SRCCOPY)

    buf = ctypes.string_at(bits, w * h * 4)
    gdi32.DeleteObject(hbmp)
    gdi32.DeleteDC(hdc)
    user32.ReleaseDC(0, hdc_screen)

    rows = []
    for y in range(h):
        row = bytearray()
        row.append(0)
        off = y * w * 4
        for x in range(w):
            b = buf[off + x * 4]
            g = buf[off + x * 4 + 1]
            r = buf[off + x * 4 + 2]
            row += bytes((r, g, b))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 6))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)
    return ok, w, h


def main():
    exe = r"D:\Code\dbhelper\target\debug\dbhelper.exe"
    out = sys.argv[1] if len(sys.argv) > 1 else r"D:\Code\dbhelper\.shots\shot.png"
    wait = float(sys.argv[2]) if len(sys.argv) > 2 else 5.0

    proc = subprocess.Popen(
        [exe],
        cwd=r"D:\Code\dbhelper",
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(wait)
    found = None
    for _ in range(30):
        found = find_window(proc.pid)
        if found:
            break
        time.sleep(0.3)
    if not found:
        print("NO_WINDOW")
        proc.terminate()
        return 1
    hwnd, rect = found
    ok, w, h = capture(hwnd, rect, out)
    print(f"OK printwindow={bool(ok)} {w}x{h} -> {out}")
    proc.terminate()
    return 0


if __name__ == "__main__":
    sys.exit(main())

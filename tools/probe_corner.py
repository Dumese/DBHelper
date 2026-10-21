"""DWM 圆角快速探针。

在运行中的 DBHelper 窗口上直接用 ctypes 试不同的 Win32 组合，
每次组合后截图并检查左上角像素是否变成桌面色（=被裁圆）。
比改 Rust 再编译（1.5 分钟）快得多。

用法： python tools/probe_corner.py
"""

import ctypes
import ctypes.wintypes as wt
import os
import struct
import subprocess
import time
import zlib

EXE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                    "target", "release", "dbhelper.exe")
SHOT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".shots")

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
dwmapi = ctypes.windll.dwmapi
user32.SetProcessDPIAware()

GWL_STYLE = -16
WS_POPUP = 0x80000000
WS_CAPTION = 0x00C00000
WS_THICKFRAME = 0x00040000
WS_SYSMENU = 0x00080000
WS_MINIMIZEBOX = 0x00020000
WS_MAXIMIZEBOX = 0x00010000
WS_CLIPCHILDREN = 0x02000000
WS_CLIPSIBLINGS = 0x04000000

DWMWA_WINDOW_CORNER_PREFERENCE = 33
DWMWA_BORDER_COLOR = 34
DWMWA_SYSTEMBACKDROP_TYPE = 38
DWMWCP_ROUND = 2
DWMWCP_ROUNDFRAME = 3
DWMWCP_SMALL = 1

SWP_NOSIZE = 0x0001
SWP_NOMOVE = 0x0002
SWP_NOZORDER = 0x0004
SWP_NOACTIVATE = 0x0010
SWP_FRAMECHANGED = 0x0020


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


def grab(hwnd):
    r = wt.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    w, h = r.right - r.left, r.bottom - r.top
    hdc = user32.GetDC(hwnd)
    mem = gdi32.CreateCompatibleDC(hdc)
    bmp = gdi32.CreateCompatibleBitmap(hdc, w, h)
    gdi32.SelectObject(mem, bmp)
    gdi32.BitBlt(mem, 0, 0, w, h, hdc, 0, 0, 0x00CC0020)

    class BIH(ctypes.Structure):
        _fields_ = [("biSize", wt.DWORD), ("biWidth", ctypes.c_long),
                    ("biHeight", ctypes.c_long), ("biPlanes", wt.WORD),
                    ("biBitCount", wt.WORD), ("biCompression", wt.DWORD),
                    ("biSizeImage", wt.DWORD), ("biXPelsPerMeter", ctypes.c_long),
                    ("biYPelsPerMeter", ctypes.c_long), ("biClrUsed", wt.DWORD),
                    ("biClrImportant", wt.DWORD)]

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
    raw = b"".join(b"\x00" + bytes(buf[y * w * 4:(y + 1) * w * 4]) for y in range(h))

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw, 6))
           + chunk(b"IEND", b""))
    with open(path, "wb") as f:
        f.write(png)


def corner_state(buf, w):
    """检查左上角 6x6 区域是否出现「与内容不同」的颜色（被裁圆的迹象）。"""
    def px(x, y):
        o = (y * w + x) * 4
        return (buf[o], buf[o + 1], buf[o + 2])

    c00 = px(0, 0)
    c11 = px(1, 1)
    inner = px(60, 60)
    return c00, c11, inner


def apply(hwnd, style, corner, label):
    if style is not None:
        user32.SetWindowLongPtrW(hwnd, GWL_STYLE, style)
        user32.SetWindowPos(hwnd, None, 0, 0, 0, 0,
                            SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED)
    if corner is not None:
        v = ctypes.c_int(corner)
        hr = dwmapi.DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE,
                                          ctypes.byref(v), 4)
    else:
        hr = -1
    time.sleep(0.8)
    buf, w, h = grab(hwnd)
    c00, c11, inner = corner_state(buf, w)
    rounded = "圆角!" if c00 != inner else "直角"
    print(f"  [{label}] hr=0x{hr & 0xFFFFFFFF:X} corner0={c00} inner={inner} → {rounded}")
    save_png(buf, w, h, os.path.join(SHOT, f"probe_{label}.png"))
    return c00 != inner


def main():
    proc = subprocess.Popen([EXE])
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
    time.sleep(0.5)

    cur = user32.GetWindowLongPtrW(hwnd, GWL_STYLE)
    print(f"当前 style = 0x{cur & 0xFFFFFFFF:08X}")

    print("组合测试：")
    # A: 仅设圆角偏好，不改样式
    apply(hwnd, None, DWMWCP_ROUND, "A_round_only")
    # B: WS_CAPTION|WS_THICKFRAME + 圆角
    apply(hwnd, (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX
                 | WS_MAXIMIZEBOX | WS_CLIPCHILDREN | WS_CLIPSIBLINGS),
          DWMWCP_ROUND, "B_caption_thick")
    # C: 完整标准窗口样式（OVERLAPPEDWINDOW 等价）+ 圆角
    apply(hwnd, (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX
                 | WS_MAXIMIZEBOX | WS_CLIPCHILDREN | WS_CLIPSIBLINGS),
          DWMWCP_ROUNDFRAME, "C_roundframe")
    # D: 恢复 POPUP + 不圆角（Win10 风格对照）
    apply(hwnd, (WS_POPUP | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX
                 | WS_MAXIMIZEBOX | WS_CLIPCHILDREN | WS_CLIPSIBLINGS),
          DWMWCP_DONOTROUND if False else 0, "D_popup_noround")

    proc.terminate()
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()


if __name__ == "__main__":
    main()

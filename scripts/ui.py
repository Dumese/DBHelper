"""开发期 UI 验证助手：常驻启动应用、按逻辑坐标点击、PrintWindow 截图。

用法：
  python scripts/ui.py start
  python scripts/ui.py click 120 200
  python scripts/ui.py key "{F5}"
  python scripts/ui.py shot .shots/er.png
  python scripts/ui.py quit
"""

import ctypes
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
EXE = r"D:\Code\dbhelper\target\release\dbhelper.exe"
CWD = r"D:\Code\dbhelper"


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


def windows():
    result = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def enum_proc(hwnd, _):
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        name = ctypes.create_unicode_buffer(512)
        user32.GetWindowTextW(hwnd, name, 512)
        if "DBHelper" in name.value and user32.IsWindowVisible(hwnd):
            rect = wintypes.RECT()
            user32.GetWindowRect(hwnd, ctypes.byref(rect))
            if rect.right - rect.left > 300:
                result.append((hwnd, pid.value, rect))
        return True

    user32.EnumWindows(enum_proc, 0)
    return result


def target():
    w = windows()
    if not w:
        return None
    return w[0]


def start():
    if target():
        print("ALREADY_RUNNING")
        return
    subprocess.Popen(
        [EXE], cwd=CWD, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP,
    )
    for _ in range(40):
        time.sleep(0.4)
        if target():
            print("STARTED")
            return
    print("NO_WINDOW")


def scale(hwnd, rect):
    dpi = user32.GetDpiForWindow(hwnd)
    return dpi / 96.0, rect


def click(x, y):
    t = target()
    if not t:
        print("NO_WINDOW")
        return
    hwnd, _, rect = t
    s, _ = scale(hwnd, rect)
    px = int(rect.left + x * s)
    py = int(rect.top + y * s)
    user32.SetForegroundWindow(hwnd)
    user32.SetCursorPos(px, py)
    time.sleep(0.08)
    user32.mouse_event(0x0002, 0, 0, 0, 0)  # left down
    time.sleep(0.05)
    user32.mouse_event(0x0004, 0, 0, 0, 0)  # left up
    print(f"CLICK {x},{y} -> {px},{py} scale={s:.2f}")


def move(x, y):
    t = target()
    if not t:
        print("NO_WINDOW")
        return
    hwnd, _, rect = t
    s, _ = scale(hwnd, rect)
    px = int(rect.left + x * s)
    py = int(rect.top + y * s)
    user32.SetForegroundWindow(hwnd)
    user32.SetCursorPos(px, py)
    print(f"MOVE {x},{y} -> {px},{py}")


def drag(x1, y1, x2, y2):
    t = target()
    if not t:
        print("NO_WINDOW")
        return
    hwnd, _, rect = t
    s, _ = scale(hwnd, rect)
    p1 = (int(rect.left + x1 * s), int(rect.top + y1 * s))
    p2 = (int(rect.left + x2 * s), int(rect.top + y2 * s))
    user32.SetForegroundWindow(hwnd)
    user32.SetCursorPos(*p1)
    time.sleep(0.2)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    steps = 12
    for i in range(1, steps + 1):
        x = p1[0] + (p2[0] - p1[0]) * i // steps
        y = p1[1] + (p2[1] - p1[1]) * i // steps
        user32.SetCursorPos(x, y)
        time.sleep(0.03)
    user32.mouse_event(0x0004, 0, 0, 0, 0)
    print(f"DRAG {x1},{y1} -> {x2},{y2}")


def key(seq):
    t = target()
    if not t:
        print("NO_WINDOW")
        return
    hwnd, _, _ = t
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.1)
    import subprocess as sp

    ps = (
        "$ws = New-Object -ComObject WScript.Shell; "
        f"$ws.SendKeys('{seq}')"
    )
    sp.run(["powershell", "-NoProfile", "-Command", ps], check=False)
    print("KEY", seq)


def shot(path):
    t = target()
    if not t:
        print("NO_WINDOW")
        return
    hwnd, _, rect = t
    w = rect.right - rect.left
    h = rect.bottom - rect.top
    hdc_screen = user32.GetDC(0)
    hdc = gdi32.CreateCompatibleDC(hdc_screen)

    bmi = BITMAPINFO()
    bmi.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    bmi.bmiHeader.biWidth = w
    bmi.bmiHeader.biHeight = -h
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
        row = bytearray([0])
        off = y * w * 4
        for x in range(w):
            row += bytes((buf[off + x * 4 + 2], buf[off + x * 4 + 1], buf[off + x * 4]))
        rows.append(bytes(row))

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(b"".join(rows), 6))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)
    print(f"SHOT {w}x{h} -> {path}")


def quit_():
    t = target()
    if not t:
        print("NO_WINDOW")
        return
    hwnd, pid, _ = t
    user32.PostMessageW(hwnd, 0x0010, 0, 0)  # WM_CLOSE
    time.sleep(0.6)
    subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True)
    print("QUIT")


def run_script(path):
    """一次性完成「启动 → 一系列操作 → 退出」，避免子进程随会话结束被回收。"""
    proc = subprocess.Popen(
        [EXE], cwd=CWD, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(40):
            time.sleep(0.4)
            if target():
                break
        else:
            print("NO_WINDOW")
            return 1
        time.sleep(float(open(path, encoding="utf-8").read().split("wait", 1)[1].split("\n")[0].strip() or 5) if "wait" in open(path, encoding="utf-8").readline() else 4)
        for line in open(path, encoding="utf-8"):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            cmd = parts[0]
            if cmd == "drag":
                drag(int(parts[1]), int(parts[2]), int(parts[3]), int(parts[4]))
            elif cmd == "move":
                move(int(parts[1]), int(parts[2]))
            elif cmd == "click":
                click(int(parts[1]), int(parts[2]))
            elif cmd == "key":
                key(" ".join(parts[1:]))
            elif cmd == "shot":
                shot(parts[1])
            elif cmd == "wait":
                time.sleep(float(parts[1]))
            else:
                print("skip", line)
            time.sleep(0.35)
    finally:
        try:
            proc.terminate()
        except Exception:
            pass
    return 0


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "shot"
    if cmd == "start":
        start()
    elif cmd == "click":
        click(int(sys.argv[2]), int(sys.argv[3]))
    elif cmd == "key":
        key(sys.argv[2])
    elif cmd == "shot":
        shot(sys.argv[2] if len(sys.argv) > 2 else r"D:\Code\dbhelper\.shots\ui.png")
    elif cmd == "quit":
        quit_()
    elif cmd == "run":
        sys.exit(run_script(sys.argv[2]))
    else:
        print("unknown", cmd)

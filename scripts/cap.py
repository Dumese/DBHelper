"""启动 DBHelper 并截图（按进程名定位窗口，不依赖窗口标题）。

用法：
  python scripts/cap.py run  <out.png> [wait_seconds]
  python scripts/cap.py shot <out.png>
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
kernel32 = ctypes.windll.kernel32
user32.SetProcessDPIAware()

PW_RENDERFULLCONTENT = 0x00000002
SRCCOPY = 0x00CC0020
BI_RGB = 0
EXE = r"D:\Code\dbhelper\target\release\dbhelper.exe"
CWD = r"D:\Code\dbhelper"

CREATE_DETACHED = 0x00000008
CREATE_NEW_GROUP = 0x00000200


class PROCESSENTRY32(ctypes.Structure):
    _fields_ = [
        ("dwSize", wintypes.DWORD),
        ("cntUsage", wintypes.DWORD),
        ("th32ProcessID", wintypes.DWORD),
        ("th32DefaultHeapID", ctypes.POINTER(wintypes.ULONG)),
        ("th32ModuleID", wintypes.DWORD),
        ("cntThreads", wintypes.DWORD),
        ("th32ParentProcessID", wintypes.DWORD),
        ("pcPriClassBase", wintypes.LONG),
        ("dwFlags", wintypes.DWORD),
        ("szExeFile", wintypes.CHAR * 260),
    ]


def find_pid(name):
    hSnap = kernel32.CreateToolhelp32Snapshot(0x00000002, 0)
    pe = PROCESSENTRY32()
    pe.dwSize = ctypes.sizeof(PROCESSENTRY32)
    pid = None
    if kernel32.Process32First(hSnap, ctypes.byref(pe)):
        while True:
            if pe.szExeFile.decode("utf-8", "ignore").lower() == name.lower():
                pid = pe.th32ProcessID
                break
            if not kernel32.Process32Next(hSnap, ctypes.byref(pe)):
                break
    kernel32.CloseHandle(hSnap)
    return pid


def target():
    pid = find_pid("dbhelper.exe")
    if not pid:
        return None
    res = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def proc(h, _):
        p = wintypes.DWORD()
        user32.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid and user32.IsWindowVisible(h):
            r = wintypes.RECT()
            user32.GetWindowRect(h, ctypes.byref(r))
            w = r.right - r.left
            hh = r.bottom - r.top
            if w > 400 and hh > 300:
                res.append((h, w, hh))
        return True

    user32.EnumWindows(proc, 0)
    if not res:
        return None
    res.sort(key=lambda x: x[1] * x[2], reverse=True)
    return res[0][0]


def shot(path):
    hwnd = target()
    if not hwnd:
        print("NO_WINDOW")
        return False
    rect = wintypes.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    w = rect.right - rect.left
    h = rect.bottom - rect.top

    def grab(use_print):
        hdc_src = user32.GetWindowDC(hwnd) if not use_print else user32.GetDC(0)
        hdc = gdi32.CreateCompatibleDC(hdc_src)
        bmi = struct.pack("iiiiiiiiiii", ctypes.sizeof(ctypes.c_ulong) * 40, w, -h, 1, 32, BI_RGB, w * h * 4, 0, 0, 0, 0)
        bits = ctypes.c_void_p()
        hbmp = gdi32.CreateDIBSection(hdc, bmi, 0, ctypes.byref(bits), None, 0)
        gdi32.SelectObject(hdc, hbmp)
        if use_print:
            ok = user32.PrintWindow(hwnd, hdc, PW_RENDERFULLCONTENT)
        else:
            ok = gdi32.BitBlt(hdc, 0, 0, w, h, hdc_src, 0, 0, SRCCOPY)
        data = ctypes.string_at(bits, w * h * 4) if ok else b""
        gdi32.DeleteObject(hbmp)
        gdi32.DeleteDC(hdc)
        if use_print:
            user32.ReleaseDC(0, hdc_src)
        else:
            user32.ReleaseDC(hwnd, hdc_src)
        return data

    user32.SetForegroundWindow(hwnd)
    time.sleep(0.4)
    buf = grab(True)
    if not buf or set(buf) == {0}:
        time.sleep(0.6)
        buf = grab(False)
    if not buf:
        print("GRAB_FAILED")
        return False

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
    print(f"SHOT {w}x{h} -> {path} (hwnd={hwnd})")
    return True


def launch_and_shot(path, wait):
    subprocess.Popen(
        [EXE], cwd=CWD, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        creationflags=CREATE_DETACHED | CREATE_NEW_GROUP,
    )
    for _ in range(50):
        time.sleep(0.4)
        if target():
            break
    else:
        print("NO_WINDOW_STARTED")
        return
    time.sleep(wait)
    shot(path)


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "shot"
    out = sys.argv[2] if len(sys.argv) > 2 else r"D:\Code\dbhelper\.shots\cap.png"
    if mode == "run":
        wait = float(sys.argv[3]) if len(sys.argv) > 3 else 5.0
        launch_and_shot(out, wait)
    else:
        shot(out)

"""交互压测：空闲 / 鼠标扫动 / 点击 三阶段驱动，配合 perf 日志定位卡顿来源。"""
import ctypes
import math
import subprocess
import time
from ctypes import wintypes

u = ctypes.windll.user32
k = ctypes.windll.kernel32
u.SetProcessDPIAware()

class FILETIME(ctypes.Structure):
    _fields_ = [("dwLow", wintypes.DWORD), ("dwHigh", wintypes.DWORD)]

def cpu_ms(h):
    c, e, k1, uu = FILETIME(), FILETIME(), FILETIME(), FILETIME()
    k.GetProcessTimes(h, ctypes.byref(c), ctypes.byref(e), ctypes.byref(k1), ctypes.byref(uu))
    return (((k1.dwHigh << 32) | k1.dwLow) + ((uu.dwHigh << 32) | uu.dwLow)) / 10000.0

proc = subprocess.Popen(
    [r"D:\Code\dbhelper\target\release\dbhelper.exe"],
    cwd=r"D:\Code\dbhelper", stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    creationflags=8 | 0x200,
)
time.sleep(4)
h = k.OpenProcess(0x1000, False, proc.pid)

found = []

@ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
def en(hwnd, _):
    r = wintypes.RECT()
    u.GetWindowRect(hwnd, ctypes.byref(r))
    if r.right - r.left > 1500 and r.bottom - r.top > 800:
        found.append(r)
    return True

u.EnumWindows(en, 0)
r = found[0]
print(f"PHASE idle start  window=({r.left},{r.top},{r.right},{r.bottom})", flush=True)

base = cpu_ms(h)
time.sleep(6)
print(f"PHASE idle done   cpu={cpu_ms(h)-base:.0f}ms/6s", flush=True)

base = cpu_ms(h)
t_end = time.time() + 6
i = 0
while time.time() < t_end:
    x = r.left + 200 + int(700 * math.sin(i / 9))
    y = r.top + 250 + int(400 * math.cos(i / 12))
    u.SetCursorPos(x, y)
    i += 1
    time.sleep(0.012)
print(f"PHASE move done   cpu={cpu_ms(h)-base:.0f}ms/6s", flush=True)

base = cpu_ms(h)
from datetime import datetime, timezone
for j in range(24):
    x = r.left + 150 + (j % 6) * 320
    y = r.top + 120 + (j // 6) * 260
    u.SetCursorPos(x, y)
    time.sleep(0.05)
    t0 = datetime.now(timezone.utc).strftime("%H:%M:%S.%f")[:-3]
    u.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.03)
    u.mouse_event(0x0004, 0, 0, 0, 0)
    print(f"CLICK {j} sent@{t0}", flush=True)
    time.sleep(0.28)
print(f"PHASE click done  cpu={cpu_ms(h)-base:.0f}ms/10s", flush=True)

time.sleep(2)
k.CloseHandle(h)
proc.terminate()
print("DONE", flush=True)

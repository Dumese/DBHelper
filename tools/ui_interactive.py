#!/usr/bin/env python
"""DBHelper 交互式黑盒验证：点击进设置页 → 切换语言 → 切换字体 → 截图取证。

坐标基于窗口客户区（与 ui_blackbox.py 相同的 PrintWindow 抓图）。
"""
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ui_blackbox import find_window, screenshot, save_png, user32, SHOT_DIR, EXE

MOUSEMOVE = 0x0200
LBUTTONDOWN = 0x0201
LBUTTONUP = 0x0202


def _rect(hwnd):
    r = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(r))
    return r


def real_click(hwnd, x, y, settle=0.7):
    """真实鼠标输入：移动到窗口内 (x,y) 并点击（客户区坐标）。

    比 PostMessage 合成事件可靠——iced/winit 走的是系统输入队列。
    """
    r = _rect(hwnd)
    sx = r.left + x
    sy = r.top + y
    user32.SetCursorPos(sx, sy)
    time.sleep(0.25)
    user32.mouse_event(0x0002, 0, 0, 0, 0)  # LEFTDOWN
    time.sleep(0.08)
    user32.mouse_event(0x0004, 0, 0, 0, 0)  # LEFTUP
    time.sleep(settle)


def real_key(vk, settle=0.35):
    user32.keybd_event(vk, 0, 0, 0)
    time.sleep(0.08)
    user32.keybd_event(vk, 0, 0x0002, 0)
    time.sleep(settle)


def shot(hwnd, name):
    buf, (w, h) = screenshot(hwnd, None)
    if buf is None:
        print("  截图失败:", name)
        return None
    p = os.path.join(SHOT_DIR, name)
    save_png(buf, w, h, p)
    print("  截图:", p)
    return p


def main():
    proc = subprocess.Popen([EXE])
    time.sleep(6)
    if proc.poll() is not None:
        print("FAIL: 启动即退出")
        return 1
    hwnd = find_window(proc.pid)
    if not hwnd:
        print("FAIL: 无窗口")
        proc.terminate()
        return 1

    user32.SetForegroundWindow(hwnd)
    time.sleep(0.5)
    rect = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    W = rect.right - rect.left
    H = rect.bottom - rect.top
    print("窗口 %dx%d" % (W, H))
    # 截图是 2 倍缩放显示的，坐标按实际像素给出
    shot(hwnd, "bb_00_start.png")

    # 1) 点击侧栏底部的齿轮（设置）—— 截图显示在 (168,644)*2
    print("点击设置齿轮")
    real_click(hwnd, 336, 1288)
    shot(hwnd, "bb_01_settings.png")

    # 2) 点击语言下拉（设置页「语言」行右侧）—— 显示坐标 (975,232)*2
    print("点击语言下拉")
    real_click(hwnd, 1950, 464)
    shot(hwnd, "bb_02_lang_open.png")

    # 3) 直接点 English（显示坐标 888,276 → 实际 1776,552）
    print("点击 English")
    real_click(hwnd, 1847, 555)
    shot(hwnd, "bb_03_lang_en.png")

    # 4) 点击「显示字体」下拉 —— 显示坐标 (860,316)*2
    print("点击显示字体下拉")
    real_click(hwnd, 1720, 632)
    shot(hwnd, "bb_04_font_open.png")

    # 5) 选第二项（Noto Serif SC）
    print("选择 Noto Serif SC")
    real_click(hwnd, 1780, 723)
    shot(hwnd, "bb_05_font_serif.png")

    alive = proc.poll() is None
    print("交互后进程存活:", alive)
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    return 0 if alive else 1


if __name__ == "__main__":
    sys.exit(main())

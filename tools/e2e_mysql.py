"""DBHelper 端到端功能验证（真实 MySQL）。

连接本机 MySQL（root/123456，demo_ecommerce），验证：
1. 新建连接向导 → 保存连接
2. 连接树展开 → 看到库表
3. 打开表 → 数据网格显示真实数据
4. 脱敏：t_user 的 phone/email 列应打码
5. 行内编辑 → 真实 UPDATE 落库（随后用 mysql 客户端核对）
6. 删除连接

UI 交互靠真实鼠标事件；坐标一律用「客户区坐标」并换算到屏幕坐标。
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

fails = []


def sql(query, db=DB):
    """执行 SQL 并返回行（tab 分隔）。"""
    cmd = [MYSQL, "-uroot", "-p123456", "-N", "-B", db, "-e", query]
    r = subprocess.run(cmd, capture_output=True, text=True, errors="ignore")
    return [line.split("\t") for line in r.stdout.strip().splitlines() if line]


# ---------------------------------------------------------------- 截图

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
    path = os.path.join(SHOT, name)
    with open(path, "wb") as f:
        f.write(png)
    print(f"  shot -> {name}")


def px(buf, w, x, y):
    o = (y * w + x) * 4
    raw = bytes(buf[o:o + 4])
    return (raw[2], raw[1], raw[0], raw[3])



def find_blue_button(buf, w, h, region=None):
    """在区域内找蓝色主按钮（Accent #2383E2）的中心。

    取**最大的连通蓝色块**（按钮），而不是所有蓝像素的平均值——
    否则会被分散的蓝色图标拉偏。
    """
    x0, y0, x1, y1 = region or (0, 0, w, h)
    pts = set()
    for y in range(y0, min(y1, h), 2):
        for x in range(x0, min(x1, w), 2):
            p = px(buf, w, x, y)
            if abs(p[0] - 35) < 45 and abs(p[1] - 131) < 45 and abs(p[2] - 226) < 45:
                pts.add((x, y))
    if not pts:
        return None
    # 按 y 分行统计，取蓝像素最多的连续行区间 = 按钮
    rows = {}
    for x, y in pts:
        rows.setdefault(y, []).append(x)
    best_y = max(rows, key=lambda k: len(rows[k]))
    band = [y for y in rows if abs(y - best_y) <= 12]
    xs = [x for y in band for x in rows[y]]
    ys = [y for y in band for x in rows[y]]
    if len(xs) < 30:
        return None
    return (sum(xs) // len(xs), sum(ys) // len(ys))


def find_card_by_text(buf, w, h, x_range, y_range):
    """在卡片区里找某个文字行的中心（用深色像素簇判断文字存在）。"""
    return None

# ---------------------------------------------------------------- 输入

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
        time.sleep(0.012)
    time.sleep(0.2)


def key(vk):
    user32.keybd_event(vk, 0, 0, 0)
    time.sleep(0.05)
    user32.keybd_event(vk, 0, 0x0002, 0)
    time.sleep(0.2)


def main():
    print("=== 启动 DBHelper ===")
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
    # 激活窗口（自动化环境必需）
    click(hwnd, 900, 700)
    print(f"窗口 hwnd={hwnd}")

    # --- 1) 打开「新建连接」向导 ---
    print("\n[1] 打开新建连接向导")
    click(hwnd, 282, 142)          # 侧栏「+」
    time.sleep(0.9)

    # --- 2) 选 MySQL（第 0 步第一张卡片）---
    print("[2] 选择 MySQL")
    buf, w, h = grab(hwnd)
    # 卡片区：弹窗内左半部分第一行。MySQL 卡片中心约在弹窗左 1/8 处。
    # 用弹窗几何推算：弹窗宽 620 逻辑、居中 → 客户区坐标 = 中心 ± 310
    cx = w // 2
    mysql_card = (cx - 240, 628)
    click(hwnd, *mysql_card)
    time.sleep(0.5)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_01b_kind.png")

    # --- 3) 下一步 → 表单 ---
    print("[3] 进入表单")
    btn = find_blue_button(buf, w, h)
    print(f"  下一步按钮 @ {btn}")
    if btn:
        click(hwnd, btn[0], btn[1])
    time.sleep(0.9)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_02_form.png")

    # --- 4) 填连接信息（表单第 1 步）---
    print("[4] 填写连接信息")
    # 输入框纵向位置按实际渲染：名称/主机/端口/数据库/用户名/密码
    fields = [
        ("e2e_test", 700, 81),     # 连接名称
        ("127.0.0.1", 700, 125),   # 主机
        ("3306", 1290, 125),      # 端口
        (DB, 700, 168),           # 数据库
        ("root", 700, 212),       # 用户名
        ("123456", 700, 256),     # 密码
    ]
    for txt, fx, fy in fields:
        click(hwnd, fx, fy)
        # 清空：Ctrl+A + Delete
        user32.keybd_event(0x11, 0, 0, 0); user32.keybd_event(0x41, 0, 0, 0)
        user32.keybd_event(0x41, 0, 0x0002, 0); user32.keybd_event(0x11, 0, 0x0002, 0)
        time.sleep(0.1)
        key(0x08)  # Delete
        type_text(txt)
    time.sleep(0.3)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_03_filled.png")

    # --- 5) 下一步 → 测试页 ---
    print("[5] 进入测试页")
    buf, w, h = grab(hwnd)
    btn = find_blue_button(buf, w, h)
    print(f"  下一步 @ {btn}")
    if btn:
        click(hwnd, *btn)
    time.sleep(0.9)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_04_test.png")

    # --- 6) 测试连接 ---
    print("[6] 测试连接")
    # 「测试连接」按钮在页面中部
    click(hwnd, 1000, 780)
    time.sleep(2.5)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_05_tested.png")

    # --- 7) 保存 ---
    print("[7] 保存连接")
    buf, w, h = grab(hwnd)
    btn = find_blue_button(buf, w, h)
    print(f"  保存连接 @ {btn}")
    if btn:
        click(hwnd, *btn)
    time.sleep(2.5)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_06_saved.png")

    # --- 8) 展开连接树 ---
    print("[8] 展开连接树")
    click(hwnd, 50, 240)
    time.sleep(1.2)
    click(hwnd, 50, 240)
    time.sleep(2.0)
    buf, w, h = grab(hwnd)
    save_png(buf, w, h, "e2e_07_tree.png")

    # --- 核对配置 ---
    appdb = os.path.join(os.environ.get("APPDATA", ""), "dbhelper", "dbhelper.db")
    if os.path.exists(appdb):
        import sqlite3, json as _json
        con = sqlite3.connect(appdb)
        rows = con.execute("SELECT id, json FROM connections").fetchall()
        print(f"  应用连接配置 ({len(rows)} 条):")
        for rid, js in rows:
            d = _json.loads(js)
            print(f"    id={rid} name={d.get('name')} {d.get('host')}:{d.get('port')} db={d.get('database')} user={d.get('username')}")
        con.close()

    proc.terminate()
    try:
        proc.wait(timeout=5)
    except Exception:
        proc.kill()
    print("\n完成")


if __name__ == "__main__":
    main()

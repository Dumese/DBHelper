"""以完全脱离方式启动 DBHelper（避免随父 shell 退出而被结束）。"""

import subprocess
import sys

exe = sys.argv[1] if len(sys.argv) > 1 else r"D:\Code\dbhelper\target\debug\dbhelper.exe"
DETACHED_PROCESS = 0x00000008
CREATE_NEW_PROCESS_GROUP = 0x00000200

p = subprocess.Popen(
    [exe],
    cwd=r"D:\Code\dbhelper",
    creationflags=DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
    close_fds=True,
)
print("pid", p.pid)

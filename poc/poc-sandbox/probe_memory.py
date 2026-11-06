#!/usr/bin/env python3
# 内存占用探测：起 1 个沙箱保持运行，打印其 id 与 guest 内 free -m，供宿主侧采样
import os
import sys
import time

from cubesandbox import Sandbox

TEMPLATE_ID = os.environ["CUBE_TEMPLATE_ID"]
sb = Sandbox.create(template=TEMPLATE_ID)
sb.commands.run("true")
print("SANDBOX_ID", sb.sandbox_id, flush=True)
print("GUEST_FREE", sb.commands.run("free -m | tr '\\n' '|'").stdout.replace("\n", " "), flush=True)
print("GUEST_NPROC", sb.commands.run("nproc").stdout.strip(), flush=True)
# 保持 60 秒供外部采样
time.sleep(60)
sb.kill()
print("PROBE_DONE", flush=True)

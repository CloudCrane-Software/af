#!/usr/bin/env python3
# poc-sandbox 最小闭环：创建沙箱 → 执行 shell → 文件写入/取出 → 快照/恢复
# 运行环境变量：
#   CUBE_API_URL=http://127.0.0.1:13000
#   CUBE_TEMPLATE_ID=tpl-xxx
#   SSL_CERT_FILE=cube-rootCA.pem
import os
import sys
import time

from cubesandbox import Sandbox

TEMPLATE_ID = os.environ["CUBE_TEMPLATE_ID"]
results = []


def step(name, fn):
    t0 = time.perf_counter()
    out = fn()
    dt = (time.perf_counter() - t0) * 1000
    results.append((name, dt, out))
    print(f"[OK] {name}  ({dt:.0f} ms)  -> {out}", flush=True)
    return out


# 1. 创建沙箱（冷启动，从模板）
t0 = time.perf_counter()
sb = Sandbox.create(template=TEMPLATE_ID)
create_ms = (time.perf_counter() - t0) * 1000
print(f"[OK] sandbox.create  ({create_ms:.0f} ms)  -> id={sb.sandbox_id}", flush=True)

try:
    # 2. 执行一条 shell
    r = step("commands.run('echo...')",
             lambda: sb.commands.run("echo hello-from-cube && uname -r && id -u").stdout.strip())

    # 3. 文件写入沙箱 → 从沙箱取出
    local_src = "/tmp/poc_upload.txt" if False else os.path.join(os.path.dirname(os.path.abspath(__file__)), "poc_upload.txt")
    with open(local_src, "w") as f:
        f.write("cube-file-exchange-payload-2026")
    step("files.write", lambda: sb.files.write("/tmp/poc_upload.txt", open(local_src, "rb").read()))
    got = step("files.read", lambda: sb.files.read("/tmp/poc_upload.txt"))
    content = got.decode() if isinstance(got, (bytes, bytearray)) else str(got)
    assert content == "cube-file-exchange-payload-2026", f"content mismatch: {content!r}"
    # 在沙箱内 shell 校验同一文件（证明写入真实落盘）
    r2 = step("cat inside sandbox",
              lambda: sb.commands.run("cat /tmp/poc_upload.txt").stdout.strip())
    assert r2 == "cube-file-exchange-payload-2026", f"in-sandbox cat mismatch: {r2!r}"

    # 4. 快照 → 破坏状态 → 回滚恢复
    step("run_code write v1", lambda: sb.run_code("open('/tmp/v.txt','w').write('v1')"))
    snap = step("create_snapshot", lambda: sb.create_snapshot().snapshot_id)
    sb.run_code("open('/tmp/v.txt','w').write('v2')")
    before = sb.run_code("print(open('/tmp/v.txt').read())").logs.stdout[0].strip()
    assert before == "v2", f"before rollback expected v2, got {before!r}"
    step("rollback", lambda: sb.rollback(snap))
    after = sb.run_code("print(open('/tmp/v.txt').read())").logs.stdout[0].strip()
    assert after == "v1", f"after rollback expected v1, got {after!r}"
    print("[OK] snapshot/rollback round-trip: v2 -> rollback -> v1", flush=True)

    # 5. 从快照克隆出新沙箱（状态延续验证）
    def _clone():
        c = Sandbox.create(template=snap)
        out = c.run_code("print(open('/tmp/v.txt').read())").logs.stdout[0].strip()
        c.kill()
        return out
    cloned_state = step("clone from snapshot", _clone)
    assert cloned_state == "v1", f"cloned sandbox state mismatch: {cloned_state!r}"

    print("CLOSED_LOOP_ALL_OK", flush=True)
finally:
    sb.kill()
    print(f"[OK] sandbox.kill -> {sb.sandbox_id}", flush=True)

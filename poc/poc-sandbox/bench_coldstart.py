#!/usr/bin/env python3
# 冷启动计时 10 次：Sandbox.create（从 READY 模板冷启动到可用）+ 首条命令时延
import os
import statistics
import time

from cubesandbox import Sandbox

TEMPLATE_ID = os.environ["CUBE_TEMPLATE_ID"]
N = 10

create_ms = []
first_cmd_ms = []
ids = []
for i in range(N):
    t0 = time.perf_counter()
    sb = Sandbox.create(template=TEMPLATE_ID)
    t1 = time.perf_counter()
    sb.commands.run("true")
    t2 = time.perf_counter()
    create_ms.append((t1 - t0) * 1000)
    first_cmd_ms.append((t2 - t1) * 1000)
    ids.append(sb.sandbox_id)
    sb.kill()
    print(f"run{i+1:02d} create={create_ms[-1]:.0f}ms first_cmd={first_cmd_ms[-1]:.0f}ms id={ids[-1]}", flush=True)
    time.sleep(0.3)


def stats(xs):
    return {
        "n": len(xs),
        "min": round(min(xs), 1),
        "p50": round(statistics.median(xs), 1),
        "mean": round(statistics.mean(xs), 1),
        "p90": round(sorted(xs)[int(0.9 * len(xs)) - 1], 1),
        "max": round(max(xs), 1),
        "stdev": round(statistics.stdev(xs), 1) if len(xs) > 1 else 0.0,
    }


print("CREATE_MS", stats(create_ms), flush=True)
print("FIRST_CMD_MS", stats(first_cmd_ms), flush=True)

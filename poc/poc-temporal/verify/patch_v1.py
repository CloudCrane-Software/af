"""验证项 B —— v1（初始版）：sleep 窗口后只走旧分支 v1-path。

三代文件 patch_v1/v2/v3.py 的 workflow 类名都叫 PatchWorkflow、同 task queue，
worker 进程按代加载不同模块即可换"部署版本"。
"""
import asyncio
import datetime
import os
import time
from dataclasses import dataclass
from typing import Any, Dict, List

import temporalio.activity
import temporalio.workflow

TASK_QUEUE = "verify-patch-queue"
TRACE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                     "verify-logs", "patch_trace.log")
GEN = "v1"  # 代码代际标记（worker 日志/trace 里标识当次加载的是哪代代码）


def trace(line: str) -> None:
    now = time.time()
    stamp = time.strftime("%H:%M:%S", time.localtime(now)) + f".{int(now * 1000) % 1000:03d}"
    with open(TRACE, "a") as f:
        f.write(f"[{stamp}] {line}\n")


def worker_role() -> str:
    return os.environ.get("WORKER_ROLE", "??")


@temporalio.activity.defn
async def record_path(path: str, seconds: float = 4.0) -> str:
    info = temporalio.activity.info()
    trace(f"ACT started path={path} attempt={info.attempt} worker={worker_role()} "
          f"pid={os.getpid()} workflow_id={info.workflow_id}")
    left = seconds
    while left > 0:
        step = min(1.0, left)
        await asyncio.sleep(step)
        left -= step
        temporalio.activity.heartbeat(f"{seconds - left:.1f}/{seconds}")
    trace(f"ACT completed path={path} attempt={info.attempt} worker={worker_role()} pid={os.getpid()}")
    return f"recorded:{path}"


@dataclass
class PatchInput:
    label: str = ""
    sleep_seconds: float = 15.0  # 换代码窗口


@temporalio.workflow.defn
class PatchWorkflow:
    @temporalio.workflow.run
    async def run(self, inp: PatchInput) -> Dict[str, Any]:
        info = temporalio.workflow.info()
        paths: List[str] = []
        trace(f"WF[{GEN}] run wid={info.workflow_id} replaying="
              f"{temporalio.workflow.unsafe.is_replaying()}")
        await temporalio.workflow.sleep(inp.sleep_seconds)  # 换代码窗口
        # v1：只有旧分支
        r = await temporalio.workflow.execute_activity(
            record_path, args=["v1-path", 4.0],
            start_to_close_timeout=datetime.timedelta(seconds=60),
            heartbeat_timeout=datetime.timedelta(seconds=3),
        )
        paths.append(r)
        trace(f"WF[{GEN}] branch-taken=v1-path wid={info.workflow_id} "
              f"replaying={temporalio.workflow.unsafe.is_replaying()}")
        return {"label": inp.label, "paths": paths}

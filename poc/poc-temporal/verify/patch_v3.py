"""验证项 B —— v3：deprecate_patch("add-fastpath") 后无条件走新分支。

前提（文档语义）：所有"早于补丁"的无 marker 旧 run 都已结束后才能 deprecate。
负例 patch-run-mix 会故意违反这个前提，验证会发生什么。
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
GEN = "v3"


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
    sleep_seconds: float = 15.0


@temporalio.workflow.defn
class PatchWorkflow:
    @temporalio.workflow.run
    async def run(self, inp: PatchInput) -> Dict[str, Any]:
        info = temporalio.workflow.info()
        paths: List[str] = []
        trace(f"WF[{GEN}] run wid={info.workflow_id} replaying="
              f"{temporalio.workflow.unsafe.is_replaying()}")
        await temporalio.workflow.sleep(inp.sleep_seconds)
        temporalio.workflow.deprecate_patch("add-fastpath")
        trace(f"WF[{GEN}] deprecate_patch('add-fastpath') called wid={info.workflow_id} "
              f"replaying={temporalio.workflow.unsafe.is_replaying()}")
        # 无条件新分支（v2 的 patched() 分支已收拢）
        r = await temporalio.workflow.execute_activity(
            record_path, args=["v2-fastpath", 1.0],
            start_to_close_timeout=datetime.timedelta(seconds=60),
            heartbeat_timeout=datetime.timedelta(seconds=3),
        )
        paths.append(r)
        trace(f"WF[{GEN}] branch-taken=v2-fastpath(unconditional) wid={info.workflow_id}")
        return {"label": inp.label, "paths": paths}

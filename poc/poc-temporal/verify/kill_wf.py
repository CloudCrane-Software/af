"""验证项 A：worker 被 kill -9 后的任务重投递 / 双 worker 接管 / workflow 层续跑。

- activity trace_activity：写 "started attempt N"（带 worker 角色+pid）→ sleep → 写 "completed"。
- KillTestWorkflow：两个 activity（step1=2s, step2=可配秒数），workflow 自身也向同一 trace 文件
  写调度标记（含 unsafe.is_replaying()），用于证明重启后是 replay 续跑、step1 不重跑。
- heartbeat=True 时 activity 每秒 heartbeat 一次（heartbeat_timeout=3s，让 server 快速发现死 worker）；
  heartbeat=False 时观察纯 StartToClose 路径。
"""
import asyncio
import datetime
import os
import time
from dataclasses import dataclass
from typing import Any, Dict

import temporalio.activity
import temporalio.common
import temporalio.workflow

TASK_QUEUE = "verify-kill-queue"
_TRACE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                      "verify-logs", "activity_trace.log")


def trace(line: str) -> None:
    """activity 与 workflow 共用的本地轨迹文件（workflow 里也允许写，replay 时会重写，正好作证据）。"""
    now = time.time()
    stamp = time.strftime("%H:%M:%S", time.localtime(now)) + f".{int(now * 1000) % 1000:03d}"
    with open(_TRACE, "a") as f:
        f.write(f"[{stamp}] {line}\n")


def worker_role() -> str:
    return os.environ.get("WORKER_ROLE", "??")


@temporalio.activity.defn
async def trace_activity(tag: str, seconds: float, heartbeat: bool) -> Dict[str, Any]:
    info = temporalio.activity.info()
    role = worker_role()
    trace(f"ACT started tag={tag} attempt={info.attempt} worker={role} pid={os.getpid()} "
          f"activity_id={info.activity_id} workflow_id={info.workflow_id}")
    if heartbeat:
        left = seconds
        while left > 0:
            step = min(1.0, left)
            await asyncio.sleep(step)
            left -= step
            temporalio.activity.heartbeat(f"progress={seconds - left:.1f}/{seconds}")
    else:
        await asyncio.sleep(seconds)
    trace(f"ACT completed tag={tag} attempt={info.attempt} worker={role} pid={os.getpid()}")
    return {"tag": tag, "attempt": info.attempt, "worker": role, "pid": os.getpid()}


@dataclass
class KillInput:
    label: str = ""
    step2_seconds: float = 10.0
    heartbeat: bool = True
    slc_seconds: int = 60  # start_to_close（验证无心跳/StartToClose 路径时可调小）
    gap_seconds: float = 0.0  # 两个 activity 之间的纯定时器窗口（kill 命中窗口用）


@temporalio.workflow.defn
class KillTestWorkflow:
    @temporalio.workflow.run
    async def run(self, inp: KillInput) -> Dict[str, Any]:
        info = temporalio.workflow.info()
        trace(f"WF run wid={info.workflow_id} run={info.run_id[:8]} "
              f"replaying={temporalio.workflow.unsafe.is_replaying()}")
        slc = datetime.timedelta(seconds=inp.slc_seconds)
        hb = datetime.timedelta(seconds=3) if inp.heartbeat else None
        retry = temporalio.common.RetryPolicy(
            initial_interval=datetime.timedelta(seconds=1),
            backoff_coefficient=1.5,
            maximum_attempts=10,
        )
        results = []
        for i, (tag, secs) in enumerate((("step1", 2.0), ("step2", inp.step2_seconds))):
            if i == 1 and inp.gap_seconds > 0:
                trace(f"WF gap-sleep enter seconds={inp.gap_seconds} "
                      f"replaying={temporalio.workflow.unsafe.is_replaying()} wid={info.workflow_id}")
                await temporalio.workflow.sleep(inp.gap_seconds)
            trace(f"WF schedule tag={tag} replaying={temporalio.workflow.unsafe.is_replaying()} "
                  f"wid={info.workflow_id}")
            r = await temporalio.workflow.execute_activity(
                trace_activity,
                args=[tag, secs, inp.heartbeat],
                start_to_close_timeout=slc,
                heartbeat_timeout=hb,
                retry_policy=retry,
            )
            results.append(r)
        trace(f"WF done wid={info.workflow_id} "
              f"replaying={temporalio.workflow.unsafe.is_replaying()}")
        return {"label": inp.label, "steps": results}

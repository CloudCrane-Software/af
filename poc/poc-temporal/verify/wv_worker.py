"""加分项：Worker Deployments（v2 worker versioning）双版本并存路由。

用法：python wv_worker.py <build_id> <role> [pidfile]
deployment 名固定 verify-dep，task queue 固定 verify-wv-queue。
"""
import asyncio
import datetime
import os
import sys
from typing import Any, Dict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from temporalio.client import Client
from temporalio.common import VersioningBehavior
from temporalio.worker import (
    Worker,
    WorkerDeploymentConfig,
    WorkerDeploymentVersion,
)

import temporalio.activity
import temporalio.workflow

import kill_wf  # 复用 trace 基建（trace 文件、activity 形态）

DEPLOYMENT = "verify-dep"
QUEUE = "verify-wv-queue"
WV_TRACE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                        "verify-logs", "wv_trace.log")


def wtrace(line: str) -> None:
    import time
    now = time.time()
    stamp = time.strftime("%H:%M:%S", time.localtime(now)) + f".{int(now * 1000) % 1000:03d}"
    with open(WV_TRACE, "a") as f:
        f.write(f"[{stamp}] {line}\n")


@temporalio.activity.defn
async def wv_activity(tag: str) -> Dict[str, Any]:
    build = os.environ.get("WORKER_BUILD", "??")
    wtrace(f"ACT {tag} executed by build={build} pid={os.getpid()}")
    return {"tag": tag, "build": build, "pid": os.getpid()}


@temporalio.workflow.defn
class WvWorkflow:
    @temporalio.workflow.run
    async def run(self, tag: str) -> Dict[str, Any]:
        # 等信号再执行 activity：留出"改 current version 后在飞 run 是否钉版本"的观察窗口
        await temporalio.workflow.wait_condition(lambda: self._go)
        r = await temporalio.workflow.execute_activity(
            wv_activity, args=[tag], start_to_close_timeout=datetime.timedelta(seconds=30)
        )
        return r

    def __init__(self) -> None:
        self._go = False

    @temporalio.workflow.signal
    def go(self) -> None:
        self._go = True


async def main() -> None:
    build_id = sys.argv[1]
    role = sys.argv[2] if len(sys.argv) > 2 else build_id
    os.environ["WORKER_BUILD"] = build_id
    if len(sys.argv) > 3:
        with open(sys.argv[3], "w") as f:
            f.write(str(os.getpid()))
    client = await Client.connect("localhost:7234")
    worker = Worker(
        client,
        task_queue=QUEUE,
        workflows=[WvWorkflow],
        activities=[wv_activity],
        deployment_config=WorkerDeploymentConfig(
            version=WorkerDeploymentVersion(deployment_name=DEPLOYMENT, build_id=build_id),
            use_worker_versioning=True,
            default_versioning_behavior=VersioningBehavior.PINNED,
        ),
    )
    print(f"wv worker up build={build_id} role={role} deployment={DEPLOYMENT} queue={QUEUE}",
          flush=True)
    await worker.run()


if __name__ == "__main__":
    asyncio.run(main())

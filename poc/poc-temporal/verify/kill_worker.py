"""验证项 A 的 worker 进程。

用法：python kill_worker.py <role> [pidfile]
启动即把 pid 写入 pidfile（供 kill -9），角色名进 activity trace。
"""
import asyncio
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from temporalio.client import Client
from temporalio.worker import UnsandboxedWorkflowRunner, Worker

import kill_wf

# 验证需要 workflow 代码直接写本地 trace 文件（展示 replay），故关闭默认沙箱。
# 生产工作流禁止这样做；PoC 验证专用。


async def main() -> None:
    role = sys.argv[1] if len(sys.argv) > 1 else "worker"
    os.environ["WORKER_ROLE"] = role
    if len(sys.argv) > 2:
        with open(sys.argv[2], "w") as f:
            f.write(str(os.getpid()))
    client = await Client.connect("localhost:7234")
    worker = Worker(
        client,
        task_queue=kill_wf.TASK_QUEUE,
        workflows=[kill_wf.KillTestWorkflow],
        activities=[kill_wf.trace_activity],
        workflow_runner=UnsandboxedWorkflowRunner(),
    )
    print(f"worker up role={role} pid={os.getpid()} queue={kill_wf.TASK_QUEUE}", flush=True)
    await worker.run()


if __name__ == "__main__":
    asyncio.run(main())

"""验证项 B 的 worker：python patch_worker.py <模块名:patch_v1|patch_v2|patch_v3> <role> [pidfile]

按代加载不同模块（同一个 workflow 类型名 PatchWorkflow、同 task queue），模拟"换部署代码"。
"""
import asyncio
import importlib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from temporalio.client import Client
from temporalio.worker import UnsandboxedWorkflowRunner, Worker


async def main() -> None:
    module_name = sys.argv[1]
    role = sys.argv[2] if len(sys.argv) > 2 else module_name
    os.environ["WORKER_ROLE"] = role
    if len(sys.argv) > 3:
        with open(sys.argv[3], "w") as f:
            f.write(str(os.getpid()))
    mod = importlib.import_module(module_name)
    client = await Client.connect("localhost:7234")
    worker = Worker(
        client,
        task_queue=mod.TASK_QUEUE,
        workflows=[mod.PatchWorkflow],
        activities=[mod.record_path],
        workflow_runner=UnsandboxedWorkflowRunner(),
    )
    print(f"patch worker up module={module_name} gen={mod.GEN} role={role} pid={os.getpid()} "
          f"queue={mod.TASK_QUEUE}", flush=True)
    await worker.run()


if __name__ == "__main__":
    asyncio.run(main())

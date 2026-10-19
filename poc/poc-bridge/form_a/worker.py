"""形态A常驻 worker：跑 BridgeOrderA + 引擎调用/账本 activities。"""
import asyncio
import logging
import sys

from temporalio.client import Client
from temporalio.worker import Worker

import workflow as wf


async def main() -> None:
    logging.basicConfig(level=logging.WARNING)
    client = await Client.connect("localhost:7240")
    worker = Worker(
        client,
        task_queue=wf.TASK_QUEUE,
        workflows=[wf.BridgeOrderA],
        activities=[wf.sign_voucher, wf.forge_artifact, wf.engine_call],
    )
    print(f"worker-A up, task_queue={wf.TASK_QUEUE}, pid={__import__('os').getpid()}", flush=True)
    await worker.run()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)

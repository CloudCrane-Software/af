"""形态B常驻 worker：跑 BridgeOrderB + 账本 activities。"""
import asyncio
import logging
import os
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
        workflows=[wf.BridgeOrderB],
        activities=[wf.sign_voucher, wf.forge_artifact],
    )
    print(f"worker-B up, task_queue={wf.TASK_QUEUE}, pid={os.getpid()}", flush=True)
    await worker.run()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)

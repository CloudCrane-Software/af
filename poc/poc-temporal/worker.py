"""常驻 worker：跑 OrderWorkflow / FulfillItemWorkflow + ledger activities。"""
import asyncio
import logging
import sys

from temporalio.client import Client
from temporalio.worker import Worker

import poc_workflow as pw


async def main() -> None:
    logging.basicConfig(level=logging.WARNING)
    client = await Client.connect("localhost:7233")
    worker = Worker(
        client,
        task_queue=pw.TASK_QUEUE,
        workflows=[pw.OrderWorkflow, pw.FulfillItemWorkflow],
        activities=[pw.reserve_budget, pw.commit_budget_deduct, pw.fulfill_item],
    )
    print(f"worker up, task_queue={pw.TASK_QUEUE}", flush=True)
    await worker.run()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)

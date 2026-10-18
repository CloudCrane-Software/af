"""加分项驱动：python drive_wv.py start <wf_id> <tag> | signal <wf_id> | result <wf_id> <timeout>"""
import asyncio
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from temporalio.client import Client

import wv_worker


async def main() -> None:
    cmd, wf_id = sys.argv[1], sys.argv[2]
    client = await Client.connect("localhost:7234")
    if cmd == "start":
        handle = await client.start_workflow(
            wv_worker.WvWorkflow.run, sys.argv[3],
            id=wf_id, task_queue=wv_worker.QUEUE,
        )
        print(json.dumps({"workflow_id": handle.id, "run_id": handle.first_execution_run_id}))
    elif cmd == "signal":
        await client.get_workflow_handle(wf_id).signal(wv_worker.WvWorkflow.go)
        print(json.dumps({"signaled": True}))
    elif cmd == "result":
        try:
            r = await asyncio.wait_for(client.get_workflow_handle(wf_id).result(),
                                       timeout=float(sys.argv[3]))
            print(json.dumps({"ok": True, "result": r}))
        except Exception as e:
            print(json.dumps({"ok": False, "error": f"{type(e).__name__}: {e}"}))
            sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())

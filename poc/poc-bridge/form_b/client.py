"""形态B驱动端：start / approve / status / wait（与形态A client 对齐）。"""
import asyncio
import json
import sys

from temporalio.client import Client

import workflow as wf


async def main() -> None:
    cmd = sys.argv[1]
    wid = sys.argv[2]
    client = await Client.connect("localhost:7240")
    handle = client.get_workflow_handle(wid)
    if cmd == "start":
        s = wf.StateB(order_id=wid,
                      with_deadline="--deadline" in sys.argv,
                      use_can="--can" in sys.argv)
        h = await client.start_workflow(wf.BridgeOrderB.run, s, id=wid, task_queue=wf.TASK_QUEUE)
        print(json.dumps({"workflow_id": wid, "run_id": h.first_execution_run_id}))
    elif cmd == "approve":
        await handle.signal(wf.BridgeOrderB.approve, sys.argv[3])
        print(json.dumps({"approved": wid, "by": sys.argv[3]}))
    elif cmd == "status":
        print(json.dumps(await handle.query(wf.BridgeOrderB.status), ensure_ascii=False))
    elif cmd == "wait":
        result = await handle.result()
        print(json.dumps(result, ensure_ascii=False, indent=1))
    else:
        raise SystemExit(f"unknown cmd {cmd}")


if __name__ == "__main__":
    asyncio.run(main())

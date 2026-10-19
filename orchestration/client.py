"""形态A驱动端：start / approve / status / wait。

用法:
  python client.py start <wf-id> [--deadline] [--can]   # 启动（不等待）
  python client.py approve <wf-id> <approver>           # 发审批 signal
  python client.py status <wf-id>                       # query status
  python client.py wait <wf-id>                         # 等终态并打印 result JSON
"""
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
        s = wf.StateA(order_id=wid,
                      with_deadline="--deadline" in sys.argv,
                      use_can="--can" in sys.argv)
        h = await client.start_workflow(wf.BridgeOrderA.run, s, id=wid, task_queue=wf.TASK_QUEUE)
        print(json.dumps({"workflow_id": wid, "run_id": h.first_execution_run_id}))
    elif cmd == "approve":
        await handle.signal(wf.BridgeOrderA.approve, sys.argv[3])
        print(json.dumps({"approved": wid, "by": sys.argv[3]}))
    elif cmd == "status":
        print(json.dumps(await handle.query(wf.BridgeOrderA.status), ensure_ascii=False))
    elif cmd == "wait":
        result = await handle.result()
        print(json.dumps(result, ensure_ascii=False, indent=1))
    else:
        raise SystemExit(f"unknown cmd {cmd}")


if __name__ == "__main__":
    asyncio.run(main())

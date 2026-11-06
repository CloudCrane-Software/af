"""验证项 A 的驱动客户端。

用法：
  python drive_kill.py start <wf_id> <step2_seconds> <heartbeat:0|1> <slc_seconds> <label>
  python drive_kill.py result <wf_id> <timeout_seconds>
"""
import argparse
import asyncio
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from temporalio.client import Client

import kill_wf


async def main() -> None:
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)

    sp = sub.add_parser("start")
    sp.add_argument("wf_id")
    sp.add_argument("step2_seconds", type=float)
    sp.add_argument("heartbeat", type=int)  # 0/1
    sp.add_argument("slc_seconds", type=int)
    sp.add_argument("label")
    sp.add_argument("--gap", type=float, default=0.0)
    sp = sub.add_parser("result")
    sp.add_argument("wf_id")
    sp.add_argument("timeout", type=float)

    args = p.parse_args()
    client = await Client.connect("localhost:7234")
    if args.cmd == "start":
        handle = await client.start_workflow(
            kill_wf.KillTestWorkflow.run,
            kill_wf.KillInput(
                label=args.label,
                step2_seconds=args.step2_seconds,
                heartbeat=bool(args.heartbeat),
                slc_seconds=args.slc_seconds,
                gap_seconds=args.gap,
            ),
            id=args.wf_id,
            task_queue=kill_wf.TASK_QUEUE,
        )
        print(json.dumps({"workflow_id": handle.id, "run_id": handle.first_execution_run_id}))
    elif args.cmd == "result":
        handle = client.get_workflow_handle(args.wf_id)
        try:
            r = await asyncio.wait_for(handle.result(), timeout=args.timeout)
            print(json.dumps({"ok": True, "result": r}))
        except Exception as e:
            print(json.dumps({"ok": False, "error": f"{type(e).__name__}: {e}"}))
            sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())

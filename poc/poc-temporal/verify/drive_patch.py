"""验证项 B 的驱动客户端。

用法：
  python drive_patch.py start <wf_id> <sleep_seconds> <label>
  python drive_patch.py result <wf_id> <timeout_seconds>
"""
import argparse
import asyncio
import json
import os
import sys
from typing import Any, Dict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from temporalio.client import Client

import patch_v1  # 仅用其 PatchInput（三代输入类型一致）


async def main() -> None:
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)

    sp = sub.add_parser("start")
    sp.add_argument("wf_id")
    sp.add_argument("sleep_seconds", type=float)
    sp.add_argument("label")
    sp = sub.add_parser("result")
    sp.add_argument("wf_id")
    sp.add_argument("timeout", type=float)

    args = p.parse_args()
    client = await Client.connect("localhost:7234")
    if args.cmd == "start":
        handle = await client.start_workflow(
            "PatchWorkflow",  # 按类型名字符串启动，避免驱动端绑定某代代码
            patch_v1.PatchInput(label=args.label, sleep_seconds=args.sleep_seconds),
            id=args.wf_id,
            task_queue=patch_v1.TASK_QUEUE,
            result_type=Dict[str, Any],
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

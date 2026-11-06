"""CLI 驱动：start / status / approve(signal) / approve-update / settle / result / ledger"""
import argparse
import asyncio
import datetime
import json
import sys

from temporalio.client import Client

import ledger
import poc_workflow as pw


async def get_client() -> Client:
    return await Client.connect("localhost:7233")


async def cmd_start(args: argparse.Namespace) -> None:
    client = await get_client()
    inp = pw.OrderState(
        order_id=args.wid,
        amount=args.amount,
        items=[f"item-{i}" for i in range(1, args.items + 1)],
    )
    handle = await client.start_workflow(
        pw.OrderWorkflow.run, inp, id=args.wid, task_queue=pw.TASK_QUEUE
    )
    print(json.dumps({"workflow_id": handle.id, "run_id": handle.first_execution_run_id}))


async def cmd_status(args: argparse.Namespace) -> None:
    client = await get_client()
    handle = client.get_workflow_handle(args.wid)
    try:
        st = await handle.query(pw.OrderWorkflow.status)
        print(json.dumps({"ok": True, "status": st}))
    except Exception as e:
        print(json.dumps({"ok": False, "error": f"{type(e).__name__}: {e}"}))


async def cmd_approve(args: argparse.Namespace) -> None:
    client = await get_client()
    handle = client.get_workflow_handle(args.wid)
    await handle.signal(pw.OrderWorkflow.approve, args.approver)
    print(json.dumps({"signaled": True, "approver": args.approver}))


async def cmd_approve_update(args: argparse.Namespace) -> None:
    client = await get_client()
    handle = client.get_workflow_handle(args.wid)
    r = await handle.execute_update(pw.OrderWorkflow.approve_via_update, args.approver)
    print(json.dumps({"update_result": r}))


async def cmd_settle(args: argparse.Namespace) -> None:
    client = await get_client()
    handle = client.get_workflow_handle(args.wid)
    await handle.signal(pw.OrderWorkflow.settle)
    print(json.dumps({"settled": True}))


async def cmd_result(args: argparse.Namespace) -> None:
    client = await get_client()
    handle = client.get_workflow_handle(args.wid)
    try:
        r = await asyncio.wait_for(handle.result(), timeout=args.timeout)
        print(json.dumps({"ok": True, "result": r}))
    except Exception as e:
        print(json.dumps({"ok": False, "error": f"{type(e).__name__}: {e}"}))
        sys.exit(1)


async def cmd_ledger(_: argparse.Namespace) -> None:
    snap = ledger.snapshot()
    print(json.dumps(snap))


def main() -> None:
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)

    sp = sub.add_parser("start")
    sp.add_argument("--wid", required=True)
    sp.add_argument("--amount", type=float, default=300.0)
    sp.add_argument("--items", type=int, default=5)
    sp.set_defaults(fn=cmd_start)

    for name, fn in [("status", cmd_status), ("approve", cmd_approve),
                     ("approve-update", cmd_approve_update), ("settle", cmd_settle),
                     ("result", cmd_result), ("ledger", cmd_ledger)]:
        sp = sub.add_parser(name)
        if name != "ledger":
            sp.add_argument("--wid", required=True)
        if name in ("approve", "approve-update"):
            sp.add_argument("--approver", default="alice")
        if name == "result":
            sp.add_argument("--timeout", type=float, default=60.0)
        sp.set_defaults(fn=fn)

    args = p.parse_args()
    asyncio.run(args.fn(args))


if __name__ == "__main__":
    main()

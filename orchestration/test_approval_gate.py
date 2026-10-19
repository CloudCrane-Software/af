"""F5 审批门集成测试。

跑法（仓库根）:

    source /data/af-factory/env.sh
    cargo build -j 4 -p af-engine --bin af-engine
    /opt/gpumachine/temporal-qw-worker/.venv/bin/python orchestration/test_approval_gate.py

本机 temporal CLI 由 WorkflowEnvironment.start_local 拉起（dev server）。
不读系统钟做判定。轮询只用于等 workflow 走到断言点，超时是测试失败，不是门控逻辑。
"""
from __future__ import annotations

import asyncio
import hashlib
import sys
import uuid
from typing import Any, Callable, Dict

from temporalio.client import Client, WorkflowFailureError, WorkflowHandle
from temporalio.exceptions import ApplicationError
from temporalio.testing import WorkflowEnvironment
from temporalio.worker import Worker

import identity
import workflow as wf

TEMPORAL_CLI = "/data/af-factory/temporal"


def seal(actor_id: str, role: str, decision: str, payload: str, key_id: str) -> Dict[str, Any]:
    """与 DecisionArtifact::seal 同形：角色默认权限、SHA-256、占位签名。"""
    permissions = {
        "Reader": ["Read"],
        "Approver": ["Read", "Approve"],
        "Reviser": ["Read", "Revise"],
        "Arbiter": ["Read", "Approve", "Revise", "Arbitrate"],
    }[role]
    return {
        "actor_id": actor_id,
        "role": role,
        "permissions": permissions,
        "decision": decision,
        "payload_hash": hashlib.sha256(payload.encode("utf-8")).hexdigest(),
        "signature": {
            "algorithm": "ed25519-placeholder",
            "key_id": key_id,
            "bytes": [0xDE] * 64,
        },
    }


def approval_for(order_id: str, *, stable_id: str | None = None, role: str = "Approver") -> Dict[str, Any]:
    sid = stable_id if stable_id is not None else identity.stable_id_for(order_id)
    payload = wf.bound_decision_payload(sid, "g-bridge")
    return {
        "approver": "user-approver",
        "role": role,
        "artifact": seal("user-approver", role, "Approve", payload, "k-approver"),
    }


def _failure_types(exc: BaseException) -> list[str]:
    found: list[str] = []
    seen: set[int] = set()
    cur: BaseException | None = exc
    while cur is not None and id(cur) not in seen:
        seen.add(id(cur))
        kind = getattr(cur, "type", None)
        found.append(f"{type(cur).__name__}:{kind}")
        nxt = getattr(cur, "cause", None)
        if not isinstance(nxt, BaseException):
            nxt = cur.__cause__ if isinstance(cur.__cause__, BaseException) else None
        cur = nxt
    return found


def assert_typed(exc: BaseException, kind: str) -> None:
    types = _failure_types(exc)
    assert any(item.endswith(f":{kind}") for item in types), types


async def wait_status(
    handle: WorkflowHandle, pred: Callable[[Dict[str, Any]], bool], timeout_s: float = 30
) -> Dict[str, Any]:
    loop = asyncio.get_running_loop()
    deadline = loop.time() + timeout_s
    last: Dict[str, Any] = {}
    while loop.time() < deadline:
        last = await handle.query(wf.BridgeOrderA.status)
        if pred(last):
            return last
        await asyncio.sleep(0.05)
    raise AssertionError(last)


async def start(client: Client, order_id: str) -> WorkflowHandle:
    return await client.start_workflow(
        wf.BridgeOrderA.run,
        wf.StateA(order_id=order_id),
        id=order_id,
        task_queue=wf.TASK_QUEUE,
    )


async def test_trace_reads_order_state_and_gate_missing() -> None:
    """旧代码读 resp['state'] 与 error.detail，两条都恒为 null。"""
    stale = {
        "ok": True,
        "state": None,
        "order": {"state": "Released"},
        "engine_version": "0.1.0",
    }
    record = wf.engine_trace_record({"op": "release", "tag": "release"}, stale)
    assert record["state_after"] == "Released", record
    assert record["tag"] == "release"

    wrong_key = {
        "ok": False,
        "error": {
            "kind": "Gate",
            "detail": {"PreconditionsNotMet": {"missing": ["WRONG"]}},
            "gate": {
                "PreconditionsNotMet": {
                    "transition": "Release",
                    "missing": ["LineVersionNotBound", "BudgetVoucherNotIssued"],
                }
            },
        },
    }
    record = wf.engine_trace_record({"op": "release", "tag": "release"}, wrong_key)
    assert record["missing"] == ["LineVersionNotBound", "BudgetVoucherNotIssued"], record
    assert record["state_after"] is None

    order_id = "trace-" + uuid.uuid4().hex
    order = {
        "id": order_id,
        "stable_id": identity.stable_id_for(order_id),
        "run_id": "",
        "state": "Created",
        "graph_version": "g-bridge",
        "line": None,
        "budget_voucher": None,
        "artifacts": [],
        "sla": {"deadline": "2026-12-31T00:00:00Z", "priority": "High"},
        "supersedes": None,
        "history": [],
    }
    resp = await wf.engine_call({"op": "release", "order": order, "tag": "release"})
    assert resp["ok"] is False, resp
    live = wf.engine_trace_record({"op": "release", "tag": "release"}, resp)
    assert live["state_after"] is None, live
    assert live["missing"] == ["LineVersionNotBound", "BudgetVoucherNotIssued"], live

    payload = wf.bound_decision_payload(order["stable_id"], "g-bridge")
    artifact = seal("user-approver", "Approver", "Approve", payload, "k-approver")
    ok = await wf.engine_call(
        {
            "op": "approve_decision",
            "order": order,
            "tag": "approve_decision",
            "artifact": artifact,
            "payload": payload,
        }
    )
    assert ok["ok"] is True, ok
    live_ok = wf.engine_trace_record({"op": "approve_decision", "tag": "approve_decision"}, ok)
    assert live_ok["state_after"] == "Created", live_ok
    assert live_ok["missing"] is None


async def test_unapproved_release_is_rejected(client: Client) -> None:
    order_id = "unapproved-" + uuid.uuid4().hex
    handle = await start(client, order_id)
    status = await wait_status(
        handle, lambda s: s.get("voucher_signed") and s.get("order_state") == "Created"
    )
    assert "released" not in status["trace"], status
    assert status["approved_by"] == ""
    assert status["decision_artifact"] == {}

    foreign = approval_for(order_id, stable_id="sid-other")
    await handle.signal(wf.BridgeOrderA.approve, foreign)
    try:
        await handle.result()
    except WorkflowFailureError as exc:
        assert_typed(exc, "Decision")
    else:
        raise AssertionError("未绑定本单的审批应当失败")

    status = await handle.query(wf.BridgeOrderA.status)
    assert "released" not in status["trace"], status
    assert status["order_state"] == "Created", status
    assert status["decision_artifact"] == {}


async def test_approved_release_proceeds(client: Client) -> None:
    order_id = "approved-" + uuid.uuid4().hex
    handle = await start(client, order_id)
    await handle.signal(wf.BridgeOrderA.approve, approval_for(order_id))
    result = await handle.result()
    assert result["final_state"] == "Completed", result
    assert "released" in result["trace"], result
    history = result["history"]
    assert {"from": "Created", "to": "Released"} in history, history


async def test_approver_role_is_recorded_on_artifact(client: Client) -> None:
    order_id = "role-" + uuid.uuid4().hex
    handle = await start(client, order_id)
    await handle.signal(wf.BridgeOrderA.approve, approval_for(order_id, role="Approver"))
    result = await handle.result()
    artifact = result["decision_artifact"]
    assert artifact["role"] == "Approver", artifact
    assert artifact["actor_id"] == "user-approver", artifact
    assert result["approver_role"] == "Approver"
    assert result["approved_by"] == "user-approver"
    assert "released" in result["trace"], result


async def main() -> None:
    await test_trace_reads_order_state_and_gate_missing()
    print("trace keys ok", flush=True)
    async with await WorkflowEnvironment.start_local(
        dev_server_existing_path=TEMPORAL_CLI,
        ui=False,
        dev_server_log_level="error",
    ) as env:
        async with Worker(
            env.client,
            task_queue=wf.TASK_QUEUE,
            workflows=[wf.BridgeOrderA],
            activities=[wf.sign_voucher, wf.forge_artifact, wf.engine_call],
        ):
            await test_unapproved_release_is_rejected(env.client)
            print("unapproved release rejected", flush=True)
            await test_approver_role_is_recorded_on_artifact(env.client)
            print("approver role recorded", flush=True)
            await test_approved_release_proceeds(env.client)
            print("approved release proceeded", flush=True)
    print("ok", flush=True)


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except Exception:
        sys.stderr.write("approval gate integration failed\n")
        raise

"""形态A「薄壳」：Python workflow 只做编排（审批门/步骤推进/CAN/timer），
每次状态跃迁与订单变更都调用 Rust 判定引擎（af-workorder 门控的独立 bin，stdin/stdout JSON）。

订单模型的唯一权威在 Rust（af-workorder）：workflow 里的 `order` 字段是引擎回传的
JSON 快照，Python 只透传、只读 `state` 字段决定"下一步做什么"；"能不能做"全部问引擎。
"""
import asyncio
import datetime
import json
import os
import subprocess
from dataclasses import dataclass, field
from typing import Any, Dict, List

import temporalio.activity
import temporalio.workflow
from temporalio.common import RetryPolicy
from temporalio.exceptions import ApplicationError

import ledger

TASK_QUEUE = "bridge-queue-a"
ACT_TIMEOUT = datetime.timedelta(seconds=60)
ENGINE_BIN = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "target", "debug", "af-engine"
)
ENGINE_TRACE = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "data", "engine_calls.log"
)


# ---------- activities：外部调用端 ----------

@temporalio.activity.defn
async def sign_voucher(order_id: str) -> Dict[str, Any]:
    return ledger.sign_voucher(order_id)


@temporalio.activity.defn
async def forge_artifact(order_id: str) -> Dict[str, Any]:
    for i in range(8):  # 模拟 8s 外部产出；逐秒心跳（activity 默认心跳纪律）
        await asyncio.sleep(1)
        temporalio.activity.heartbeat(f"{i + 1}/8")
    return ledger.forge_artifact(order_id)


@temporalio.activity.defn
async def engine_call(req: Dict[str, Any]) -> Dict[str, Any]:
    """每次引擎调用 = 独立子进程：请求 JSON 进、应答 JSON 出，引擎无状态。"""
    proc = subprocess.run(
        [ENGINE_BIN], input=json.dumps(req), capture_output=True, text=True, timeout=30
    )
    if proc.returncode != 0:
        raise ApplicationError(f"engine crashed rc={proc.returncode}: {proc.stderr[:300]}")
    resp = json.loads(proc.stdout)
    with open(ENGINE_TRACE, "a", encoding="utf-8") as f:  # 观测轨迹：哪个版本引擎判了什么
        f.write(json.dumps({
            "t": datetime.datetime.now().isoformat(timespec="milliseconds"),
            "tag": req.get("tag"), "op": req.get("op"), "transition": req.get("transition"),
            "ok": resp.get("ok"), "engine_version": resp.get("engine_version"),
            "state_after": resp.get("state"),
            "missing": (resp.get("error") or {}).get("detail", {}).get("PreconditionsNotMet", {}).get("missing"),
        }, ensure_ascii=False) + "\n")
    return resp


# ---------- workflow 状态（编排层簿记 + 引擎回传的订单快照） ----------

@dataclass
class StateA:
    order_id: str = ""
    with_deadline: bool = False   # E3 构造用：新前置条件（release 需 SLA deadline）下谁会被拒
    use_can: bool = False         # E4 构造用：release 后 continue-as-new
    phase: int = 1                # CAN 后置 2
    runs_done: int = 0
    order: Dict[str, Any] = field(default_factory=dict)  # Rust 拥有的订单 JSON 快照（透传）
    approved_by: str = ""
    voucher_signed: bool = False
    artifact_forged: bool = False
    trace: List[str] = field(default_factory=list)


def _engine_options() -> Dict[str, Any]:
    return dict(
        start_to_close_timeout=ACT_TIMEOUT,
        retry_policy=RetryPolicy(initial_interval=datetime.timedelta(seconds=1),
                                 backoff_coefficient=2.0, maximum_attempts=5),
    )


def _long_activity_options() -> Dict[str, Any]:
    """长活动：必须带 heartbeat_timeout，否则 worker 被杀后 server 只能等满 StartToClose。"""
    return dict(
        start_to_close_timeout=ACT_TIMEOUT,
        heartbeat_timeout=datetime.timedelta(seconds=5),
        retry_policy=RetryPolicy(initial_interval=datetime.timedelta(seconds=1),
                                 backoff_coefficient=2.0, maximum_attempts=5),
    )


@temporalio.workflow.defn
class BridgeOrderA:
    def __init__(self) -> None:
        self._s = StateA()

    async def _engine(self, op: str, **kw: Any) -> Dict[str, Any]:
        req = {"op": op, "order": self._s.order, **kw}
        resp = await temporalio.workflow.execute_activity(engine_call, args=[req], **_engine_options())
        if not resp.get("ok"):
            err = resp.get("error", {})
            raise ApplicationError(
                f"engine rejected {op}: {err.get('message')}", resp,
                non_retryable=True, type=err.get("kind") or "EngineRejected",
            )
        self._s.order = resp["order"]
        return resp

    async def _engine_transition(self, t: str) -> None:
        await self._engine(t)

    @temporalio.workflow.run
    async def run(self, s: StateA) -> Dict[str, Any]:
        self._s = s
        if not s.order:  # 首个 run：拼初始快照（字段集合 = af-workorder::WorkOrder 的 serde 形态）
            from identity import stable_id_for  # stable_id 确定性派生（与 Rust 侧同规则）
            s.order = {
                "id": s.order_id, "stable_id": stable_id_for(s.order_id), "run_id": "",
                "state": "Created", "graph_version": "g-bridge",
                "line": None, "budget_voucher": None, "artifacts": [],
                "sla": {"deadline": ("2026-12-31T00:00:00Z" if s.with_deadline else None),
                        "priority": "High"},
                "supersedes": None, "history": [],
            }
            s.trace.append("started")

        # 登记产线绑定（release 前置①；无外部依赖，直接问引擎登记）
        if s.order.get("line") is None:
            await self._engine("bind_line", line={"line_id": "line-A", "line_version": "lv-7"})
            s.trace.append("line_bound")

        # activity A1：外部预算账本签发凭证（幂等）
        if not s.voucher_signed:
            v = await temporalio.workflow.execute_activity(sign_voucher, args=[s.order_id], **_engine_options())
            await self._engine("attach_voucher", voucher=v)
            s.voucher_signed = True
            s.trace.append("voucher_signed")

        # 审批门（signal 放行）
        if not s.approved_by:
            await temporalio.workflow.wait_condition(lambda: bool(s.approved_by))
            s.trace.append(f"approved_by={s.approved_by}")

        # 门控跃迁：release —— 本流程唯一的门控前置条件点，判定在 Rust 引擎
        if s.order.get("state") == "Created":
            await self._engine_transition("release")
            s.trace.append("released")

        # 跃迁 start（Released → InProgress，无门控；工件只许在 InProgress/Completed 挂载）
        if s.order.get("state") == "Released":
            await self._engine_transition("start")
            s.trace.append("started_work")

        # E4：CAN 续接点（release 后把订单快照随身带进新 run）
        if s.use_can and s.phase == 1:
            s.phase = 2
            s.runs_done += 1
            s.trace.append("continue_as_new")
            temporalio.workflow.continue_as_new(s)

        # activity A2：产线产出工件（长活动、心跳）
        if not s.artifact_forged:
            a = await temporalio.workflow.execute_activity(forge_artifact, args=[s.order_id], **_long_activity_options())
            await self._engine("attach_artifact", artifact=a)
            s.artifact_forged = True
            s.trace.append("artifact_forged")

        # 收尾跃迁：complete（无门控）
        if s.order.get("state") == "InProgress":
            await self._engine_transition("complete")
            s.trace.append("completed")

        return {
            "order_id": s.order_id, "final_state": s.order.get("state"),
            "history": s.order.get("history"), "trace": s.trace, "runs_done": s.runs_done,
        }

    @temporalio.workflow.signal
    def approve(self, approver: str) -> None:
        if not self._s.approved_by:
            self._s.approved_by = f"signal:{approver}"

    @temporalio.workflow.query
    def status(self) -> Dict[str, Any]:
        return {
            "order_id": self._s.order_id, "order_state": self._s.order.get("state"),
            "phase": self._s.phase, "approved_by": self._s.approved_by,
            "voucher_signed": self._s.voucher_signed, "artifact_forged": self._s.artifact_forged,
            "trace": list(self._s.trace), "run_id": temporalio.workflow.info().run_id,
            "replaying": temporalio.workflow.unsafe.is_replaying(),
        }

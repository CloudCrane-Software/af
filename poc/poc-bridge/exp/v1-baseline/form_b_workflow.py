"""形态B「镜像」：把 af-workorder 的状态机+门控直接翻译进 Python workflow 代码，
Rust 不参与运行时。

翻译对照（af-workorder → 本文件）：
  state.rs  OrderState/Transition/allowed_transitions → ORDER 字符串枚举 + ALLOWED/TO_STATE 表
  order.rs:189-202 release_missing                    → release_missing()
  order.rs:205-211 close_missing                      → close_missing()
  order.rs:148-183  bind_line/attach_voucher/attach_artifact 的状态守卫 → 同名函数
  order.rs:216-236 transition（先查边再查门控）         → transition()
  error.rs  GateError 三分法                           → GateFailure(kind, detail)
跨 run 状态全部走 StateB（CAN 参数类型必须与 run 签名一致，poc-temporal 踩坑⑤）。
"""
import asyncio
import datetime
import json
from dataclasses import dataclass, field
from typing import Any, Dict, List, NoReturn, Optional

import temporalio.activity
import temporalio.workflow
from temporalio.common import RetryPolicy
from temporalio.exceptions import ApplicationError

import ledger

TASK_QUEUE = "bridge-queue-b"
ACT_TIMEOUT = datetime.timedelta(seconds=60)


# ---------- activities（与形态A完全相同的外部系统调用端） ----------

@temporalio.activity.defn
async def sign_voucher(order_id: str) -> Dict[str, Any]:
    return ledger.sign_voucher(order_id)


@temporalio.activity.defn
async def forge_artifact(order_id: str) -> Dict[str, Any]:
    for i in range(8):
        await asyncio.sleep(1)
        temporalio.activity.heartbeat(f"{i + 1}/8")
    return ledger.forge_artifact(order_id)


# ---------- 状态机翻译（af-workorder::state） ----------

ALLOWED = {
    "Created": ("Release", "Cancel"),
    "Released": ("Start", "Cancel"),
    "InProgress": ("Complete", "Cancel"),
    "Completed": ("Close",),
    "Closed": (),
    "Cancelled": (),
}
TO_STATE = {
    "Release": "Released", "Start": "InProgress", "Complete": "Completed",
    "Close": "Closed", "Cancel": "Cancelled",
}


def can_apply(o: "OrderB", t: str) -> bool:
    if t == "Cancel":
        return o.state in ("Created", "Released", "InProgress")
    return t in ALLOWED[o.state]


# ---------- 订单模型翻译（af-workorder::order） ----------

@dataclass
class LineBinding:
    line_id: str
    line_version: str


@dataclass
class BudgetVoucher:
    voucher_id: str
    amount: int
    signed: bool


@dataclass
class WorkArtifact:
    artifact_id: str
    is_final: bool
    qualified: bool


@dataclass
class Sla:
    deadline: Optional[str]
    priority: str


@dataclass
class HistoryRecord:
    from_state: str
    to_state: str


@dataclass
class OrderB:
    """翻译自 af-workorder::WorkOrder（graph_version 不可变约定同样成立：无人写它）。"""
    id: str
    state: str = "Created"
    graph_version: str = "g-bridge"
    line: Optional[LineBinding] = None
    budget_voucher: Optional[BudgetVoucher] = None
    artifacts: List[WorkArtifact] = field(default_factory=list)
    sla: Optional[Sla] = None
    supersedes: Optional[str] = None
    history: List[HistoryRecord] = field(default_factory=list)


class GateFailure(Exception):
    """对齐 GateError 三分法：IllegalTransition / PreconditionsNotMet / MutationForbidden。"""

    def __init__(self, kind: str, detail: Dict[str, Any]) -> None:
        super().__init__(f"{kind}: {json.dumps(detail, ensure_ascii=False)}")
        self.kind = kind
        self.detail = detail


def release_missing(o: OrderB) -> List[Any]:  # order.rs:189-202 直译
    m: List[Any] = []
    if o.line is None:
        m.append("LineVersionNotBound")
    if o.budget_voucher is None:
        m.append("BudgetVoucherNotIssued")
    elif not o.budget_voucher.signed:
        m.append({"BudgetVoucherNotSigned": {"voucher_id": o.budget_voucher.voucher_id}})
    return m


def close_missing(o: OrderB) -> List[str]:  # order.rs:205-211 直译
    return [] if any(a.is_final and a.qualified for a in o.artifacts) else ["NoQualifiedFinalArtifact"]


def transition(o: OrderB, t: str) -> OrderB:  # order.rs:216-236 直译：先查边、再查门控
    if not can_apply(o, t):
        raise GateFailure("IllegalTransition",
                          {"from": o.state, "to": TO_STATE[t], "allowed": list(ALLOWED[o.state])})
    if t == "Release":
        missing = release_missing(o)
    elif t == "Close":
        missing = close_missing(o)
    else:
        missing = []
    if missing:
        raise GateFailure("PreconditionsNotMet", {"transition": t, "missing": missing})
    o.history.append(HistoryRecord(from_state=o.state, to_state=TO_STATE[t]))
    o.state = TO_STATE[t]
    return o


def bind_line(o: OrderB, line: LineBinding) -> OrderB:
    if o.state != "Created":
        raise GateFailure("MutationForbidden", {"state": o.state, "op": "bind_line"})
    o.line = line
    return o


def attach_voucher(o: OrderB, v: BudgetVoucher) -> OrderB:
    if o.state != "Created":
        raise GateFailure("MutationForbidden", {"state": o.state, "op": "attach_voucher"})
    o.budget_voucher = v
    return o


def attach_artifact(o: OrderB, a: WorkArtifact) -> OrderB:
    if o.state not in ("InProgress", "Completed"):
        raise GateFailure("MutationForbidden", {"state": o.state, "op": "attach_artifact"})
    o.artifacts.append(a)
    return o


# ---------- workflow ----------

@dataclass
class StateB:
    order_id: str = ""
    with_deadline: bool = False
    use_can: bool = False
    phase: int = 1
    runs_done: int = 0
    order: OrderB = field(default_factory=lambda: OrderB(id=""))
    approved_by: str = ""
    voucher_signed: bool = False
    artifact_forged: bool = False
    trace: List[str] = field(default_factory=list)


def _activity_options() -> Dict[str, Any]:
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


def _gate_fail(e: GateFailure) -> NoReturn:
    raise ApplicationError(f"gate rejected: {e}", e.detail, non_retryable=True, type=e.kind)


@temporalio.workflow.defn
class BridgeOrderB:
    def __init__(self) -> None:
        self._s = StateB()

    @temporalio.workflow.run
    async def run(self, s: StateB) -> Dict[str, Any]:
        self._s = s
        if not s.order.id:
            s.order = OrderB(
                id=s.order_id, graph_version="g-bridge",
                sla=Sla(deadline=("2026-12-31T00:00:00Z" if s.with_deadline else None), priority="High"),
            )
            s.trace.append("started")

        try:
            # 登记产线绑定（release 前置①）
            if s.order.line is None:
                bind_line(s.order, LineBinding(line_id="line-A", line_version="lv-7"))
                s.trace.append("line_bound")

            # activity A1：外部预算账本签发凭证（幂等）
            if not s.voucher_signed:
                v = await temporalio.workflow.execute_activity(
                    sign_voucher, args=[s.order_id], **_activity_options())
                attach_voucher(s.order, BudgetVoucher(**v))
                s.voucher_signed = True
                s.trace.append("voucher_signed")

            # 审批门（signal 放行）
            if not s.approved_by:
                await temporalio.workflow.wait_condition(lambda: bool(s.approved_by))
                s.trace.append(f"approved_by={s.approved_by}")

            # 门控跃迁：release —— 判定在本文件 Python 代码里
            if s.order.state == "Created":
                transition(s.order, "Release")
                s.trace.append("released")

            # 跃迁 start（Released → InProgress，无门控；工件只许在 InProgress/Completed 挂载）
            if s.order.state == "Released":
                transition(s.order, "Start")
                s.trace.append("started_work")

            # E4：CAN 续接点
            if s.use_can and s.phase == 1:
                s.phase = 2
                s.runs_done += 1
                s.trace.append("continue_as_new")
                temporalio.workflow.continue_as_new(s)

            # activity A2：产线产出工件（长活动、心跳）
            if not s.artifact_forged:
                a = await temporalio.workflow.execute_activity(
                    forge_artifact, args=[s.order_id], **_long_activity_options())
                attach_artifact(s.order, WorkArtifact(**a))
                s.artifact_forged = True
                s.trace.append("artifact_forged")

            # 收尾跃迁：complete（无门控）
            if s.order.state == "InProgress":
                transition(s.order, "Complete")
                s.trace.append("completed")
        except GateFailure as e:
            _gate_fail(e)

        return {
            "order_id": s.order_id, "final_state": s.order.state,
            "history": [{"from": h.from_state, "to": h.to_state} for h in s.order.history],
            "trace": s.trace, "runs_done": s.runs_done,
        }

    @temporalio.workflow.signal
    def approve(self, approver: str) -> None:
        if not self._s.approved_by:
            self._s.approved_by = f"signal:{approver}"

    @temporalio.workflow.query
    def status(self) -> Dict[str, Any]:
        return {
            "order_id": self._s.order_id, "order_state": self._s.order.state,
            "phase": self._s.phase, "approved_by": self._s.approved_by,
            "voucher_signed": self._s.voucher_signed, "artifact_forged": self._s.artifact_forged,
            "trace": list(self._s.trace), "run_id": temporalio.workflow.info().run_id,
            "replaying": temporalio.workflow.unsafe.is_replaying(),
        }

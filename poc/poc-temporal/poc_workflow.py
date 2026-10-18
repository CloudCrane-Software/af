"""订单工作流 PoC：signal/Update 审批门 + 外部预算账本（幂等）+ child fan-out + continue-as-new。

状态机（每次 run）：
  awaiting_approval --signal/update approve--> reserved --children fan-out-->
  [counter>=阈值 -> continue-as-new, 同一 workflow id 新 run] --> awaiting_settle
  --signal settle--> committed(完成)

纪律：所有跨 run 状态都走 OrderState（CAN 的参数类型必须与 run 签名一致）；
run 内易失标志只用于"本 run 语义"（settle 门）。
"""
import asyncio
import datetime
from dataclasses import dataclass, field
from typing import Any, Dict, List

import temporalio.activity
import temporalio.workflow

import ledger

TASK_QUEUE = "poc-order-queue"
OPS_THRESHOLD = 6  # 计数器（业务操作数）到阈值即 continue-as-new
ACT_TIMEOUT = datetime.timedelta(seconds=30)


# ---------- activities（模拟外部系统的调用端） ----------

@temporalio.activity.defn
async def reserve_budget(order_id: str, amount: float) -> Dict[str, Any]:
    return ledger.reserve(order_id, amount)


@temporalio.activity.defn
async def commit_budget_deduct(order_id: str, reservation_id: str) -> Dict[str, Any]:
    return ledger.commit(order_id, reservation_id)


@temporalio.activity.defn
async def fulfill_item(order_id: str, index: int, item: str) -> str:
    await asyncio.sleep(0.05)
    return f"fulfilled:{order_id}#{index}:{item}"


# ---------- 子工作流：单个子任务 ----------

@dataclass
class ChildResult:
    index: int
    item: str
    detail: str


@temporalio.workflow.defn
class FulfillItemWorkflow:
    @temporalio.workflow.run
    async def run(self, order_id: str, index: int, item: str) -> ChildResult:
        detail = await temporalio.workflow.execute_activity(
            fulfill_item, args=[order_id, index, item], start_to_close_timeout=ACT_TIMEOUT
        )
        return ChildResult(index=index, item=item, detail=detail)


# ---------- 订单工作流 ----------

@dataclass
class OrderState:
    """工作流唯一状态：既是 input 也是 continue-as-new 的携带载体（类型必须与 run 签名一致）"""

    order_id: str = ""
    amount: float = 0.0
    items: List[str] = field(default_factory=list)
    approved_by: str = ""
    reservation_id: str = ""
    child_results: List[str] = field(default_factory=list)
    ops: List[str] = field(default_factory=list)  # 计数器 = len(ops)
    runs_done: int = 0  # 本 workflow id 已经 continue-as-new 过几次
    receipt: str = ""
    balance_after: float = 0.0


@temporalio.workflow.defn
class OrderWorkflow:
    def __init__(self) -> None:
        self._state = OrderState()
        self._settled = False  # run 内标志：settle 门只属于当前 run（CAN 后需重新 settle）

    @temporalio.workflow.run
    async def run(self, s: OrderState) -> Dict[str, Any]:
        self._state = s  # 首个 run：client 传入的初始状态；CAN 后的 run：携带状态
        if not s.ops:
            s.ops.append("started")

        # ① 人工审批等待：signal 或 Update 任一到达即可放行（门开在携带状态上，CAN 后不重审）
        if not s.approved_by:
            await temporalio.workflow.wait_condition(lambda: bool(s.approved_by))
            s.ops.append(f"approved_by={s.approved_by}")

        # ② 外部预算账本：预占（幂等；已预占则跳过）
        if not s.reservation_id:
            r = await temporalio.workflow.execute_activity(
                reserve_budget, args=[s.order_id, s.amount], start_to_close_timeout=ACT_TIMEOUT
            )
            s.reservation_id = str(r["reservation_id"])
            s.ops.append("reserved")

        # ③ child workflow fan-out：map 5 个子任务（只做一次）
        if len(s.child_results) < len(s.items):
            handles = [
                await temporalio.workflow.start_child_workflow(
                    FulfillItemWorkflow.run,
                    args=[s.order_id, i, item],
                    id=f"{s.order_id}-fulfill-{i}",
                )
                for i, item in enumerate(s.items)
            ]
            results = await asyncio.gather(*handles)
            s.child_results = [f"{r.index}:{r.detail}" for r in results]
            for r in results:  # 每个子任务计入计数器
                s.ops.append(f"child{r.index}:{r.detail}")

        # ④ 计数器到阈值 -> continue-as-new（同一 workflow id 换新 run，状态随身携带）
        if len(s.ops) >= OPS_THRESHOLD and s.runs_done < 1:
            s.runs_done += 1
            s.ops.append(f"continue_as_new(counter={len(s.ops)})")
            temporalio.workflow.continue_as_new(s)  # raises ContinueAsNewError

        # ⑤ settle 门（run 内语义：每个 run 都要等一次 settle，留出 CAN 后的观察窗口）
        if not self._settled:
            await temporalio.workflow.wait_condition(lambda: self._settled)
            s.ops.append("settled")

        # ⑥ 幂等扣减（activity 首次调用"扣款成功但抛错"，Temporal 重试后账本去重，只扣一次）
        rec = await temporalio.workflow.execute_activity(
            commit_budget_deduct, args=[s.order_id, s.reservation_id],
            start_to_close_timeout=ACT_TIMEOUT,
        )
        s.receipt = str(rec["receipt"])
        s.balance_after = float(rec["balance_after"])
        s.ops.append("committed")
        return {
            "order_id": s.order_id,
            "receipt": s.receipt,
            "balance_after": s.balance_after,
            "counter": len(s.ops),
            "ops": list(s.ops),
            "runs_done": s.runs_done,
            "children": list(s.child_results),
        }

    # --- signal：异步通知，无返回值 ---
    @temporalio.workflow.signal
    def approve(self, approver: str) -> None:
        if not self._state.approved_by:
            self._state.approved_by = f"signal:{approver}"

    @temporalio.workflow.signal
    def settle(self) -> None:
        self._settled = True

    # --- update：同步调用，有返回值（审批的另一种实现） ---
    @temporalio.workflow.update
    def approve_via_update(self, approver: str) -> str:
        if not self._state.approved_by:
            self._state.approved_by = f"update:{approver}"
        return f"approved-by-update:{approver}"

    # --- query：只读 ---
    @temporalio.workflow.query
    def status(self) -> Dict[str, Any]:
        return {
            "order_id": self._state.order_id,
            "approved_by": self._state.approved_by,
            "reservation_id": self._state.reservation_id,
            "children_done": len(self._state.child_results),
            "counter": len(self._state.ops),
            "ops": list(self._state.ops),
            "runs_done": self._state.runs_done,
            "workflow_run_id": temporalio.workflow.info().run_id,
            "settled": self._settled,
        }

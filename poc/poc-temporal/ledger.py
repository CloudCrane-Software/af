"""模拟外部预算账本：文件持久化（独立于 Temporal / worker 进程），预占 + 幂等扣减。

外部系统语义：
- reserve(order_id, amount)：按 order_id 幂等，重复预占返回同一个 reservation_id。
- commit(order_id, reservation_id)：按 order_id 幂等，只扣一次钱；
  第一次调用"扣款成功但响应丢失"（模拟 crash-after-effect），Temporal 重试后第二次
  调用发现已扣过 → 返回相同回执，不二次扣款。
"""
import json
import os
import threading
import uuid

LEDGER_PATH = os.environ.get("POC_LEDGER_PATH", "/tmp/poc-budget-ledger.json")
INITIAL_BALANCE = 10_000.0
_lock = threading.Lock()


def _load() -> dict:
    if os.path.exists(LEDGER_PATH):
        with open(LEDGER_PATH, "r", encoding="utf-8") as f:
            return json.load(f)
    return {
        "balance": INITIAL_BALANCE,
        "reservations": {},  # order_id -> {reservation_id, amount, status, commit_attempts}
        "ops_log": [],
    }


def _save(state: dict) -> None:
    with open(LEDGER_PATH, "w", encoding="utf-8") as f:
        json.dump(state, f, ensure_ascii=False, indent=1)


def reserve(order_id: str, amount: float) -> dict:
    with _lock:
        s = _load()
        r = s["reservations"].get(order_id)
        if r is not None:
            s["ops_log"].append(f"reserve:dedup order={order_id}")
            _save(s)
            return {"reservation_id": r["reservation_id"], "dedup": True}
        rid = "rsv-" + uuid.uuid4().hex[:12]
        s["reservations"][order_id] = {
            "reservation_id": rid,
            "amount": amount,
            "status": "reserved",
            "commit_attempts": 0,
        }
        s["ops_log"].append(f"reserve:ok order={order_id} amount={amount}")
        _save(s)
        return {"reservation_id": rid, "dedup": False}


def commit(order_id: str, reservation_id: str) -> dict:
    with _lock:
        s = _load()
        r = s["reservations"].get(order_id)
        if r is None or r["reservation_id"] != reservation_id:
            raise ValueError(f"unknown reservation order={order_id} rsv={reservation_id}")
        r["commit_attempts"] += 1
        if r["status"] == "committed":
            # 已扣过：幂等返回，不再扣款
            s["ops_log"].append(f"commit:dedup order={order_id} attempts={r['commit_attempts']}")
            _save(s)
            return {
                "receipt": r["receipt"],
                "balance_after": s["balance"],
                "already_deducted": True,
                "commit_attempts": r["commit_attempts"],
            }
        # 真实扣款
        s["balance"] -= r["amount"]
        r["status"] = "committed"
        r["receipt"] = "rcpt-" + uuid.uuid4().hex[:12]
        s["ops_log"].append(
            f"commit:deduct order={order_id} amount={r['amount']} balance={s['balance']}"
        )
        _save(s)
        first = r["commit_attempts"] == 1
    if first:
        # 模拟"扣款已落库但响应丢失"：第一次调用抛错，逼 Temporal 重试
        raise RuntimeError("simulated ledger timeout after commit (response lost)")
    return {
        "receipt": r["receipt"],
        "balance_after": s["balance"],
        "already_deducted": False,
        "commit_attempts": r["commit_attempts"],
    }


def snapshot() -> dict:
    with _lock:
        return _load()

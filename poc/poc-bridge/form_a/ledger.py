"""外部系统模拟（形态A共用）：预算账本签发凭证 + 产线产出工件。

独立于 Temporal/worker 的 JSON 文件持久化，attempts 计数用于证明 activity 的
at-least-once 重试与业务幂等。与形态B的 ledger.py 代码相同，仅数据文件不同。
"""
import json
import os
import threading
import time
from typing import Any, Dict

HERE = os.path.dirname(os.path.abspath(__file__))
LEDGER_PATH = os.path.join(HERE, "..", "data", "ledger-a.json")
_LOCK = threading.Lock()


def _load() -> Dict[str, Any]:
    if os.path.exists(LEDGER_PATH):
        with open(LEDGER_PATH, encoding="utf-8") as f:
            return json.load(f)
    return {"vouchers": {}, "artifacts": {}, "ops_log": []}


def _save(d: Dict[str, Any]) -> None:
    os.makedirs(os.path.dirname(LEDGER_PATH), exist_ok=True)
    with open(LEDGER_PATH, "w", encoding="utf-8") as f:
        json.dump(d, f, ensure_ascii=False, indent=1)


def sign_voucher(order_id: str) -> Dict[str, Any]:
    """外部预算账本：为订单签发预算凭证（幂等：同单第二次直接返回已签凭证）。"""
    with _LOCK:
        d = _load()
        rec = d["vouchers"].setdefault(
            order_id, {"voucher_id": f"bv-{order_id}", "amount": 500, "signed": True, "attempts": 0}
        )
        rec["attempts"] += 1
        d["ops_log"].append(f"sign_voucher order={order_id} attempt={rec['attempts']}")
        _save(d)
        return {"voucher_id": rec["voucher_id"], "amount": rec["amount"], "signed": rec["signed"]}


def forge_artifact(order_id: str) -> Dict[str, Any]:
    """产线：产出合格终态工件（幂等）。耗时由 activity 侧重试循环模拟，这里只登记结果。"""
    with _LOCK:
        d = _load()
        rec = d["artifacts"].setdefault(
            order_id, {"artifact_id": f"fa-{order_id}", "is_final": True, "qualified": True, "attempts": 0}
        )
        rec["attempts"] += 1
        d["ops_log"].append(f"forge_artifact order={order_id} attempt={rec['attempts']} t={time.strftime('%H:%M:%S')}")
        _save(d)
        return {"artifact_id": rec["artifact_id"], "is_final": rec["is_final"], "qualified": rec["qualified"]}

"""stable_id 的 Python 侧确定性派生，与 af-workorder StableId::generate 同规则。
域前缀 af-workorder/stable-id/v1\0 || order_id → sha256 小写 hex，加 sid- 前缀。"""
import hashlib

_DOMAIN = b"af-workorder/stable-id/v1\0"

def stable_id_for(order_id: str) -> str:
    return "sid-" + hashlib.sha256(_DOMAIN + order_id.encode()).hexdigest()

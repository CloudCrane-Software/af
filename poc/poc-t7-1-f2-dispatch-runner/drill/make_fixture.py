#!/usr/bin/env python3
"""make_fixture — 造 F2 执行面**演练夹具**（本地文件面；**不是**合成 gitea 投递）。

用途：给 `ci/f2_dispatch.py ingest` 一份**形态与在役面一致**的输入
（注册表 + 已验信封 + 派单记录），以便在**隔离目录**里端到端演练
「派单 → 认领 → 跑门 → SUMMARY → 回报 → integrator 复核」。

★ 边界（诚实登记）：
* 本件**不联网、不发 webhook、不碰在役收件箱/门户/观测面**；
  夹具只落在调用方给的 `--out` 目录里（演练隔离目录）。
* 夹具内容是**合成的**（payload 为自造的 pull_request 事件），
  用途只有一件：让派单侧有输入可演。**它不构成任何真实门结论**。
* 夹具里的 `head.sha` / `base.sha` 必须指向**真实存在的提交**
  （演练用候选树的头），否则下游的绑定面无解。

用法：
  python3 -I make_fixture.py --out <隔离目录> --source-sha <40hex> --base-sha <40hex>
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path

CONTRACT = "af.trigger.v1"


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def atomic_write(path: Path, obj) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=1, sort_keys=True) + "\n",
                    encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description="造 F2 派单演练夹具（本地文件面）")
    parser.add_argument("--out", required=True, help="隔离目录（会在其下建 triggers/）")
    parser.add_argument("--source-sha", required=True, help="PR 头提交（真实存在）")
    parser.add_argument("--base-sha", required=True, help="PR base 提交（真实存在）")
    parser.add_argument("--repo", default="opchadmin/af")
    parser.add_argument("--number", type=int, default=4242)
    parser.add_argument("--branch", default="feature/f2-drill")
    parser.add_argument("--delivery-id", default="drill-f2-delivery-0001")
    parser.add_argument("--action", default="opened", choices=["opened", "synchronize"])
    parser.add_argument("--salt", default="", help="注册表 $comment 盐（改 recipe_digest ⇒ 改 order_id）")
    args = parser.parse_args()

    out = Path(args.out).expanduser()
    triggers = out / "triggers"
    stamp = datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")

    # ── 注册表（形态同在役运行位；recipe_digest 自算并写回，保证两侧同值）──
    registry = {
        "$comment": "F2 派单演练夹具注册表（合成件，仅演练用；非在役运行位）"
                    + (f" salt={args.salt}" if args.salt else ""),
        "schema": "af.trigger.registry.v1",
        "sources": [{
            "name": "gitea-webhook", "domain": "git", "key_id": "drill-key-1",
            "hmac": "sha256", "events": ["push", "pull_request"],
        }],
        "repo_suites": [{
            "repo_glob": "**",
            "map": {
                "push": {"suite": "F1"},
                "pull_request": {"suite": "F2", "actions": ["opened", "synchronize"]},
            },
        }],
        "liveness": {"inbox_silent_after_min": 240},
    }
    registry_text = json.dumps(registry, ensure_ascii=False, indent=1, sort_keys=True) + "\n"
    recipe_digest = sha256_hex(registry_text.encode("utf-8"))
    registry["recipe_digest"] = recipe_digest
    atomic_write(triggers / "registry.json", registry)

    # ── payload（合成 pull_request 事件）──
    payload = {
        "event": "pull_request",
        "action": args.action,
        "delivery_id": args.delivery_id,
        "repository": {"full_name": args.repo, "name": args.repo.split("/")[-1]},
        "pull_request": {
            "number": args.number,
            "head": {"sha": args.source_sha, "ref": args.branch},
            "base": {"sha": args.base_sha, "ref": "product/main"},
        },
    }
    raw = json.dumps(payload, ensure_ascii=False, sort_keys=True).encode("utf-8")
    payload_sha = sha256_hex(raw)

    # ── 已验信封（字段口径同 trigger-inbox.py 的 envelope_for）──
    thread_key = f"git:{args.repo}#{args.number}"
    dedup_key = f"pull_request:{args.repo}:{args.number}:{args.source_sha}"
    envelope = {
        "contract": CONTRACT,
        "event_id": f"gitea-webhook-{args.delivery_id}",
        "thread_key": thread_key,
        "payload_ref": f"inbox/gitea-webhook/{args.delivery_id}.json",
        "payload_sha256": payload_sha,
        "dedup_key": dedup_key,
        "source": "gitea-webhook",
        "received_at": stamp,
        "delivery_id": args.delivery_id,
        "event_type": "pull_request",
        "key_id": "drill-key-1",
        "hmac": "sha256",
        "biz_object": f"#{args.number}",
        "body": payload,
        "body_b64": base64.b64encode(raw).decode(),
    }
    atomic_write(triggers / "inbox" / "gitea-webhook" / f"{args.delivery_id}.json", envelope)

    # ── 在役派单记录（形态同 event-router.py 的 dispatch 件）──
    order_material = "|".join([args.repo, "F2", args.source_sha, recipe_digest])
    order_id = f"F2-{args.repo.replace('/', '-')}-{sha256_hex(order_material.encode())[:16]}"
    dispatch = {
        "order_id": order_id,
        "suite": "F2",
        "repo": args.repo,
        "subject": args.source_sha,
        "thread_key": thread_key,
        "dedup_key": dedup_key,
        "source": "gitea-webhook",
        "delivery_id": args.delivery_id,
        "event_type": "pull_request",
        "action": args.action,
        "recipe_digest": recipe_digest,
        "payload_sha256": payload_sha,
        "created_at": stamp,
        "status": "dispatch-record-only",
        "note": "演练夹具派单记录（合成件）：形态同在役 dispatch 件；真实在役面由 "
                "tools/event-router.py 消费腿写。",
    }
    atomic_write(triggers / "dispatch" / f"{order_id}.json", dispatch)

    print(json.dumps({
        "triggers_home": str(triggers),
        "order_id": order_id,
        "delivery_id": args.delivery_id,
        "source_sha": args.source_sha,
        "base_sha": args.base_sha,
        "recipe_digest": recipe_digest,
        "note": "合成夹具；不构成任何真实门结论",
    }, ensure_ascii=False, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""f2_dispatch — F2 全量门「收件箱派单」侧（v0.3-T7① F2 批新增件）

职责**只有一件**：把「在役收件箱派单面」已登记的 F2 订单，翻译成
**独立 runner 可领跑的 F2 任务件**，落进 runner 自有的队列目录。

────────────────────────────────────────────────────────────────────────────
边界（先读这一节）：

* 本件**不派 webhook、不收件、不跑门、不写合流面**。收件仍由
  `tools/trigger-inbox.py` 独占（在役面，禁碰）；派单登记仍由
  `tools/event-router.py` 的消费腿独占（在役面，禁碰）。
* 本件对在役面**只读**：`af-state/triggers/inbox/**`（已验信封）、
  `af-state/triggers/dispatch/**`（派单记录）、`af-state/triggers/registry.json`
  （注册表运行位）。**一个字节都不写**。
* 本件只写**runner 自有队列目录**（`--queue`，缺省 `$AF_F2_QUEUE`，
  再缺省 `$HOME/.local/state/af-f2`）——队列目录与在役收件箱**物理分离**，
  互不覆盖。
* 本件**不常驻**：一次性 CLI（`ingest` / `list` / `show`），无常驻守护、
  不起监听、不用 sudo。

────────────────────────────────────────────────────────────────────────────
派单契约（`af.f2.task.v1`）要点——正文见 `docs/F2-DISPATCH-RUNNER.md`：

* **派单消息的 schema**：复用 `af.trigger.v1` 七字段信封
  （`event_id`/`thread_key`/`payload_ref`/`payload_sha256`/`dedup_key`/
  `source`/`received_at`）＋ 外层 `delivery_id`/`event_type`/`body`；
  本件**不重造信封**，只做「已验信封 → F2 任务件」的翻译。
* **幂等键**：`idempotency_key = sha256(order_id|source_sha|base_ref|base_sha|
  recipe_digest)`；同内容必然同键 ⇒ 重投天然幂等（派单面第三道，
  前两道是收件箱 delivery 幂等 + 消费腿 dedup_key）。
* **去重**：`order_id` 沿用在役消费腿的**确定性生成公式**
  （`sha256(repo|suite|subject|recipe_digest)[:16]`）⇒ 本件算出的
  `order_id` 与在役派单记录**逐字同值**（不符即拒，不改写）。
* **认领唯一写者**：本件**只写队列任务件**，不领单；领单唯一写者是
  `ci/f2_runner.py`（flock 互斥 + 租约超时）。
* **租约与超时**：任务件带 `lease{runner_id,claimed_at,expires_at}`；
  过期才可被他人重领（`ci/f2_runner.py` 实现）。
* **失败重试与退避**：本件不重试（它只建单）；退避在 runner 侧执行
  （`attempts`/`next_attempt_at`/`backoff_sec`，指数退避 + 抖动）。

────────────────────────────────────────────────────────────────────────────
用法：

  # 建队列（从在役收件箱 + 在役派单记录）
  python3 -I ci/f2_dispatch.py ingest \
      --triggers-home /path/to/af-state/triggers --queue /path/to/f2-queue

  # 看队列
  python3 -I ci/f2_dispatch.py list --queue /path/to/f2-queue
  python3 -I ci/f2_dispatch.py show --queue /path/to/f2-queue --task-id <id>

退出码：0 = 成功（ingest 可以含「跳过」项，见输出）；1 = 红（拒绝/失败）；
      2 = 用法错或环境错。**任何非 0 都是红。**
"""

from __future__ import annotations

import argparse
import base64
import fnmatch
import hashlib
import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

AF_ROOT = Path(__file__).resolve().parent.parent
CONTRACT = "af.trigger.v1"
TASK_SCHEMA = "af.f2.task.v1"
SUITE = "F2"
GATE_ID = "f2-full"
GATE_COMMAND = "bash ci/run.sh"
# 宿主供给需求（缺件 ⇒ SKIP 不得计绿；见 docs/CI-BOUNDARY.md §一 与
# docs/F2-DISPATCH-RUNNER.md §三）：cosign / cargo-deny / advisory-db / netns
HOST_SUPPLY_REQUIREMENTS = ("cosign", "cargo-deny", "advisory-db", "netns")
# 合流目标唯一（docs/CI-BOUNDARY.md §五）：PR base 必须是 product/main
MERGE_TARGET_REF = "product/main"
DEFAULT_MAX_ATTEMPTS = 3


# ── 小工具 ────────────────────────────────────────────────────────────────────
def now_iso() -> str:
    return datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def short(sha: str, n: int = 12) -> str:
    return (sha or "")[:n]


def atomic_write_json(path: Path, obj: Any) -> None:
    """tmp + os.replace 原子写（承 trigger-inbox.py / event-router.py 同款语义），
    落盘后补父目录 fsync（rename 的持久性需要父目录 fsync）。"""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    with open(tmp, "w", encoding="utf-8") as handle:
        json.dump(obj, handle, ensure_ascii=False, indent=1, sort_keys=True)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(tmp, path)
    try:
        fd = os.open(str(path.parent), os.O_RDONLY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    except OSError:
        pass


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def default_queue_dir() -> Path:
    env = os.environ.get("AF_F2_QUEUE")
    if env:
        return Path(env).expanduser()
    return Path.home() / ".local" / "state" / "af-f2"


# ── 在役面只读适配 ────────────────────────────────────────────────────────────
def load_registry(reg_path: Path) -> tuple[dict, str]:
    """注册表（运行位优先；缺省落仓内正本）。返回 (注册表, recipe_digest)。

    recipe_digest 口径与在役消费腿一致：优先取注册表自带值，缺席则对原文字节现算。
    """
    raw = reg_path.read_text(encoding="utf-8")
    reg = json.loads(raw)
    if reg.get("schema") != "af.trigger.registry.v1":
        raise ValueError(f"注册表 schema 不识别: {str(reg.get('schema'))[:60]!r}")
    digest = reg.get("recipe_digest")
    if not (isinstance(digest, str) and len(digest) == 64):
        digest = sha256_hex(raw.encode("utf-8"))
    return reg, digest


def source_domain(reg: dict, source: str) -> str:
    for row in reg.get("sources", []):
        if row.get("name") == source:
            return row.get("domain") or "unknown"
    return "unknown"


def suite_for_event(reg: dict, repo: str, event_type: str, action: str | None) -> str | None:
    """注册表驱动的套件映射（多条命中取最窄 glob；pull_request 的 action
    不在 actions 列表 ⇒ 不派单）。口径与在役消费腿 `_suite_for_event` 同款。"""
    hits = []
    for row in reg.get("repo_suites", []):
        pattern = row.get("repo_glob", "**")
        if fnmatch.fnmatch(repo, pattern):
            hits.append((len(pattern), row))
    if not hits:
        return None
    row = max(hits, key=lambda item: item[0])[1]
    mapping = (row.get("map") or {}).get(event_type)
    if not isinstance(mapping, dict):
        return None
    if event_type == "pull_request":
        actions = mapping.get("actions") or []
        if action not in actions:
            return None
    suite = mapping.get("suite")
    return suite if isinstance(suite, str) and suite else None


def order_id_for(repo: str, suite: str, subject: str, recipe_digest: str) -> str:
    """确定性 order_id（与在役消费腿 `_trigger_order_id` **逐字同公式**）。"""
    digest = sha256_hex("|".join([repo, suite, subject, recipe_digest]).encode("utf-8"))
    return f"{suite}-{repo.replace('/', '-')}-{digest[:16]}"


def verify_envelope(env: dict, source: str, delivery_id: str, reg: dict) -> tuple[dict | None, list[str]]:
    """只读已验消息（承在役消费腿 `_trigger_verify_envelope` 同款口径）：
    唯一可信载荷 = `body_b64`（`payload_sha256` 自证）；派单决策输入一律从
    **重解析 body_b64** 取；信封冗余字段逐项与重解析核对，任一不符 = 拒读。

    返回 (facts, reasons)；facts 为 None ⇒ 拒读（不猜、不改）。
    """
    reasons: list[str] = []
    if env.get("contract") != CONTRACT:
        reasons.append(f"contract 不符: {str(env.get('contract'))[:40]!r}")
    b64 = env.get("body_b64")
    raw = None
    if not isinstance(b64, str) or not b64:
        reasons.append("body_b64 缺失/非字符串")
    else:
        try:
            raw = base64.b64decode(b64, validate=True)
        except Exception as error:  # noqa: BLE001 - 拒读即可，不带崩本轮
            reasons.append(f"body_b64 非法 base64: {type(error).__name__}")
    parsed = None
    if raw is not None:
        try:
            parsed = json.loads(raw.decode("utf-8"))
        except Exception as error:  # noqa: BLE001
            reasons.append(f"body_b64 非 UTF-8 JSON: {type(error).__name__}")
        if isinstance(parsed, dict) and env.get("payload_sha256") != sha256_hex(raw):
            reasons.append("payload_sha256 对 body_b64 复算不符（篡改痕迹）")
    if not isinstance(parsed, dict):
        reasons.append("body_b64 顶层不是 JSON 对象")
        return None, reasons
    if reasons:
        return None, reasons

    repo_obj = parsed.get("repository") or {}
    repo = str(repo_obj.get("full_name") or repo_obj.get("name") or "unknown-repo")
    etype = parsed.get("event")
    after = str(parsed.get("after") or parsed.get("sha") or "")
    pr = parsed.get("pull_request") or {}
    head = str((pr.get("head") or {}).get("sha") or parsed.get("head_sha") or "")
    number = pr.get("number") or parsed.get("number") or "?"
    action = str(parsed.get("action") or "") or None
    domain = source_domain(reg, source)
    if etype == "push":
        subject = after or delivery_id
        dedup_key = f"push:{repo}:{after}"
        thread_key = f"{domain}:{repo}@{short(after)}"
    elif etype == "pull_request":
        subject = head or delivery_id
        dedup_key = f"pull_request:{repo}:{number}:{head}"
        thread_key = f"{domain}:{repo}#{number}"
    else:
        subject = head or delivery_id
        dedup_key = f"{etype}:{repo}"
        thread_key = f"{domain}:{repo}"

    # 信封冗余字段逐项核对（被篡 = 拒读）
    if not isinstance(etype, str) or not etype:
        reasons.append("重解析 event 缺失/非字符串")
    elif env.get("event_type") != etype:
        reasons.append(f"event_type 信封={env.get('event_type')!r} ≠ 重解析={etype!r}")
    if env.get("delivery_id") != delivery_id:
        reasons.append(f"delivery_id 信封={env.get('delivery_id')!r} ≠ 文件名={delivery_id!r}")
    body_delivery = parsed.get("delivery_id")
    if isinstance(body_delivery, str) and body_delivery and body_delivery != delivery_id:
        reasons.append(f"delivery_id body={body_delivery!r} ≠ 文件名={delivery_id!r}")
    if env.get("dedup_key") != dedup_key:
        reasons.append(f"dedup_key 信封={str(env.get('dedup_key'))[:60]!r} ≠ 重推导={dedup_key!r}")
    if "body" in env and env.get("body") != parsed:
        reasons.append("冗余 parsed body 与 body_b64 重解析不一致（篡改痕迹）")
    if env.get("thread_key") != thread_key:
        reasons.append(f"thread_key 信封={str(env.get('thread_key'))[:60]!r} ≠ 重推导={thread_key!r}")
    if reasons:
        return None, reasons

    facts = {
        "source": source,
        "delivery_id": delivery_id,
        "event_type": etype,
        "action": action,
        "repo": repo,
        "subject": subject,
        "dedup_key": dedup_key,
        "thread_key": thread_key,
        "payload_sha256": env.get("payload_sha256"),
        "payload_ref": env.get("payload_ref"),
        "event_id": env.get("event_id"),
        "received_at": env.get("received_at"),
        "pull_request": pr,
    }
    return facts, []


def build_task(facts: dict, order_id: str, recipe_digest: str, base_ref: str,
               base_sha: str, source_sha: str, source_branch: str,
               max_attempts: int) -> dict:
    """任务件（`af.f2.task.v1`）。四元组输入齐备 ⇒ runner 可直接领跑。"""
    idem_material = "|".join([order_id, source_sha, base_ref, base_sha, recipe_digest])
    stamp = now_iso()
    return {
        "schema_version": TASK_SCHEMA,
        "task_id": order_id,
        "order_id": order_id,
        "idempotency_key": sha256_hex(idem_material.encode("utf-8")),
        "suite": SUITE,
        "gate_id": GATE_ID,
        "gate_command": GATE_COMMAND,
        "repo": facts["repo"],
        "remote": "origin",
        "base_ref": base_ref,
        "base_sha": base_sha,
        "source_sha": source_sha,
        "source_branch": source_branch,
        "subject": facts["subject"],
        "thread_key": facts["thread_key"],
        "dedup_key": facts["dedup_key"],
        "source": facts["source"],
        "delivery_id": facts["delivery_id"],
        "event_type": facts["event_type"],
        "action": facts["action"],
        "recipe_digest": recipe_digest,
        "payload_sha256": facts["payload_sha256"],
        "host_supply_requirements": list(HOST_SUPPLY_REQUIREMENTS),
        "status": "queued",
        "attempts": 0,
        "max_attempts": max_attempts,
        "next_attempt_at": stamp,
        "backoff_sec": 0,
        "lease": None,
        "created_at": stamp,
        "updated_at": stamp,
    }


# ── 子命令 ────────────────────────────────────────────────────────────────────
def cmd_ingest(args: argparse.Namespace) -> int:
    triggers_home = Path(args.triggers_home).expanduser()
    queue = Path(args.queue).expanduser()
    if not triggers_home.is_dir():
        print(f"f2_dispatch: 在役触发面目录不在位: {triggers_home}", file=sys.stderr)
        return 2
    registry_path = (Path(args.registry).expanduser() if args.registry
                     else triggers_home / "registry.json")
    if not registry_path.is_file():
        fallback = AF_ROOT / "poc" / "poc-super-agent" / "registry" / "trigger-registry.v1.json"
        if fallback.is_file():
            registry_path = fallback
        else:
            print(f"f2_dispatch: 注册表不在位: {registry_path}", file=sys.stderr)
            return 2
    try:
        reg, recipe_digest = load_registry(registry_path)
    except Exception as error:  # noqa: BLE001
        print(f"f2_dispatch: 注册表读失败: {type(error).__name__}: {error}", file=sys.stderr)
        return 2

    inbox_dir = triggers_home / "inbox"
    dispatch_dir = triggers_home / "dispatch"
    tasks_dir = queue / "tasks"
    tasks_dir.mkdir(parents=True, exist_ok=True)

    report: list[dict] = []
    counts = {"created": 0, "duplicate": 0, "skipped": 0, "conflict": 0}
    if inbox_dir.is_dir():
        envelope_paths = sorted(inbox_dir.glob("*/*.json"))
    else:
        envelope_paths = []
        print(f"f2_dispatch: WARN 在役收件箱目录不在位: {inbox_dir}（本轮无输入）", file=sys.stderr)

    for path in envelope_paths:
        source = path.parent.name
        delivery_id = path.stem
        try:
            env = read_json(path)
        except Exception as error:  # noqa: BLE001
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": f"信封不可读: {type(error).__name__}: {error}"})
            continue
        facts, reasons = verify_envelope(env, source, delivery_id, reg)
        if facts is None:
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": "拒读: " + "; ".join(reasons)})
            continue
        suite = suite_for_event(reg, facts["repo"], facts["event_type"], facts["action"])
        if suite is None:
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": "注册表无套件映射（收下不派单）"})
            continue
        if suite != SUITE:
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": f"套件={suite}，非本件职责（{SUITE}）"})
            continue

        pr = facts["pull_request"]
        head = pr.get("head") or {}
        base = pr.get("base") or {}
        source_sha = str(head.get("sha") or "")
        source_branch = str(head.get("ref") or "")
        base_sha = str(base.get("sha") or "")
        base_branch = str(base.get("ref") or "")
        if not source_sha or not re.fullmatch(r"[0-9a-f]{40}", source_sha):
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": "PR 头提交不可解析（不猜）"})
            continue
        if facts["subject"] != source_sha:
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": "subject ≠ PR 头 sha（与在役口径不符，拒）"})
            continue
        if base_branch != MERGE_TARGET_REF:
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": f"PR base={base_branch!r} 不是唯一合流目标 {MERGE_TARGET_REF!r}"
                                     "（不在位 F2 门覆盖面）"})
            continue
        if not re.fullmatch(r"[0-9a-f]{40}", base_sha):
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": "PR base 提交不可解析（不猜）"})
            continue
        base_ref = f"origin/{base_branch}"

        order_id = order_id_for(facts["repo"], suite, facts["subject"], recipe_digest)
        # 与在役派单记录对账（默认必须存在且逐项相符 ⇒ 不自造订单）
        dispatch_path = dispatch_dir / f"{order_id}.json"
        if dispatch_path.is_file():
            try:
                disp = read_json(dispatch_path)
            except Exception as error:  # noqa: BLE001
                counts["skipped"] += 1
                report.append({"delivery_id": delivery_id, "result": "skipped",
                               "reason": f"在役派单记录不可读: {type(error).__name__}"})
                continue
            mismatch = []
            for key, want in (("order_id", order_id), ("suite", suite), ("repo", facts["repo"]),
                              ("subject", facts["subject"]), ("dedup_key", facts["dedup_key"]),
                              ("delivery_id", delivery_id)):
                if disp.get(key) != want:
                    mismatch.append(f"{key}: 派单={disp.get(key)!r} 重算={want!r}")
            if mismatch:
                counts["skipped"] += 1
                report.append({"delivery_id": delivery_id, "result": "skipped",
                               "reason": "在役派单记录与重算不符: " + "; ".join(mismatch)})
                continue
        elif not args.allow_missing_dispatch_record:
            counts["skipped"] += 1
            report.append({"delivery_id": delivery_id, "result": "skipped",
                           "reason": f"在役派单记录不在位: {dispatch_path}"
                                     "（在役消费腿未登记 ⇒ 不自造订单）"})
            continue

        task = build_task(facts, order_id, recipe_digest, base_ref, base_sha,
                          source_sha, source_branch, args.max_attempts)
        task_path = tasks_dir / f"{task['task_id']}.json"
        if task_path.is_file():
            try:
                old = read_json(task_path)
            except Exception as error:  # noqa: BLE001
                counts["conflict"] += 1
                report.append({"delivery_id": delivery_id, "result": "conflict",
                               "reason": f"既有任务件不可读: {type(error).__name__}"})
                continue
            if old.get("idempotency_key") == task["idempotency_key"]:
                counts["duplicate"] += 1
                report.append({"delivery_id": delivery_id, "result": "duplicate",
                               "task_id": task["task_id"], "status": old.get("status"),
                               "reason": "同幂等键任务已在队列（重投天然幂等）"})
                continue
            counts["conflict"] += 1
            report.append({"delivery_id": delivery_id, "result": "conflict",
                           "task_id": task["task_id"],
                           "reason": "同 task_id 但幂等键不同（同单不同内容）⇒ 不覆盖，"
                                     "留给人工判读"})
            continue
        atomic_write_json(task_path, task)
        counts["created"] += 1
        report.append({"delivery_id": delivery_id, "result": "created",
                       "task_id": task["task_id"], "order_id": order_id,
                       "source_sha": source_sha, "base_sha": base_sha,
                       "idempotency_key": task["idempotency_key"],
                       "task_path": str(task_path)})

    payload = {
        "command": "ingest",
        "triggers_home": str(triggers_home),
        "registry": str(registry_path),
        "queue": str(queue),
        "recipe_digest": recipe_digest,
        "counts": counts,
        "items": report,
        "note": "在役面只读；本件只写 runner 自有队列目录。",
    }
    if args.json:
        print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print(f"F2 派单 ingest：新建 {counts['created']} / 幂等重复 {counts['duplicate']} / "
              f"跳过 {counts['skipped']} / 冲突 {counts['conflict']}"
              f"（队列={queue}）")
        for item in report:
            print(f"  [{item['result']}] {item.get('delivery_id')} "
                  f"{item.get('task_id', '')} {item.get('reason', '')}")
    return 0


def cmd_list(args: argparse.Namespace) -> int:
    queue = Path(args.queue).expanduser()
    tasks_dir = queue / "tasks"
    items: list[dict] = []
    if tasks_dir.is_dir():
        for path in sorted(tasks_dir.glob("*.json")):
            try:
                task = read_json(path)
            except Exception as error:  # noqa: BLE001
                items.append({"task_id": path.stem, "status": "unreadable",
                              "error": f"{type(error).__name__}: {error}"})
                continue
            items.append({
                "task_id": task.get("task_id"),
                "status": task.get("status"),
                "attempts": task.get("attempts"),
                "next_attempt_at": task.get("next_attempt_at"),
                "source_sha": task.get("source_sha"),
                "base_sha": task.get("base_sha"),
                "lease": task.get("lease"),
                "path": str(path),
            })
    payload = {"command": "list", "queue": str(queue), "count": len(items), "items": items}
    if args.json:
        print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print(f"F2 队列：{len(items)} 件（{queue}）")
        for item in items:
            lease = item.get("lease") or {}
            print(f"  {item.get('task_id')}  status={item.get('status')} "
                  f"attempts={item.get('attempts')} "
                  f"lease={lease.get('runner_id', '-')}@{lease.get('expires_at', '-')} "
                  f"source={str(item.get('source_sha'))[:12]}")
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    queue = Path(args.queue).expanduser()
    task_path = queue / "tasks" / f"{args.task_id}.json"
    result_path = queue / "results" / f"{args.task_id}.json"
    if not task_path.is_file():
        print(f"f2_dispatch: 任务件不在位: {task_path}", file=sys.stderr)
        return 2
    task = read_json(task_path)
    result = read_json(result_path) if result_path.is_file() else None
    payload = {"command": "show", "task": task, "result": result}
    if args.json:
        print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="F2 全量门派单侧：把在役收件箱派单面已登记的 F2 订单翻译成 runner 任务件")
    sub = parser.add_subparsers(dest="cmd", required=True)

    ingest = sub.add_parser("ingest", help="从在役收件箱 + 派单记录建 F2 任务队列")
    ingest.add_argument("--triggers-home", default=os.environ.get("AF_TRIGGERS_HOME", ""),
                         help="在役触发面目录（af-state/triggers；只读）")
    ingest.add_argument("--queue", default=str(default_queue_dir()),
                         help="runner 自有队列目录（只写这里）")
    ingest.add_argument("--registry", default="", help="注册表路径（缺省=运行位，再缺省=仓内正本）")
    ingest.add_argument("--max-attempts", type=int, default=DEFAULT_MAX_ATTEMPTS,
                         help="最大重试次数（默认 3）")
    ingest.add_argument("--allow-missing-dispatch-record", action="store_true",
                         help="放宽：在役派单记录缺席也建单（默认 fail-closed 拒建）")
    ingest.add_argument("--json", action="store_true", help="机读输出")
    ingest.set_defaults(func=cmd_ingest)

    lst = sub.add_parser("list", help="列队列")
    lst.add_argument("--queue", default=str(default_queue_dir()))
    lst.add_argument("--json", action="store_true")
    lst.set_defaults(func=cmd_list)

    show = sub.add_parser("show", help="看单件（任务件 + 结果件）")
    show.add_argument("--queue", default=str(default_queue_dir()))
    show.add_argument("--task-id", required=True)
    show.add_argument("--json", action="store_true")
    show.set_defaults(func=cmd_show)

    args = parser.parse_args(argv)
    if args.cmd == "ingest" and not args.triggers_home:
        print("f2_dispatch: 缺 --triggers-home（或设 AF_TRIGGERS_HOME）", file=sys.stderr)
        return 2
    return int(args.func(args))


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""f2_runner — F2 全量门「独立 runner」执行面（v0.3-T7① F2 批新增件）

职责：把一个 F2 任务件**领走 → 核宿主供给 → 跑门 → 落 SUMMARY → 结论入账
（integrator 复核制）→ 回报 → 退租约**。

────────────────────────────────────────────────────────────────────────────
★ 执行面纪律（与 CI-BOUNDARY / INTEGRATOR-SOP 逐条对齐）：

1. **F2 全量门不进 Actions**：本件是**独立 runner**（一次性 CLI，非常驻守护、
   非监听面、不用 sudo）。CI 执行器分工见 `docs/CI-BOUNDARY.md` §三。
2. **宿主供给面显式**：`bin/cosign`（不入库，`.gitignore` 行 2 `/bin`）、
   `cargo-deny`（PATH）、`advisory-db`（`~/.cargo/advisory-dbs/advisory-db-3157b0e258782691`）、
   `netns` 断网能力（`unshare -Urn`；`sudo unshare -n` 由 `ci/run.sh` 自带，
   **本件不调用 sudo**，只探测不升权）。缺件处置见下方 §宿主供给。
3. **★ SKIP 不得计绿**：宿主供给缺件 ⇒ **本件不跑门**、**不写 integrator 记录件**、
   结论 `skip-host-supply-missing` ⇒ 任务回队（退避重试），**永不**记绿。
   门跑完后若 SUMMARY 出现非绿 step 状态（FAIL / ADVISORY-FAIL / 未知词）
   ⇒ 同样不写记录件（`ci/integrator_bind_check.py emit` 也会独立拒）。
4. **四元组禁手抄**：`base_sha`/`source_sha`/`tested_tree`/`suite_digest` 由
   `ci/integrator_bind_check.py emit` **机器自算**并写进记录件；本件只把记录件
   里那四个值**读出来**放进结果件（抄的是工具产物，不是人抄）。
5. **领单唯一写者**：本件是队列的**唯一写者**；同一任务同一时刻只允许一个
   runner 领走——`flock` 互斥锁（进程死则自动释放）+ **租约**（超时才可重领）。
6. **幂等**：同一 `task_id` 重复 `run` ⇒ 已完成的直接返回既有结果（不重跑门）；
   派单面幂等由 `ci/f2_dispatch.py` 的 `idempotency_key` 保证。
7. **不删记录**：结果件、门日志、记录件一律留档；被拒/红的也留。

────────────────────────────────────────────────────────────────────────────
宿主供给契约（`af.f2.host_supply.v1`）——放哪 / 怎么核 / 缺件怎么办：

| 件 | 放在哪 | 怎么核 | 缺件怎么办 |
|---|---|---|---|
| `cosign` | `<候选树>/bin/cosign`（**不入库**） | 文件存在 + 可执行 + sha256 + `cosign version` | **SKIP**（environment_unavailable）⇒ 不跑门、不计绿；回队列重试；结果件点名缺件与投放路径 |
| `cargo-deny` | PATH（`~/.cargo/bin/cargo-deny`） | `which` + sha256 + `cargo-deny --version` | 同上 |
| `advisory-db` | `$CARGO_HOME/advisory-dbs/advisory-db-3157b0e258782691` | 目录存在 + 文件数 + （若是 git 仓）HEAD sha | 同上（刷新走 `ci/refresh-advisory-db.sh`；GitHub 不可达时沿用旧库 WARN 降级） |
| `netns` | 内核能力 | `unshare -Urn true` 退出码 0 | step6 断网隔离不可用 ⇒ 门必红（run.sh 自身语义）；本件只探测不升权 |

★ 跳过档口径承 `docs/CI-BOUNDARY.md` §一：step6（cosign）/ step7（deny）的
缺件跳过档是**对账口径**，不是预检改写；且 **SKIP 不得计绿**——本件把这一条
落成机器行为（缺件 ⇒ 无记录件 ⇒ integrator 复核无从通过）。

────────────────────────────────────────────────────────────────────────────
用法：

  # 宿主供给核（只读）
  python3 -I ci/f2_runner.py supply --repo <候选树>

  # 领单 → 跑门 → 入账 → 回报
  python3 -I ci/f2_runner.py run --queue <队列> --task-id <id> \
      --repo <候选树> [--worktree <门执行目录>] [--gate-command "bash ci/run.sh"]

  # 结论入账交接（rebase 漂移 ⇒ 可执行动作清单）
  python3 -I ci/f2_runner.py handover --record <记录件> --repo <仓> \
      --base-ref origin/product/main --branch <候选分支>

退出码：0 = 绿（结论 pass / 幂等无操作）；1 = 红（门红 / 缺件 / 复用失败）；
      2 = 用法错或环境错。**任何非 0 都是红。**
"""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

AF_ROOT = Path(__file__).resolve().parent.parent
TASK_SCHEMA = "af.f2.task.v1"
RESULT_SCHEMA = "af.f2.result.v1"
SUPPLY_SCHEMA = "af.f2.host_supply.v1"
# ci/integrator_bind_check.py 的 SUMMARY 词表与槽位口径（**逐字对齐**，改这里须与
# 复核器同批评审）：白名单 = 非红；红名单 = FAIL / ADVISORY-FAIL / 未知词。
STEP_STATUS_ALLOWED = frozenset({"PASS", "PASS-WITH-WARN", "ADVISORY-PASS", "ADVISORY-WARN"})
STEP_LINE_RE = re.compile(r"^step\S*\s+(\S+)\s+\d+ms\b")
# ci/run.sh 十四步槽位（step13 共宿 a/b/c、step14 共宿 a/b/c/d ⇒ 槽位 14 个）
REQUIRED_SLOTS = tuple(str(i) for i in range(1, 15))
# 退避参数（指数 + 抖动）
BACKOFF_BASE_SEC = 60
BACKOFF_MAX_SEC = 3600
BACKOFF_JITTER = 0.2
ADVISORY_DB_DIRNAME = "advisory-db-3157b0e258782691"


# ── 小工具 ────────────────────────────────────────────────────────────────────
def now_iso() -> str:
    return datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")


def now_ts() -> float:
    return time.time()


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def atomic_write_json(path: Path, obj: Any) -> None:
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


def run_cmd(cmd: list[str], cwd: Path | None = None, timeout: int = 60) -> tuple[int, str]:
    try:
        proc = subprocess.run(cmd, cwd=str(cwd) if cwd else None, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True, timeout=timeout)
    except FileNotFoundError:
        return 127, f"{cmd[0]}: 不在位"
    except subprocess.TimeoutExpired:
        return 124, f"{cmd[0]}: 超时 {timeout}s"
    except Exception as error:  # noqa: BLE001
        return 125, f"{cmd[0]}: {type(error).__name__}: {error}"
    return proc.returncode, (proc.stdout or "").strip()


def git(repo: Path, args: list[str], check: bool = False) -> tuple[int, str]:
    rc, out = run_cmd(["git", "-C", str(repo)] + args, timeout=120)
    if check and rc != 0:
        raise RuntimeError(f"git {' '.join(args)} → exit {rc}: {out[:300]}")
    return rc, out


# ── 宿主供给核 ────────────────────────────────────────────────────────────────
def check_supply(repo_root: Path, cargo_home: Path | None = None) -> dict:
    """宿主供给核（只读）。返回 `af.f2.host_supply.v1` 清单。"""
    pieces: dict[str, dict] = {}

    # ① cosign —— <候选树>/bin/cosign（不入库）
    cosign = repo_root / "bin" / "cosign"
    cosign_present = cosign.is_file() and os.access(cosign, os.X_OK)
    cosign_version = ""
    if cosign_present:
        rc, out = run_cmd([str(cosign), "version"], timeout=30)
        cosign_version = out.splitlines()[0][:120] if rc == 0 and out else f"version 调用 exit={rc}"
    pieces["cosign"] = {
        "path": str(cosign),
        "present": bool(cosign_present),
        "sha256": sha256_file(cosign)[:12] if cosign_present else "",
        "version": cosign_version,
        "required": True,
        "supply_rule": "放宿主 clone 的 <候选树>/bin/cosign（bin/ 不入库，见 .gitignore 行 2 /bin）",
    }

    # ② cargo-deny —— PATH
    deny_path = shutil.which("cargo-deny")
    deny_version = ""
    if deny_path:
        rc, out = run_cmd([deny_path, "--version"], timeout=30)
        deny_version = out.splitlines()[0][:120] if rc == 0 and out else f"version 调用 exit={rc}"
    pieces["cargo-deny"] = {
        "path": deny_path or "",
        "present": bool(deny_path),
        "sha256": sha256_file(Path(deny_path))[:12] if deny_path else "",
        "version": deny_version,
        "required": True,
        "supply_rule": "cargo install cargo-deny（仓外供给；本件不代装）",
    }

    # ③ advisory-db —— $CARGO_HOME/advisory-dbs/<dirname>
    home = cargo_home or (Path(os.environ["CARGO_HOME"]) if os.environ.get("CARGO_HOME")
                          else Path.home() / ".cargo")
    db_dir = home / "advisory-dbs" / ADVISORY_DB_DIRNAME
    db_files = 0
    db_head = ""
    if db_dir.is_dir():
        db_files = sum(1 for _ in db_dir.rglob("*") if _.is_file())
        if (db_dir / ".git").exists():
            rc, out = git(db_dir, ["rev-parse", "HEAD"])
            db_head = out.strip() if rc == 0 else ""
    pieces["advisory-db"] = {
        "path": str(db_dir),
        "present": db_dir.is_dir(),
        "sha256": db_head[:12] if db_head else "",
        "version": f"files={db_files}" + (f" git_head={db_head[:12]}" if db_head else ""),
        "required": True,
        "supply_rule": f"ci/refresh-advisory-db.sh 刷新到 {db_dir}（代理 socks5h://<内网地址>:1080；"
                       "GitHub 不可达时沿用旧库 WARN 降级）",
    }

    # ④ netns —— 断网能力（只探测不升权；本件永不调用 sudo）
    unshare_rc, unshare_out = run_cmd(["unshare", "-Urn", "true"], timeout=20)
    netns_ok = unshare_rc == 0
    pieces["netns"] = {
        "path": shutil.which("unshare") or "",
        "present": netns_ok,
        "sha256": "",
        "version": ("unshare -Urn 可用（非特权 user+netns）" if netns_ok
                    else f"unshare -Urn exit={unshare_rc}: {unshare_out[:80]}"),
        "required": True,
        "supply_rule": "内核需允许非特权 user namespace（unshare -Urn）；ci/run.sh 另有 sudo unshare -n "
                       "优先路径（由 run.sh 自带，本件不调用 sudo）",
    }

    missing = [name for name, piece in pieces.items() if piece["required"] and not piece["present"]]
    return {
        "schema_version": SUPPLY_SCHEMA,
        "checked_at": now_iso(),
        "repo": str(repo_root),
        "pieces": pieces,
        "missing": missing,
        "ok": not missing,
        "policy": "缺件 ⇒ SKIP（environment_unavailable）⇒ 不跑门、不写记录件、不得计绿"
                  "（docs/CI-BOUNDARY.md §一；SKIP 不得计绿）",
    }


# ── SUMMARY 解析（口径逐字对齐 ci/integrator_bind_check.py）──────────────────
def parse_summary(text: str) -> dict:
    steps: list[dict] = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        match = STEP_LINE_RE.match(line)
        if not match:
            continue
        token = line.split()[0]
        slot = token[len("step"):]
        slot = re.sub(r"[a-z]+$", "", slot)
        steps.append({"line": line, "status": match.group(1), "slot": slot})
    covered = sorted({s["slot"] for s in steps}, key=lambda s: (len(s), s))
    bad = [s for s in steps if s["status"] not in STEP_STATUS_ALLOWED]
    missing_slots = [s for s in REQUIRED_SLOTS if s not in covered]
    return {
        "steps": steps,
        "step_count": len(steps),
        "slots_covered": covered,
        "slots_required": list(REQUIRED_SLOTS),
        "slots_missing": missing_slots,
        "non_green_steps": [s["line"] for s in bad],
        "green": (not bad) and (not missing_slots) and bool(steps),
    }


# ── 队列件 ────────────────────────────────────────────────────────────────────
def task_path(queue: Path, task_id: str) -> Path:
    return queue / "tasks" / f"{task_id}.json"


def load_task(queue: Path, task_id: str) -> dict:
    path = task_path(queue, task_id)
    if not path.is_file():
        raise FileNotFoundError(f"任务件不在位: {path}")
    task = read_json(path)
    if task.get("schema_version") != TASK_SCHEMA:
        raise ValueError(f"任务件 schema 不识别: {task.get('schema_version')!r}")
    return task


def save_task(queue: Path, task: dict) -> None:
    task["updated_at"] = now_iso()
    atomic_write_json(task_path(queue, task["task_id"]), task)


def backoff_seconds(attempts: int) -> int:
    """指数退避 + 抖动（±20%）。attempts = 已失败次数。"""
    raw = min(BACKOFF_BASE_SEC * (2 ** max(0, attempts - 1)), BACKOFF_MAX_SEC)
    jitter = (hashlib.sha256(f"{attempts}:{now_ts()}".encode()).digest()[0] / 255.0) * 2 - 1
    value = int(raw * (1 + BACKOFF_JITTER * jitter))
    return max(1, min(value, BACKOFF_MAX_SEC))


class Lease:
    """领单互斥 + 租约。

    · 互斥：`flock(LOCK_EX|LOCK_NB)` —— 进程死则内核自动释放（崩溃安全）；
    · 租约：任务件里的 `lease{runner_id,claimed_at,expires_at}` —— 超时才可被
      他人重领（防「进程活着但卡死」时任务被无限期占住）。
    """

    def __init__(self, queue: Path, task: dict, runner_id: str, ttl: int) -> None:
        self.queue = queue
        self.task = task
        self.runner_id = runner_id
        self.ttl = ttl
        self.lock_path = queue / "locks" / f"{task['task_id']}.lock"
        self.fd: int | None = None
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def _lease_obj(self) -> dict:
        return {
            "runner_id": self.runner_id,
            "claimed_at": now_iso(),
            "expires_at": datetime.fromtimestamp(now_ts() + self.ttl, tz=timezone.utc)
            .astimezone().isoformat(timespec="seconds"),
            "ttl_sec": self.ttl,
        }

    def acquire(self) -> tuple[bool, str]:
        self.lock_path.parent.mkdir(parents=True, exist_ok=True)
        self.fd = os.open(str(self.lock_path), os.O_CREAT | os.O_RDWR, 0o644)
        try:
            fcntl.flock(self.fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            return False, "ALREADY-CLAIMED：另一个 runner 正持本任务的互斥锁"
        current = self.task.get("lease")
        if isinstance(current, dict) and current.get("expires_at"):
            try:
                expires = datetime.fromisoformat(current["expires_at"]).timestamp()
            except ValueError:
                expires = 0.0
            if expires > now_ts():
                return False, (f"ALREADY-CLAIMED：租约未过期（runner={current.get('runner_id')} "
                              f"expires_at={current.get('expires_at')}）")
        self.task["lease"] = self._lease_obj()
        self.task["status"] = "claimed"
        save_task(self.queue, self.task)
        self._thread = threading.Thread(target=self._renew, daemon=True)
        self._thread.start()
        return True, "claimed"

    def _renew(self) -> None:
        while not self._stop.wait(max(5, self.ttl // 3)):
            try:
                task = load_task(self.queue, self.task["task_id"])
                lease = task.get("lease") or {}
                if lease.get("runner_id") != self.runner_id:
                    return
                task["lease"] = self._lease_obj()
                save_task(self.queue, task)
                self.task = task
            except Exception:  # noqa: BLE001 - 续租失败不崩门（到期后他人可重领）
                return

    def release(self, keep_lease: bool = False) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=2)
        try:
            task = load_task(self.queue, self.task["task_id"])
            lease = task.get("lease") or {}
            if lease.get("runner_id") == self.runner_id and not keep_lease:
                task["lease"] = None
                save_task(self.queue, task)
        except Exception:  # noqa: BLE001
            pass
        if self.fd is not None:
            try:
                fcntl.flock(self.fd, fcntl.LOCK_UN)
            finally:
                os.close(self.fd)
                self.fd = None


# ── 子命令：supply ────────────────────────────────────────────────────────────
def cmd_supply(args: argparse.Namespace) -> int:
    repo = Path(args.repo).expanduser()
    manifest = check_supply(repo)
    if args.out:
        out = Path(args.out).expanduser()
        atomic_write_json(out, manifest)
        manifest["manifest_path"] = str(out)
    if args.json:
        print(json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print(f"宿主供给核：repo={repo}  结论={'齐备' if manifest['ok'] else '缺件'}")
        for name, piece in manifest["pieces"].items():
            mark = "OK " if piece["present"] else "缺 "
            print(f"  [{mark}] {name:12s} {piece['path']}  {piece['version']}")
        if manifest["missing"]:
            print(f"缺件（⇒ SKIP，不得计绿）：{', '.join(manifest['missing'])}")
    return 0 if manifest["ok"] else 1


# ── 子命令：run ───────────────────────────────────────────────────────────────
def cmd_run(args: argparse.Namespace) -> int:
    queue = Path(args.queue).expanduser()
    repo = Path(args.repo).expanduser()
    worktree = Path(args.worktree).expanduser() if args.worktree else repo
    runner_id = args.runner_id or f"{socket.gethostname()}:{os.getpid()}"
    try:
        task = load_task(queue, args.task_id)
    except Exception as error:  # noqa: BLE001
        print(f"f2_runner: {error}", file=sys.stderr)
        return 2

    result_path = queue / "results" / f"{task['task_id']}.json"
    # 幂等：已完成的任务重复 run ⇒ 直接回既有结果（不重跑门）
    if task.get("status") == "done" and result_path.is_file():
        old = read_json(result_path)
        payload = {"command": "run", "idempotent": True, "result": old}
        print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
        return 0 if old.get("verdict") == "pass" else 1

    # 退避未到 ⇒ 本轮不领（不是失败）
    next_at = task.get("next_attempt_at")
    if next_at:
        try:
            if datetime.fromisoformat(next_at).timestamp() > now_ts():
                print(f"f2_runner: 退避中，next_attempt_at={next_at}（本轮不领单）")
                return 0
        except ValueError:
            pass

    lease = Lease(queue, task, runner_id, args.lease_ttl)
    ok, reason = lease.acquire()
    if not ok:
        print(f"f2_runner: 领单失败：{reason}", file=sys.stderr)
        return 1
    task = lease.task

    started = now_iso()
    t0 = now_ts()
    reason_codes: list[str] = []
    verdict = "error"
    binding_ok = True          # 候选树绑定面成立与否（与 verdict 分开记，避免自锁）
    gate_info: dict[str, Any] = {}
    supply_info: dict[str, Any] = {}
    integrator_info: dict[str, Any] = {}
    base_head = ""

    try:
        # ① 候选树绑定：门必须跑在被测提交上（工作树干净 + HEAD == source_sha）
        head_rc, head_out = git(worktree, ["rev-parse", "HEAD"])
        head = head_out.strip()
        if head_rc != 0:
            reason_codes.append("WORKTREE-UNRESOLVED")
            binding_ok = False
        elif head != task.get("source_sha"):
            reason_codes.append("SOURCE-SHA-MISMATCH")
            binding_ok = False
        dirty_rc, dirty_out = git(worktree, ["status", "--porcelain"])
        if dirty_rc == 0 and dirty_out.strip():
            reason_codes.append("WORKTREE-DIRTY")
            binding_ok = False

        # ② 宿主供给核（缺件 ⇒ 不跑门；SKIP 不得计绿）
        supply_info = check_supply(repo)
        supply_manifest = queue / "supply" / f"host-supply-{task['task_id']}.json"
        atomic_write_json(supply_manifest, supply_info)
        supply_info["manifest_path"] = str(supply_manifest)

        # ③ 目标分支头（门跑时的目标头；供 handover 比对主线推进面）
        base_rc, base_out = git(repo, ["rev-parse", task.get("base_ref", "origin/product/main")])
        base_head = base_out.strip() if base_rc == 0 else ""

        if not binding_ok:
            verdict = "error"   # 绑定面已红 ⇒ 不往下跑（不跑门、不写记录件）
        elif not supply_info["ok"]:
            reason_codes.append("SKIP-HOST-SUPPLY-MISSING")
            verdict = "skip-host-supply-missing"
        else:
            # ④ 跑门（门命令原文进结果件与记录件——不许改写）
            gate_command = args.gate_command or task.get("gate_command") or "bash ci/run.sh"
            log_dir = queue / "logs"
            log_dir.mkdir(parents=True, exist_ok=True)
            attempt = int(task.get("attempts", 0)) + 1
            log_path = log_dir / f"{task['task_id']}.attempt{attempt}.log"
            t_start = now_ts()
            try:
                proc = subprocess.run(["bash", "-lc", gate_command], cwd=str(worktree),
                                      stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                      text=True, timeout=args.gate_timeout)
                gate_exit = proc.returncode
                gate_out = proc.stdout or ""
            except subprocess.TimeoutExpired as error:
                gate_exit = 124
                gate_out = (error.stdout or "") if isinstance(error.stdout, str) else ""
                gate_out += f"\n[f2_runner] 门超时（{args.gate_timeout}s）⇒ 记为超时红\n"
            except Exception as error:  # noqa: BLE001
                gate_exit = 125
                gate_out = f"[f2_runner] 门执行异常: {type(error).__name__}: {error}\n"
            t_end = now_ts()
            log_path.write_text(gate_out, encoding="utf-8")
            summary_path = (Path(args.summary_path).expanduser() if args.summary_path
                            else worktree / "ci" / "out" / "SUMMARY.txt")
            summary_text = ""
            summary_sha = ""
            summary_info: dict[str, Any] = {}
            if summary_path.is_file():
                summary_text = summary_path.read_text(encoding="utf-8", errors="replace")
                summary_sha = sha256_hex(summary_text.encode("utf-8"))
                summary_info = parse_summary(summary_text)
            gate_info = {
                "command": gate_command,
                "exit": gate_exit,
                "started_at": started,
                "duration_ms": int((t_end - t_start) * 1000),
                "log_path": str(log_path),
                "summary_path": str(summary_path),
                "summary_present": summary_path.is_file(),
                "summary_sha256": summary_sha,
                "summary": summary_info,
                "worktree": str(worktree),
            }
            if gate_exit != 0:
                reason_codes.append(f"GATE-EXIT-NONZERO({gate_exit})")
                verdict = "fail"
            elif not summary_path.is_file():
                reason_codes.append("GATE-SUMMARY-ABSENT")
                verdict = "fail"
            elif not summary_info.get("green"):
                if summary_info.get("slots_missing"):
                    reason_codes.append("SUMMARY-SLOTS-INCOMPLETE("
                                        + ",".join(summary_info["slots_missing"]) + ")")
                if summary_info.get("non_green_steps"):
                    reason_codes.append("SUMMARY-NON-GREEN-STEP")
                verdict = "fail"
            else:
                # 门 exit 0 + SUMMARY 全绿 + 14 槽位齐全 ⇒ 唯一可入账的绿
                verdict = "pass"

            # ⑤ 结论入账：只有「门绿 + SUMMARY 全绿 + 槽位齐全」才写记录件
            if verdict == "pass":
                checker = repo / "ci" / "integrator_bind_check.py"
                if not checker.is_file():
                    reason_codes.append("INTEGRATOR-CHECKER-ABSENT")
                    verdict = "error"
                else:
                    record_dir = (Path(args.record_dir).expanduser() if args.record_dir
                                  else worktree / "ci" / "out")
                    record_path = record_dir / f"integrator-record-f2-{task['task_id']}.json"
                    emit_cmd = [sys.executable, "-I", str(checker), "emit",
                                "--record", str(record_path),
                                "--repo", str(repo),
                                "--branch", task.get("source_branch") or task["task_id"],
                                "--remote", task.get("remote", "origin"),
                                "--base-ref", task.get("base_ref", "origin/product/main"),
                                "--gate-id", task.get("gate_id", "f2-full"),
                                "--gate-command", gate_command,
                                "--gate-exit", str(gate_exit),
                                "--gate-summary", str(summary_path),
                                "--recorded-by", runner_id]
                    # 远端头绑定优先（source_sha 语义 = 远端该分支头）；远端不可达
                    # 或不等 ⇒ 显式用本地提交并在结果件登记 source_origin。
                    remote_rc, remote_out = git(repo, ["ls-remote", task.get("remote", "origin"),
                                                      f"refs/heads/{task.get('source_branch') or ''}"])
                    remote_sha = remote_out.split()[0] if remote_rc == 0 and remote_out.split() else ""
                    source_origin = "本地提交（未绑定远端头）"
                    if remote_sha and remote_sha == task.get("source_sha"):
                        source_origin = f"{task.get('remote','origin')} refs/heads/{task.get('source_branch')} 的远端头"
                    else:
                        emit_cmd += ["--source", str(task.get("source_sha"))]
                    emit_rc, emit_out = run_cmd(emit_cmd, cwd=repo, timeout=300)
                    integrator_info = {
                        "record_path": str(record_path),
                        "emit_exit": emit_rc,
                        "emit_stdout": emit_out[:4000],
                        "source_origin": source_origin,
                    }
                    if emit_rc != 0:
                        reason_codes.append("INTEGRATOR-EMIT-REJECTED")
                        verdict = "fail"
                    else:
                        record = read_json(record_path)
                        integrator_info["record_sha256"] = sha256_file(record_path)
                        integrator_info["four_tuple"] = {key: record.get(key) for key in
                                                       ("base_sha", "source_sha", "tested_tree",
                                                        "suite_digest")}
                        integrator_info["verdict"] = record.get("verdict")
                        integrator_info["gate_base_sha"] = record.get("gate_base_sha")
    finally:
        lease.release()

    finished = now_iso()
    task = load_task(queue, task["task_id"])
    task["attempts"] = int(task.get("attempts", 0)) + 1
    if verdict == "pass":
        task["status"] = "done"
        task["lease"] = None
        save_task(queue, task)
    else:
        task["status"] = "queued"
        task["lease"] = None
        if task["attempts"] >= int(task.get("max_attempts", 3)):
            task["status"] = "failed"
        task["backoff_sec"] = backoff_seconds(int(task["attempts"]))
        task["next_attempt_at"] = datetime.fromtimestamp(
            now_ts() + task["backoff_sec"], tz=timezone.utc).astimezone().isoformat(timespec="seconds")
        save_task(queue, task)
        if task["status"] == "failed":
            dead = queue / "dead" / f"{task['task_id']}.json"
            atomic_write_json(dead, {"task_id": task["task_id"], "died_at": now_iso(),
                                     "reason_codes": reason_codes, "task": task})
            task["dead_letter_path"] = str(dead)
            save_task(queue, task)

    result = {
        "schema_version": RESULT_SCHEMA,
        "task_id": task["task_id"],
        "order_id": task.get("order_id"),
        "idempotency_key": task.get("idempotency_key"),
        "verdict": verdict,
        "reason_codes": reason_codes,
        "started_at": started,
        "finished_at": finished,
        "duration_ms": int((now_ts() - t0) * 1000),
        "runner": {"id": runner_id, "host": socket.gethostname(), "pid": os.getpid()},
        "repo": str(repo),
        "worktree": str(worktree),
        "base_head_at_gate_time": base_head,
        "base_sha_at_dispatch": task.get("base_sha"),
        "base_advanced_since_dispatch": bool(base_head and base_head != task.get("base_sha")),
        "host_supply": supply_info,
        "gate": gate_info,
        "integrator": integrator_info,
        "attempts": task.get("attempts"),
        "status": task.get("status"),
        "next_attempt_at": task.get("next_attempt_at"),
        "backoff_sec": task.get("backoff_sec"),
        "note": "结论只认三种：pass（门绿且 SUMMARY 全绿且槽位齐全、记录件已写）/ "
                "fail（门红或入账被拒）/ skip-host-supply-missing（宿主缺件 ⇒ SKIP 不得计绿）",
    }
    atomic_write_json(result_path, result)
    reported = queue / "reported.jsonl"
    reported.parent.mkdir(parents=True, exist_ok=True)
    with open(reported, "a", encoding="utf-8") as handle:
        handle.write(json.dumps({
            "task_id": task["task_id"], "order_id": task.get("order_id"),
            "verdict": verdict, "reason_codes": reason_codes,
            "reported_at": now_iso(),
            "result_path": str(result_path),
            "integrator_record": integrator_info.get("record_path"),
            "four_tuple": integrator_info.get("four_tuple"),
        }, ensure_ascii=False, sort_keys=True) + "\n")

    payload = {"command": "run", "idempotent": False, "result": result}
    print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if verdict == "pass" else 1


# ── 子命令：handover（结论入账交接：四元组来源 + rebase 漂移的可执行动作）───
def merge_tree(repo: Path, base_commit: str, source_commit: str) -> tuple[str, str]:
    rc, out = run_cmd(["git", "-C", str(repo), "merge-base", base_commit, source_commit], timeout=120)
    if rc != 0:
        return "", "merge-base 不可解析"
    base_ancestor_rc, _ = run_cmd(["git", "-C", str(repo), "merge-base", "--is-ancestor",
                                    base_commit, source_commit], timeout=120)
    if base_ancestor_rc == 0:
        rc, tree = run_cmd(["git", "-C", str(repo), "rev-parse", f"{source_commit}^{{tree}}"], timeout=120)
        return (tree.strip() if rc == 0 else ""), "fast-forward"
    rc, out = run_cmd(["git", "-C", str(repo), "merge-tree", "--write-tree", base_commit,
                       source_commit], timeout=300)
    if rc != 0:
        return "", "MERGE-TREE-CONFLICT"
    tree = out.strip().splitlines()[0].strip() if out.strip() else ""
    return tree, "merge-commit"


def suite_digest(repo: Path, tree: str) -> tuple[str, int]:
    """门套件摘要（算法与 ci/integrator_bind_check.py 同款：
    ci/ 与 .gitea/workflows/ 下的跟踪文件，排除 ci/out/、ci/__pycache__/、
    ci/advisory-db/ 与 *.bak；digest = sha256(逐成员 path\\0sha256(blob)\\n 拼接)）。

    ★ blob 取**字节**（不经文本管道、不 strip）——否则摘要与复核器算出的
    不一致（尾随换行被吃掉 ⇒ 假红）。
    """
    rc, out = run_cmd(["git", "-C", str(repo), "ls-tree", "-r", "-z", "--full-tree", tree], timeout=300)
    if rc != 0:
        return "", 0
    members = []
    for entry in out.split("\0"):
        if not entry:
            continue
        _, _, path = entry.partition("\t")
        if not path.startswith(("ci/", ".gitea/workflows/")):
            continue
        if path.startswith(("ci/out/", "ci/__pycache__/", "ci/advisory-db/")):
            continue
        if path.endswith(".bak"):
            continue
        members.append(path)
    members.sort()
    digest = hashlib.sha256()
    for path in members:
        try:
            blob = subprocess.run(["git", "-C", str(repo), "cat-file", "blob", f"{tree}:{path}"],
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                  timeout=120).stdout
        except Exception:  # noqa: BLE001
            continue
        digest.update(path.encode("utf-8"))
        digest.update(b"\0")
        digest.update(hashlib.sha256(blob).hexdigest().encode("ascii"))
        digest.update(b"\n")
    return digest.hexdigest(), len(members)


def cmd_handover(args: argparse.Namespace) -> int:
    repo = Path(args.repo).expanduser()
    record_path = Path(args.record).expanduser()
    if not record_path.is_file():
        print(f"f2_runner: 记录件不在位: {record_path}", file=sys.stderr)
        return 2
    record = read_json(record_path)
    base_rc, base_head = git(repo, ["rev-parse", args.base_ref])
    base_head = base_head.strip() if base_rc == 0 else ""
    source_sha = record.get("source_sha", "")
    tested_tree = record.get("tested_tree", "")
    gate_base = record.get("gate_base_sha", "")
    merge_tree_hash, merge_mode = merge_tree(repo, base_head, source_sha) if base_head else ("", "")
    merge_digest, merge_members = (suite_digest(repo, merge_tree_hash) if merge_tree_hash else ("", 0))
    source_tree = ""
    rc, out = git(repo, ["rev-parse", f"{source_sha}^{{tree}}"])
    source_tree = out.strip() if rc == 0 else ""
    source_digest, source_members = (suite_digest(repo, source_tree) if source_tree else ("", 0))

    base_drift = bool(gate_base and base_head and gate_base != base_head)
    tree_equivalent = bool(merge_tree_hash and merge_tree_hash == tested_tree)
    suite_equivalent = bool(merge_digest and merge_digest == record.get("suite_digest"))

    actions: list[dict] = []
    # 「内容等价」= 落地树 == 门跑过的树 且 门面摘要逐字节同 ⇒ 门结论仍描述这棵树
    # （SOP §5.2 步 6：基点变了但内容没变 ⇒ 可继续，但基点变了必须写进合流报告/台账）
    aligned = bool(tree_equivalent and suite_equivalent)
    if not aligned:
        actions = [
            {"order": 1, "what": "重落基点",
             "cmd": f"git -C {repo} fetch origin {args.base_ref} && "
                    f"git -C {repo} rebase origin/{args.base_ref.split('/')[-1]}",
             "note": "真合并形态改为把主线合入候选分支并解冲突，重做候选树定稿"},
            {"order": 2, "what": "重跑 F1 快速门",
             "cmd": f"git -C {repo} push origin <候选分支>  # Actions 自动重跑 F1"
                    "（.gitea/workflows/f1.yml，仅 base=product/main 触发）",
             "note": "F1 绿是合流的**必要非充分**条件；F1 绿**不替代** F2 记录件"},
            {"order": 3, "what": "重跑 F2 全量门",
             "cmd": "bash ci/run.sh   # 在候选树工作树上（重落基点之后）",
             "note": "按记录件 gate_id 重跑那一门；★ 不许拿「四项里前三项对上了」搪塞"},
            {"order": 4, "what": "重写记录件（新文件名带 -r2/-r3；旧件不许删）",
             "cmd": f"python3 -I {repo}/ci/integrator_bind_check.py emit --record "
                    f"{record_path.parent}/{record_path.stem}-r2.json --repo {repo} "
                    f"--branch <候选分支> --remote origin --base-ref {args.base_ref} "
                    f"--gate-id f2-full --gate-command \"bash ci/run.sh\" --gate-exit 0 "
                    f"--gate-summary <候选树>/ci/out/SUMMARY.txt --recorded-by <人>",
             "note": "emit 拒写非绿记录件（门摘要判不出绿 ⇒ 拒绝）"},
            {"order": 5, "what": "再复核",
             "cmd": f"python3 -I {repo}/ci/integrator_bind_check.py verify --record "
                    f"{record_path.parent}/{record_path.stem}-r2.json --repo {repo} "
                    f"--branch <候选分支> --remote origin --base-ref {args.base_ref}",
             "note": "复核绿 ⇒ 进 SOP §六 判读；红 ⇒ SOP §七"},
        ]

    payload = {
        "command": "handover",
        "record": str(record_path),
        "repo": str(repo),
        "base_ref": args.base_ref,
        "base_head_now": base_head,
        "gate_base_sha": gate_base,
        "base_drift": base_drift,
        "record_side": {key: record.get(key) for key in
                        ("base_sha", "source_sha", "tested_tree", "suite_digest", "verdict")},
        "measured_side": {
            "source_sha": source_sha,
            "source_tree": source_tree,
            "source_suite_digest": source_digest,
            "source_suite_members": source_members,
            "merge_tree": merge_tree_hash,
            "merge_mode": merge_mode,
            "merge_suite_digest": merge_digest,
            "merge_suite_members": merge_members,
        },
        "tree_equivalent": tree_equivalent,
        "suite_equivalent": suite_equivalent,
        "aligned": aligned,
        "actions": actions,
        "base_drift_note": ("基点已前移但内容等价（落地树 == 门跑过的树、门面逐字节同）"
                            "⇒ 门结论仍描述这棵树；基点变了这件事必须写进合流报告/台账"
                            "（docs/INTEGRATOR-SOP.md §5.2 步 6）") if (aligned and base_drift) else "",
        "note": "四元组来源＝记录件（由 ci/integrator_bind_check.py emit 机器自算，禁手抄）；"
                "本子命令只做**只读**实测侧复算与动作清单产出，**不执行** rebase/push"
                "（改 ref 与推送由 integrator 依 docs/INTEGRATOR-SOP.md §五 执行）",
    }
    if args.json:
        print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="F2 全量门独立 runner 执行面（一次性 CLI，非守护）")
    sub = parser.add_subparsers(dest="cmd", required=True)

    supply = sub.add_parser("supply", help="宿主供给核（只读）")
    supply.add_argument("--repo", required=True, help="候选树仓根")
    supply.add_argument("--out", default="", help="供给清单落盘路径")
    supply.add_argument("--json", action="store_true")
    supply.set_defaults(func=cmd_supply)

    run = sub.add_parser("run", help="领单 → 跑门 → 入账 → 回报")
    run.add_argument("--queue", required=True, help="runner 自有队列目录")
    run.add_argument("--task-id", required=True)
    run.add_argument("--repo", required=True, help="候选树仓根（含 ci/integrator_bind_check.py）")
    run.add_argument("--worktree", default="", help="门执行目录（缺省=--repo）")
    run.add_argument("--gate-command", default="", help="门命令（缺省=任务件的 gate_command）")
    run.add_argument("--summary-path", default="", help="门摘要路径（缺省 <worktree>/ci/out/SUMMARY.txt）")
    run.add_argument("--record-dir", default="", help="记录件目录（缺省 <worktree>/ci/out）")
    run.add_argument("--lease-ttl", type=int, default=1800, help="租约秒数（默认 1800）")
    run.add_argument("--gate-timeout", type=int, default=14400, help="门超时秒数（默认 14400=4h）")
    run.add_argument("--runner-id", default="", help="runner 标识（缺省=主机名:pid）")
    run.add_argument("--json", action="store_true")
    run.set_defaults(func=cmd_run)

    handover = sub.add_parser("handover", help="结论入账交接（只读实测 + 动作清单）")
    handover.add_argument("--record", required=True)
    handover.add_argument("--repo", required=True)
    handover.add_argument("--base-ref", required=True)
    handover.add_argument("--branch", default="", help="候选分支（仅供人工对照）")
    handover.add_argument("--json", action="store_true")
    handover.set_defaults(func=cmd_handover)

    args = parser.parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    sys.exit(main())

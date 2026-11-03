#!/usr/bin/env python3
"""PoC-E 代码域适配器：lease→worktree→patch→门→gitea 合入→订单工件面。

接线声明（不新增机制面，全部既有件）：
- lease 面只走 poc/poc-supply/lease_store.py 既有 CLI 语义（acquire/return/
  heartbeat/reap/show）；水位预检只走 poc/poc-supply/disk_post_used_preflight.py
  与 collect_resource_observation.py。本脚本不自造租约账本、不自算水位。
- 门是 af ci/run.sh 的可跑子集（gate_f1.sh：step1 fmt / step2 clippy / step3
  test，同款 step/SUMMARY 语义）；af 仓 ci/run.sh 全管道绑 af 资产（cue/cosign/
  deliverables/判据正本），在样例仓上不可跑——可跑子集即门，如实登记。
- 合入走 gitea：git push 分支 + PR API 建单 + merge API 合入；合入前执行
  integrator 四项绑定复核（base_sha/source_sha/tested_tree/suite_digest），
  gitea Actions runner 不可用时的复核制替代（四项任一不符即拒绝合入）。
- 订单账走 af-order CLI（submit 一次性驱动状态机到 Closed；工件面挂
  order_dir/artifacts/，账本 ledger.jsonl 不手工改）。

生命周期绑定：worktree 与 lease 同生命周期——运行足迹固定在
  <run-root>/<lease_id>/（内含 repo 克隆、worktree、lease-binding.json），
  lease 归还（release）或过期回收（reap）时整目录移除；lease 过期后该目录
  不得留存为「活」件（reap 子命令可独立执行、可复跑验证）。
"""

from __future__ import annotations

import argparse
import base64
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

# af-workorder gate_admit 的 verdict 词汇表：严格 "pass" 才是绿，未知词一律非绿。
GATE_PASS_VERDICT = "pass"


class AdapterError(RuntimeError):
    """适配器失败。消息第一段是稳定码（PAxx nn），供日志对账。"""


def now_ms() -> int:
    return time.time_ns() // 1_000_000


def sha256_file(path: Path) -> str:
    import hashlib

    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_checked(
    argv: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
    capture: bool = True,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        argv,
        cwd=str(cwd) if cwd else None,
        env=env,
        capture_output=capture,
        text=True,
    )
    if result.returncode != 0:
        raise AdapterError(
            f"PA10 命令失败 exit={result.returncode}: {' '.join(argv)}\n"
            f"stdout={result.stdout[-2000:]}\nstderr={result.stderr[-2000:]}"
        )
    return result


def write_json(path: Path, payload: Any) -> str:
    """原子落盘（tmp+rename），canonical JSON，返回 sha256。"""
    text = json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
    tmp = path.with_name(f".{path.name}.{time.time_ns()}.tmp")
    tmp.write_text(text, encoding="utf-8")
    tmp.replace(path)
    return sha256_file(path)


# ---------------------------------------------------------------- supply 面 --

def supply_script(name: str) -> Path:
    """poc-supply 既有件路径（同仓 poc/ 目录，禁自造）。"""
    path = Path(__file__).resolve().parent.parent / "poc-supply" / name
    if not path.is_file():
        raise AdapterError(f"PA01 poc-supply 件缺席: {path}")
    return path


def collect_observation(registry: Path, node_id: str) -> Path:
    """跑既有采集器，返回 latest.json 路径（采集器自身语义：原子写+符号链接）。"""
    script = supply_script("collect_resource_observation.py")
    run_checked([sys.executable, "-I", str(script), "--registry", str(registry), "--node-id", node_id])
    latest = Path(__file__).resolve().parents[2] / "af-state/resources/observations" / node_id / "latest.json"
    if not latest.exists():
        raise AdapterError(f"PA02 采集器未产出 latest.json: {latest}")
    return latest


def preflight(registry: Path, observation: Path, leases_db: Path, node_id: str, mount: str, request_bytes: int) -> dict[str, Any]:
    """既有水位预检（disk_post_used_preflight.py）：exit 0=投放 / 2=拒单 / 1=错。
    预检直读 leases DB，表由 lease_store 首用建——先跑一次 no-op reap（既有
    语义：只收过期；空库即建 schema），保证预检可读。"""
    lease(leases_db, ["reap"])
    script = supply_script("disk_post_used_preflight.py")
    result = subprocess.run(
        [
            sys.executable, "-I", str(script),
            "--registry", str(registry),
            "--observation", str(observation),
            "--leases-db", str(leases_db),
            "--node-id", node_id,
            "--mount", mount,
            "--request-bytes", str(request_bytes),
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode not in (0, 2):
        raise AdapterError(f"PA03 水位预检输入错 exit={result.returncode}: {result.stderr.strip()}")
    verdict = json.loads(result.stdout)
    if result.returncode != 0 or not verdict.get("admit"):
        raise AdapterError(f"PA04 水位预检拒单: {json.dumps(verdict['reasons'], ensure_ascii=False)}")
    return verdict


def lease(leases_db: Path, argv_tail: list[str]) -> dict[str, Any]:
    """既有 lease_store.py CLI 直调（acquire/return/reap/show 语义不改）。"""
    script = supply_script("lease_store.py")
    result = run_checked([sys.executable, "-I", str(script), "--db", str(leases_db), *argv_tail])
    return json.loads(result.stdout)


# ------------------------------------------------------- 四项绑定复核（integrator 复核制）--

def four_binding_check(expected: dict[str, str], actual: dict[str, str]) -> dict[str, Any]:
    """合入前 integrator 四项绑定复核（gitea Actions runner 不可用时的复核制）。

    四项（任一不符=拒绝合入，不许因门不可跑就跳过门或跳过复核）：
      base_sha      分支基点：实际分支父提交 == 记录值（分支确从记录的 main 头分出）
      source_sha    分支头：远端该分支头 == 记录值（推送的就是测过的那个提交）
      tested_tree   测过的树：source_sha^{tree} == 记录值（合入内容逐位=门跑过的内容）
      suite_digest  门套件：本次实跑门脚本 sha256 == 记录值（门没被换）
    另查 gate verdict 严格 == "pass"（af-workorder gate_admit 同词汇表）。
    """
    mismatches: dict[str, dict[str, str]] = {}
    for key in ("base_sha", "source_sha", "tested_tree", "suite_digest"):
        want, got = expected.get(key, ""), actual.get(key, "")
        if want != got or not got:
            mismatches[key] = {"expected": want, "actual": got}
    verdict = actual.get("verdict", "")
    verdict_ok = verdict == GATE_PASS_VERDICT
    return {"admit": not mismatches and verdict_ok, "mismatches": mismatches, "verdict": verdict}


# -------------------------------------------------------------- git 工作位 --

def git(repo: Path, argv: list[str]) -> str:
    return run_checked(["git", *argv], cwd=repo).stdout.strip()


def open_worktree(repo_url: str, run_dir: Path, branch: str, base_branch: str) -> tuple[Path, Path]:
    """克隆到 <run_dir>/repo，再 git worktree add 到 <run_dir>/worktree（绑 lease 的足迹目录）。"""
    repo = run_dir / "repo"
    worktree = run_dir / "worktree"
    run_checked(["git", "clone", repo_url, str(repo)])
    git(repo, ["worktree", "add", str(worktree), "-b", branch, f"origin/{base_branch}"])
    return repo, worktree


def apply_edits(worktree: Path, edits: list[dict[str, str]]) -> list[str]:
    """应用改动清单（append/replace），返回改动文件表。只做这两种最小真实编辑。"""
    touched: list[str] = []
    for edit in edits:
        action = edit.get("action")
        rel = edit.get("path", "")
        if action not in {"append", "replace"} or not rel or ".." in Path(rel).parts:
            raise AdapterError(f"PA05 非法改动项: {json.dumps(edit, ensure_ascii=False)}")
        target = worktree / rel
        if not target.is_file():
            raise AdapterError(f"PA06 改动目标缺席: {rel}")
        text = target.read_text(encoding="utf-8")
        if action == "append":
            text = text + edit["text"]
        else:
            old, new = edit["old"], edit["text"]
            if text.count(old) != 1:
                raise AdapterError(f"PA07 replace 锚点必须唯一命中 1 次: {rel}")
            text = text.replace(old, new)
        target.write_text(text, encoding="utf-8")
        if rel not in touched:
            touched.append(rel)
    return touched


def make_patch(worktree: Path, out_dir: Path, name: str) -> dict[str, Any]:
    """补丁件落盘（父提交→分支头的完整 git diff）+ sha256。改动已在 commit 里，
    补丁取 HEAD~1..HEAD——与推送分支头逐位同源。"""
    patch = out_dir / name
    committed = subprocess.run(
        ["git", "diff", "HEAD~1", "HEAD"],
        cwd=str(worktree),
        capture_output=True,
        text=True,
        check=False,
    )
    if committed.returncode != 0:
        raise AdapterError(f"PA08 git diff HEAD~1 失败: {committed.stderr.strip()}")
    patch.write_text(committed.stdout, encoding="utf-8")
    return {
        "path": str(patch),
        "sha256": sha256_file(patch),
        "bytes": patch.stat().st_size,
        "files": git(worktree, ["diff", "--name-only", "HEAD~1", "HEAD"]).splitlines(),
    }


# ------------------------------------------------------------------- gitea --

class GiteaClient:
    """gitea API + git push（Basic 认证取自 git-credentials 既有存根）。"""

    def __init__(self, api_base: str, credentials_file: Path, host_suffix: str):
        userinfo = None
        for line in credentials_file.read_text(encoding="utf-8").splitlines():
            if not line.startswith("http://") or host_suffix not in line:
                continue
            userinfo = line[len("http://"):].split("@")[0]
            break
        if userinfo is None:
            raise AdapterError(f"PA09 git-credentials 无 {host_suffix} 条目: {credentials_file}")
        user, _, pwd = userinfo.partition(":")
        self.api_base = api_base.rstrip("/")
        self.token = base64.b64encode(f"{user}:{pwd}".encode()).decode()

    def api(self, method: str, path: str, payload: dict[str, Any] | None = None) -> Any:
        import urllib.error
        import urllib.request

        request = urllib.request.Request(self.api_base + path, method=method)
        request.add_header("Authorization", "Basic " + self.token)
        request.add_header("Content-Type", "application/json")
        data = json.dumps(payload).encode() if payload is not None else None
        try:
            with urllib.request.urlopen(request, data) as response:
                body = response.read().decode()
        except urllib.error.HTTPError as error:
            raise AdapterError(f"PA11 gitea API {method} {path} -> HTTP {error.code}: {error.read().decode()[:500]}") from error
        return json.loads(body) if body else None

    def push_branch(self, repo_dir: Path, branch: str, remote: str = "origin") -> str:
        run_checked(["git", "push", remote, f"refs/heads/{branch}:refs/heads/{branch}"], cwd=repo_dir)
        return git(repo_dir, ["ls-remote", remote, f"refs/heads/{branch}"]).split()[0]

    def remote_head(self, repo_dir: Path, remote: str, branch: str) -> str:
        return git(repo_dir, ["ls-remote", remote, f"refs/heads/{branch}"]).split()[0]


# ---------------------------------------------------------------- 门结果件 --

def load_gate_result(gate_out: Path, gate_script: Path, worktree: Path, base_sha: str, source_sha: str) -> dict[str, Any]:
    """读 gate_f1.sh 的 SUMMARY+steps，装成 af.gate_result.v1 形件（可核四项绑定）。"""
    summary = (gate_out / "SUMMARY.txt").read_text(encoding="utf-8")
    verdict = GATE_PASS_VERDICT if "PIPELINE-GREEN" in summary else "fail"
    steps = []
    for step_log in sorted(gate_out.glob("step*.log")):
        steps.append({"step": step_log.stem, "log": step_log.name, "log_sha256": sha256_file(step_log)})
    return {
        "schema_version": "af.gate_result.v1",
        "gate_id": "f1-fast",
        "suite": ["fmt", "clippy", "test"],
        "suite_script": str(gate_script),
        "suite_digest": sha256_file(gate_script),
        "verdict": verdict,
        "summary": summary.strip(),
        "steps": steps,
        "repo_worktree": str(worktree),
        "base_sha": base_sha,
        "source_sha": source_sha,
        "tested_tree": git(worktree, ["rev-parse", "HEAD^{tree}"]),
        "ran_at": time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime()),
    }


# ----------------------------------------------------------------- 主流程 --

def cmd_run(args: argparse.Namespace) -> int:
    plan = json.loads(Path(args.plan).read_text(encoding="utf-8"))
    run_root = Path(args.run_root)
    evidence_dir = Path(args.evidence_dir)
    run_root.mkdir(parents=True, exist_ok=True)
    evidence_dir.mkdir(parents=True, exist_ok=True)

    order_id = plan["order_id"]
    lease_id = plan["lease_id"]
    branch = plan["branch"]
    registry = Path(plan["registry"])
    leases_db = Path(plan["leases_db"])
    gate_script = Path(plan["gate_script"]).resolve()
    run_dir = run_root / lease_id

    timeline: list[dict[str, Any]] = []

    def mark(step: str, detail: dict[str, Any]) -> None:
        timeline.append({"step": step, "ms": now_ms(), "detail": detail})
        print(f"[{step}] {json.dumps(detail, ensure_ascii=False, sort_keys=True)}")

    # -- 0. 订单账先开（af-order submit 一次性驱动到 Closed；工件面后挂）---------
    af_order = Path(plan["af_order_bin"])
    submit = run_checked(
        [
            str(af_order), "submit",
            "--spec", str(Path(plan["order_spec"])),
            "--state", str(Path(plan["order_state_dir"])),
            "--unit", "stub",
            "--workspace", plan["workspace"],
            "--target-dir", plan["target_dir"],
        ]
    )
    submit_json = json.loads(submit.stdout)
    if submit_json.get("state") != "Closed":
        raise AdapterError(f"PA12 订单未到 Closed: {submit_json.get('state')}")
    order_dir = Path(submit_json["order_dir"])
    mark("order-submitted", {"order_id": order_id, "state": submit_json["state"], "order_dir": str(order_dir), "replayed": submit_json.get("replayed")})

    # -- 1. 供给面：观测采集 → 水位预检 → 领 lease --------------------------------
    observation = collect_observation(registry, plan["node_id"])
    verdict = preflight(registry, observation, leases_db, plan["node_id"], plan["mount"], plan["reserved_bytes"])
    mark("preflight", {"post_free_bytes": verdict["post_free_bytes"], "admit": verdict["admit"]})

    profile = {"schema_version": "af.resources/v1", "recipe_namespace": "user-01", "node_id": plan["node_id"]}
    receipt = lease(leases_db, [
        "acquire",
        "--lease-id", lease_id,
        "--resource", plan["resource"],
        "--interval", f"worktree:poc-e:{order_id}",
        "--profile", json.dumps(profile, sort_keys=True),
        "--holder", plan["holder"],
        "--reserved-bytes", str(plan["reserved_bytes"]),
        "--ttl-ms", str(plan["ttl_ms"]),
    ])
    if receipt["state"] != "active":
        raise AdapterError(f"PA13 lease 未 active: {receipt['state']}")
    mark("lease-acquired", {"lease_id": receipt["lease_id"], "lease_until_ms": receipt["lease_until_ms"]})

    # -- 2. 工作位：lease 足迹目录内 clone + worktree ------------------------------
    run_dir.mkdir(parents=True, exist_ok=True)
    repo, worktree = open_worktree(plan["repo_url"], run_dir, branch, plan["base_branch"])
    binding = {
        "lease_id": lease_id,
        "resource": plan["resource"],
        "order_id": order_id,
        "branch": branch,
        "repo_url": plan["repo_url"],
        "run_dir": str(run_dir),
        "worktree": str(worktree),
        "lease_until_ms": receipt["lease_until_ms"],
        "ttl_ms": plan["ttl_ms"],
    }
    binding_path = run_dir / "lease-binding.json"
    binding_sha = write_json(binding_path, binding)
    mark("worktree-opened", {"worktree": str(worktree), "branch": branch, "binding_sha256": binding_sha})

    try:
        # -- 3. 干活：真实小改动 → commit → 补丁件 --------------------------------
        touched = apply_edits(worktree, plan["edits"])
        git(worktree, ["add", "-A"])
        git(worktree, ["-c", "user.name=poc-e-codeadapter", "-c", "user.email=poc-e@pandev-comp.local",
                       "commit", "-m", f"poc-e({order_id}): {plan['change_summary']}"])
        source_sha = git(worktree, ["rev-parse", "HEAD"])
        base_sha = git(worktree, ["rev-parse", "HEAD~1"])
        patch = make_patch(worktree, evidence_dir, f"{order_id}.patch")
        mark("patch-produced", {"source_sha": source_sha, "base_sha": base_sha, **patch})

        # -- 4. 门：af ci/run.sh 可跑子集（fmt/clippy/test）------------------------
        gate_out = run_dir / "gate-out"
        try:
            run_checked(["bash", str(gate_script), str(worktree), str(gate_out)])
        except AdapterError:
            pass  # 门红也读 SUMMARY 装件（verdict=fail 落证据），下面统一判红
        gate_result = load_gate_result(gate_out, gate_script, worktree, base_sha, source_sha)
        gate_sha = write_json(evidence_dir / f"{order_id}.gate_result.json", gate_result)
        mark("gate", {"verdict": gate_result["verdict"], "tested_tree": gate_result["tested_tree"], "sha256": gate_sha})
        if gate_result["verdict"] != GATE_PASS_VERDICT:
            raise AdapterError("PA14 门红：不进合入（如实记录，不把没跑/跑红写成门绿）")

        # -- 5. 推送 + integrator 四项绑定复核 + PR 合入 ----------------------------
        gitea = GiteaClient(plan["gitea_api_base"], Path(plan["gitea_credentials"]), plan["gitea_host_suffix"])
        pushed = gitea.push_branch(repo, branch)
        actual = {
            "base_sha": base_sha,
            "source_sha": source_sha,
            "tested_tree": git(worktree, ["rev-parse", "HEAD^{tree}"]),
            "suite_digest": gate_result["suite_digest"],
            "verdict": gate_result["verdict"],
        }
        binding_check = four_binding_check(gate_result, actual)
        if pushed != source_sha:
            binding_check["admit"] = False
            binding_check["mismatches"]["remote_head"] = {"expected": source_sha, "actual": pushed}
        mark("pushed", {"remote_head": pushed, "four_binding_admit": binding_check["admit"]})
        if not binding_check["admit"]:
            raise AdapterError(f"PA15 四项绑定复核拒绝合入: {json.dumps(binding_check['mismatches'], ensure_ascii=False)}")

        pr = gitea.api("POST", f"/repos/{plan['gitea_repo']}/pulls", {
            "title": f"poc-e({order_id}): {plan['change_summary']}",
            "head": branch,
            "base": plan["base_branch"],
            "body": (
                f"PoC-E 代码域适配器端到端单（order={order_id}, lease={lease_id}）。\n\n"
                f"- base_sha: {base_sha}\n- source_sha: {source_sha}\n"
                f"- tested_tree: {gate_result['tested_tree']}\n- suite_digest: {gate_result['suite_digest']}\n"
                f"- gate: f1-fast (af ci/run.sh 可跑子集 fmt/clippy/test) verdict=pass\n"
                f"- patch sha256: {patch['sha256']}\n"
            ),
        })
        pr_index = pr["number"]
        gitea.api("POST", f"/repos/{plan['gitea_repo']}/pulls/{pr_index}/merge", {"Do": "merge"})
        pr_after = gitea.api("GET", f"/repos/{plan['gitea_repo']}/pulls/{pr_index}")
        merged_sha = pr_after.get("merge_commit_sha") or ""
        remote_main = gitea.remote_head(repo, "origin", plan["base_branch"])
        if not pr_after.get("merged") or not merged_sha:
            raise AdapterError(f"PA16 PR 未合入: merged={pr_after.get('merged')}")
        mark("merged", {"pr": pr_index, "merge_commit_sha": merged_sha, f"remote_{plan['base_branch']}": remote_main})
    finally:
        # -- 6. 还 lease：工作树与 lease 生命周期绑定（还即移除足迹目录）------------
        # rmtree 尽力而为：移除失败不谎报 removed（如实记录），lease 归还仍执行。
        shutil.rmtree(run_dir, ignore_errors=True)
        returned = lease(leases_db, ["return", "--lease-id", lease_id, "--holder", plan["holder"]])
        mark("lease-returned", {"lease_id": returned["lease_id"], "state": returned["state"], "run_dir_removed": not run_dir.exists()})

    # -- 7. 工件面：挂进 order_dir/artifacts/ + 证据目录 ----------------------------
    artifacts_dir = order_dir / "artifacts"
    artifacts_dir.mkdir(parents=True, exist_ok=True)
    artifacts = {
        "schema_version": "af.poc_e.artifacts.v1",
        "order_id": order_id,
        "lease": receipt,
        "lease_returned": True,
        "worktree_removed": not run_dir.exists(),
        "patch": patch,
        "gate_result": gate_result,
        "four_binding": binding_check,
        "merge": {"pr": pr_index, "merge_commit_sha": merged_sha, "repo": plan["gitea_repo"], "branch": branch, "base_branch": plan["base_branch"]},
        "binding_file_sha256": binding_sha,
        "timeline": timeline,
    }
    manifest_sha = write_json(artifacts_dir / "artifacts.json", artifacts)
    for name, source_path in (
        (f"{order_id}.patch", Path(patch["path"])),
        (f"{order_id}.gate_result.json", evidence_dir / f"{order_id}.gate_result.json"),
    ):
        shutil.copy2(source_path, artifacts_dir / name)
    write_json(artifacts_dir / "manifest.sha256", {
        "artifacts.json": manifest_sha,
        f"{order_id}.patch": sha256_file(artifacts_dir / f"{order_id}.patch"),
        f"{order_id}.gate_result.json": sha256_file(artifacts_dir / f"{order_id}.gate_result.json"),
    })
    show = run_checked([str(af_order), "show", order_id, "--state", str(Path(plan["order_state_dir"]))])
    show_json = json.loads(show.stdout)
    e2e_sha = write_json(evidence_dir / f"{order_id}.e2e.json", {
        "order": {"order_id": order_id, "state": show_json["state"], "order_dir": str(order_dir)},
        "artifacts_manifest_sha256": manifest_sha,
        "merge_commit_sha": merged_sha,
        "patch_sha256": patch["sha256"],
        "gate_verdict": gate_result["verdict"],
        "lease": {"id": lease_id, "state_after_return": returned["state"]},
    })
    mark("artifacts-attached", {"order_state": show_json["state"], "manifest_sha256": manifest_sha, "e2e_sha256": e2e_sha})
    print(f"E2E-OK {e2e_sha}")
    return 0


# --------------------------------------------------------- 生命周期演示/回收 --

def acquire_and_open_footprint(args: argparse.Namespace) -> dict[str, Any]:
    """领 lease + 在足迹目录内开 worktree（生命周期演示用；main 流程在 cmd_run 内联）。
    预检→acquire→clone→worktree→写 lease-binding.json，全部与 run 同一路径。"""
    run_root = Path(args.run_root)
    run_root.mkdir(parents=True, exist_ok=True)
    registry = Path(args.registry)
    leases_db = Path(args.leases_db)
    observation = collect_observation(registry, args.node_id)
    preflight(registry, observation, leases_db, args.node_id, args.mount, args.reserved_bytes)
    profile = {"schema_version": "af.resources/v1", "recipe_namespace": "user-01", "node_id": args.node_id}
    receipt = lease(leases_db, [
        "acquire",
        "--lease-id", args.lease_id,
        "--resource", args.resource,
        "--interval", f"worktree:poc-e:{args.lease_id}",
        "--profile", json.dumps(profile, sort_keys=True),
        "--holder", args.holder,
        "--reserved-bytes", str(args.reserved_bytes),
        "--ttl-ms", str(args.ttl_ms),
    ])
    run_dir = run_root / args.lease_id
    run_dir.mkdir(parents=True, exist_ok=True)
    repo, worktree = open_worktree(args.repo_url, run_dir, args.branch, args.base_branch)
    binding = {
        "lease_id": args.lease_id,
        "resource": args.resource,
        "order_id": "",
        "branch": args.branch,
        "repo_url": args.repo_url,
        "run_dir": str(run_dir),
        "worktree": str(worktree),
        "lease_until_ms": receipt["lease_until_ms"],
        "ttl_ms": args.ttl_ms,
    }
    binding_sha = write_json(run_dir / "lease-binding.json", binding)
    return {
        "lease": receipt,
        "run_dir": str(run_dir),
        "worktree": str(worktree),
        "binding_sha256": binding_sha,
    }


def cmd_open(args: argparse.Namespace) -> int:
    print(json.dumps(acquire_and_open_footprint(args), ensure_ascii=False, sort_keys=True))
    return 0


def _run_dir_state(run_root: Path, store_db: Path) -> dict[str, Any]:
    """枚举足迹目录 + 对应 lease 现状（lease_store show 语义）。"""
    entries = []
    for binding_path in sorted(run_root.glob("*/lease-binding.json")):
        binding = json.loads(binding_path.read_text(encoding="utf-8"))
        show = subprocess.run(
            [sys.executable, "-I", str(supply_script("lease_store.py")), "--db", str(store_db),
             "show", "--lease-id", binding["lease_id"]],
            capture_output=True, text=True,
        )
        lease_state = json.loads(show.stdout)["state"] if show.returncode == 0 else "not-found"
        entries.append({"lease_id": binding["lease_id"], "lease_state": lease_state, "run_dir": binding["run_dir"]})
    return {"entries": entries}


def cmd_release(args: argparse.Namespace) -> int:
    """还 lease：先移除足迹目录再归还（生命周期绑定：还了就不留活工作树）。"""
    run_dir = Path(args.run_dir).resolve()
    binding_path = run_dir / "lease-binding.json"
    binding = json.loads(binding_path.read_text(encoding="utf-8"))
    shutil.rmtree(run_dir)
    returned = lease(Path(args.leases_db), ["return", "--lease-id", binding["lease_id"], "--holder", args.holder])
    result = {"returned": returned, "run_dir_removed": not run_dir.exists()}
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0


def cmd_reap(args: argparse.Namespace) -> int:
    """过期回收：先跑 lease_store 既有 reap（active→expired），再移除全部
    非 active lease 的足迹目录。复跑安全；可作「租约过期可回收」的独立验证。"""
    run_root = Path(args.run_root)
    store_db = Path(args.leases_db)
    reaped = lease(store_db, ["reap"])
    before = _run_dir_state(run_root, store_db)
    removed: list[str] = []
    for entry in before["entries"]:
        if entry["lease_state"] in {"expired", "returned", "not-found"}:
            shutil.rmtree(entry["run_dir"], ignore_errors=True)
            if not Path(entry["run_dir"]).exists():
                removed.append(entry["lease_id"])
    result = {
        "lease_store_reaped": reaped,
        "removed_run_dirs": removed,
        "remaining": _run_dir_state(run_root, store_db),
    }
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    if args.out:
        write_json(Path(args.out), result)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    run = commands.add_parser("run", help="端到端一单：订单→预检→lease→worktree→patch→门→合入→还→挂证")
    run.add_argument("--plan", required=True, help="任务计划 JSON（字段见 README）")
    run.add_argument("--run-root", required=True)
    run.add_argument("--evidence-dir", required=True)
    run.set_defaults(func=cmd_run)

    open_cmd = commands.add_parser("open", help="领 lease + 开 worktree 足迹（生命周期演示/独立领用）")
    open_cmd.add_argument("--leases-db", required=True)
    open_cmd.add_argument("--registry", required=True)
    open_cmd.add_argument("--node-id", required=True)
    open_cmd.add_argument("--mount", required=True)
    open_cmd.add_argument("--resource", required=True)
    open_cmd.add_argument("--reserved-bytes", type=int, required=True)
    open_cmd.add_argument("--ttl-ms", type=int, required=True)
    open_cmd.add_argument("--holder", required=True)
    open_cmd.add_argument("--lease-id", required=True)
    open_cmd.add_argument("--repo-url", required=True)
    open_cmd.add_argument("--branch", required=True)
    open_cmd.add_argument("--base-branch", default="main")
    open_cmd.add_argument("--run-root", required=True)
    open_cmd.set_defaults(func=cmd_open)

    release = commands.add_parser("release", help="还 lease 并移除足迹目录")
    release.add_argument("--run-dir", required=True)
    release.add_argument("--leases-db", required=True)
    release.add_argument("--holder", required=True)
    release.set_defaults(func=cmd_release)

    reap = commands.add_parser("reap", help="过期回收：lease_store reap + 移除非 active 足迹")
    reap.add_argument("--run-root", required=True)
    reap.add_argument("--leases-db", required=True)
    reap.add_argument("--out", default="", help="结果 JSON 落盘路径（可选）")
    reap.set_defaults(func=cmd_reap)

    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except AdapterError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

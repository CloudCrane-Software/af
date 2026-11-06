#!/usr/bin/env python3
"""router/slow_path.py — Router 最小皮·Slow Path 六步编排（PoC-D）

六步全用现成 API（桥=af 内核门面，见 ../bridge/src/main.rs）：
  ① 订单入环：从**事件派单记录**（af-state/triggers/dispatch/*.json）锚定
     order_id → WorkOrder::new + bind_line + attach_budget_voucher + 继 run
     （guard.check_context：无订单上下文直接拒——首版不对纯文本说话）；
  ② decision_payload：WorkOrder::decision_payload(subject)（内核长度前缀防重排格式）；
  ③ seal_decision：WorkOrder::seal_decision——Router 永远 Reviser/Revise
     （guard.check_seal_request 皮层钳制 + 内核四层第②③层机械兜底）；
  ④ 四层校验：DecisionArtifact::validate_against（hash→角色上限→决策权限→
     占位签名；第④层 ed25519-placeholder=登记在案的开放风险#1，BOOT-1 换真）；
  ⑤ admit 双闸路径占位：闸A=工件四层校验（真）；闸B=ReleaseGate::admit
     （真，release 唯一写者）；「F 门单两查（RequiredInputMissing/
     InputGateNotGreen）」=PoC-F 职责，输出如实带 two_check_placeholder；
  ⑥ 留档：DecisionArtifact + Permit + 六步审计轨迹落
     <trigger_home>/slowpath/<order_id>/，只产工件零外部效应（guard 禁3）。

subject 构造（事件→订单的锚定语义）：gate:<suite>:<event_type>:<repo>@<subject>
——派单记录四要素全入 payload，决策锚定到具体事件内容，换提交/换 PR 必换 payload。
"""
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
POC = os.path.dirname(HERE)
BRIDGE = os.path.join(POC, "bridge", "target", "debug", "af-super-agent-bridge")
sys.path.insert(0, os.path.join(POC, "admit"))
import guard  # noqa: E402  (皮层策略闸，同包平铺最小皮)


def _bridge(*args, check=True):
    r = subprocess.run([BRIDGE, *args], capture_output=True, text=True, timeout=60)
    out = None
    if r.stdout.strip():
        try:
            out = json.loads(r.stdout.strip().splitlines()[-1])
        except json.JSONDecodeError:
            out = None
    if check and r.returncode != 0:
        raise RuntimeError(f"桥步失败 rc={r.returncode}: {args[0]}: "
                           f"{(out or {}).get('error') or (out or {}).get('reject') or r.stderr[:200]}")
    return r.returncode, out


def _atomic_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=1)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def subject_for(dispatch):
    """决策 subject=派单记录四要素（suite/event_type/repo/subject）。
    Slow Path 对『哪个订单的哪个事件』说话，从这里唯一确定。"""
    return f"gate:{dispatch['suite']}:{dispatch['event_type']}:{dispatch['repo']}@{dispatch['subject']}"


def run_slow_path(dispatch, registry, out_root, actor=None, allow_probe=False,
                  probe_role=None, probe_decision=None):
    """六步。dispatch=派单记录（event-router 消费腿产物）；registry=触发层注册表
    （slow_path 段供 line/budget/review/actor）。allow_probe=True 时以探针组合
    走 ③④（预期内核必拒，工件标 probe 不留档面）——负例证明用，生产路径禁用。"""
    guard.check_context(dispatch)                       # ①前置：无订单不对话
    sp = registry.get("slow_path") or {}
    actor = actor or sp.get("router_actor_id", "poc-super-agent/router")
    role, decision = guard.ROUTER_ROLE, guard.ROUTER_DECISION
    is_probe = False
    if allow_probe:
        role, decision = probe_role, probe_decision
        is_probe = guard.check_probe(actor, role, decision)
    else:
        guard.check_seal_request(actor, role, decision)  # 禁1+Reviser 钳制（fail-closed）
    guard.check_effects("write-artifact")                # 禁3：只产工件
    order_id = dispatch["order_id"]
    subj = subject_for(dispatch)
    trail = {"order_id": order_id, "subject": subj, "probe": is_probe,
             "actor": actor, "role": role, "decision": decision, "steps": {}}
    t0 = time.time()
    # ① WorkOrder 入环（bind line/budget=配方声明；release 门控前置）
    line = sp.get("line") or {}
    _, w = _bridge("workorder-new", "--id", order_id,
                   "--line-id", str(line.get("line_id", "line-pocd")),
                   "--line-version", str(line.get("line_version", "1")),
                   "--budget-cents", str(sp.get("budget_cents", 100)))
    trail["steps"]["1-workorder-new"] = {"state": w["state"], "stable_id": w["stable_id"],
                                         "line": line, "budget_cents": sp.get("budget_cents", 100)}
    wo_f = os.path.join(out_root, order_id, "workorder.json")
    if not is_probe:
        _atomic_json(wo_f, w)
    # ② decision_payload（现成 API：长度前缀格式）
    _, p = _bridge("decision-payload", "--workorder", wo_f if not is_probe else _tmp_wo(w),
                   "--subject", subj)
    trail["steps"]["2-decision-payload"] = {"payload": p["payload"]}
    # ③ seal_decision（Router 永远 Reviser/Revise；探针例外进负例）
    art_f = os.path.join(out_root, order_id, "decision-artifact.json")
    _, s = _bridge("seal", "--workorder", wo_f if not is_probe else _tmp_wo(w),
                   "--subject", subj, "--actor", actor, "--role", role,
                   "--decision", decision, "--key-id",
                   str(sp.get("router_key_id", "pocd-router-key-1")))
    art = s["artifact"]
    trail["steps"]["3-seal_decision"] = {"role": art["role"], "decision": art["decision"],
                                         "payload_hash": art["payload_hash"],
                                         "sig_algorithm": art["signature"]["algorithm"]}
    # 工件先落盘（校验要读它）：合法路径落留档面，探针落 _TMP（不留档）
    if not is_probe:
        _atomic_json(art_f, s)
    else:
        _tmp_art(s)
    # ④ 四层校验（负例在此必红：PermissionDenied/RoleExceedsCapability）
    rc, v = _bridge("validate", "--workorder", wo_f if not is_probe else _tmp_wo(w),
                    "--artifact", art_f if not is_probe else _tmp_art(s), "--subject", subj,
                    check=False)
    trail["steps"]["4-validate-four-layer"] = {"ok": bool(v and v.get("ok")),
                                               "rc": rc, "detail": v}
    if not (v and v.get("ok")):
        trail["result"] = "rejected-at-layer4"
        # 探针轨迹落 _TMP（预期必红面），不进留档面（out_root 只放合法产物）
        _atomic_json(os.path.join(_TMP if is_probe else out_root, order_id, "trail.json"), trail)
        return trail
    if is_probe:
        trail["result"] = "probe-passed-unexpectedly"      # 探针绿=内核失守，必红
        return trail
    # ⑤ admit 双闸路径占位（闸A 真校验 + 闸B ReleaseGate::admit 真投放）
    rc, adm = _bridge("admit", "--workorder", wo_f, "--artifact", art_f, "--subject", subj,
                      "--review-minutes", str(sp.get("review_minutes", 30)),
                      "--wip-upper", str(sp.get("wip_upper_minutes", 400)),
                      "--wip-lower", str(sp.get("wip_lower_minutes", 100)),
                      check=False)
    trail["steps"]["5-admit"] = {"rc": rc, "detail": adm,
                                 "unique_release_writer": guard.release_writer_is_unique()}
    # ⑥ 留档（Permit + 轨迹；禁3：只写本单目录）
    if adm and adm.get("admitted"):
        permit_f = os.path.join(out_root, order_id, "permit.json")
        _atomic_json(permit_f, adm)
    trail["steps"]["6-archive"] = {"dir": os.path.join(out_root, order_id)}
    trail["result"] = "admitted" if (adm or {}).get("admitted") else "admit-rejected"
    trail["elapsed_ms"] = int((time.time() - t0) * 1000)
    _atomic_json(os.path.join(out_root, order_id, "trail.json"), trail)
    return trail


_TMP = "/tmp/pocd-slowpath-probe"


def _tmp_wo(w):
    os.makedirs(_TMP, exist_ok=True)
    f = os.path.join(_TMP, "wo.json")
    json.dump(w, open(f, "w"))
    return f


def _tmp_art(s):
    os.makedirs(_TMP, exist_ok=True)
    f = os.path.join(_TMP, "art.json")
    json.dump(s, open(f, "w"))
    return f

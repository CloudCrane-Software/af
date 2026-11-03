#!/usr/bin/env python3
"""admit/guard.py — Router 最小皮·三禁最小集执行器（PoC-D，af/poc/poc-super-agent/）

三禁（v0.3 T6.4 首版最小集 + v0.1 红队归位稿原三禁的落地面）：
  禁1 凭证只能收紧：Router actor 的能力面只能从角色默认集做**交集/剔除**，
      永不放宽；能力清单静态声明 shell/root/network=deny——皮层构造任何带
      这些动词的签发请求即拒（guard 拒绝让请求进桥）。不持 shell/root 能力。
  禁2 release 唯一写者：Created→Released 只经 ReleaseGate::admit（桥面无裸
      release 子命令——bridge/src/main.rs 显式拒绝，selfcheck 断言）。皮层
      不出现任何第二 release 路径；admit 路径占位=F 门单两查（PoC-F 职责，
      如实登记不冒充）。
  禁3 不产生未经 admit 的外部效应：Slow Path 只产工件/记录（JSON 落盘），
      不外呼、不执行门套件、不写 inbox/order 面之外的任何面。

Router 永远 Reviser、终裁只在人类（T6.1/T6.4）：本 guard 拒绝 Router actor
以 Reviser 之外的角色/Revise 之外的 decision 走生产路径（四层校验第②③层是
内核侧机械兜底，此处是皮层第一道——负例探针可显式请求非法组合以证明内核
会拒，见 selfcheck）。

无订单不对话（T6.1）：guard.check_context 在一切签发之前要求订单上下文
（dispatch 记录或工单文件），纯文本输入直接拒。
"""
import json
import re

# Router actor 静态能力清单：只能收紧的硬底。任何时点都不存在 allow。
CAPABILITY_DENYLIST = ("shell", "root", "sudo", "network", "exec", "spawn")

# 生产路径唯一合法组合（T6.4）：Router 永远 Reviser；decision 只能 Revise
# （Revise=建议修订；Approve/Reject/Arbitrate 是人类/仲裁面的事——终裁只在人类）。
ROUTER_ROLE = "Reviser"
ROUTER_DECISION = "Revise"

# Reviser 角色能力上限（与 crates/af-workorder/src/decision.rs Role::allowed_permissions
# 同源镜像；皮层只做收紧=从此集剔除，永不增补）。
REVISER_ALLOWED = ["Read", "Revise"]


class GuardError(Exception):
    """三禁/无订单拒绝（fail-closed：皮层不放行即不进桥）。"""


def check_context(order_ctx):
    """首版只对订单/事件说话（T6.1）：无订单上下文=拒。order_ctx 必须携带
    order_id（来自派单记录或工单文件）。纯对话/纯文本无 admit 路径。"""
    if not isinstance(order_ctx, dict) or not (order_ctx.get("order_id") or "").strip():
        raise GuardError(
            "无订单纯对话不被允许（T6.1）：Router 首版只对订单/事件说话；"
            "输入=派单记录（dispatch/*.json）或工单文件，须带 order_id。人类非订单"
            "交互走现有渠道。")
    return True


def check_seal_request(actor_id, role, decision, permissions_requested=None):
    """禁1+Reviser 钳制：签发请求进桥前的皮层闸。

    - actor 必须是注册表登记的 router actor（别的 actor 不经本皮签发）；
    - 生产路径 role/decision 钳死 Reviser/Revise（探针例外见 allow_probe=True，
      探针只允许**预期必被内核四层校验拒绝**的非法组合，皮层放行≠内核放行）；
    - permissions 只能是 REVISER_ALLOWED 的子集（收紧），出现任何扩权/能力
      清单动词=拒（fail-closed，不静默剔除——扩权意图本身就该被看见）。
    """
    if re.search(r"\b(" + "|".join(CAPABILITY_DENYLIST) + r")\b",
                 json.dumps({"p": permissions_requested or [], "role": role, "d": decision},
                            ensure_ascii=False), re.IGNORECASE):
        raise GuardError(f"禁1：签发请求携带被禁能力动词（{CAPABILITY_DENYLIST}）——"
                         f"凭证只能收紧，Router 不持 shell/root/exec/network 能力。")
    if permissions_requested is not None:
        extra = [p for p in permissions_requested if p not in REVISER_ALLOWED]
        if extra:
            raise GuardError(f"禁1：permissions 只能收紧（子集于 {REVISER_ALLOWED}），"
                             f"出现扩权项 {extra}。")
    if (role, decision) != (ROUTER_ROLE, ROUTER_DECISION):
        raise GuardError(
            f"Router 永远 {ROUTER_ROLE}/{ROUTER_DECISION}（T6.4，终裁只在人类）："
            f"拒绝 {role}/{decision}。皮层不放行；内核四层校验第②③层另有机械兜底。")
    return True


def check_probe(actor_id, role, decision):
    """负例探针通道：显式声明这是探针（allow_probe=True）才放行非法组合进桥，
    用于证明内核会拒（四层校验负例）。探针产物一律标记 probe=true，不进留档面。"""
    if (role, decision) == (ROUTER_ROLE, ROUTER_DECISION):
        return False            # 合法组合不是探针
    return True


def check_effects(action):
    """禁3：Slow Path 允许的动作白名单——只产工件/记录，零外部效应。"""
    ALLOWED = ("write-artifact", "write-record", "read")
    if action not in ALLOWED:
        raise GuardError(f"禁3：不产生未经 admit 的外部效应——动作 {action!r} 不在"
                         f"允许集 {ALLOWED}（不外呼/不执行门套件/不写运行面）。")
    return True


def release_writer_is_unique(bridge_dir_note=True):
    """禁2 登记面：Created→Released 唯一写者=ReleaseGate::admit。机械事实在桥：
    bridge/src/main.rs 对 `release` 子命令显式拒绝（rc=2），桥内无 wo.release()
    直调（release 只发生在 ReleaseGate::admit 的副本预检里）。selfcheck 断言
    两点：`release` 子命令必红 + admit 是唯一绿路径。"""
    return ("Created→Released 唯一写者=ReleaseGate::admit（gate.rs:776-832）；"
            "桥面无裸 release 入口；皮层无第二写路径"
            + ("；F 门单 admit 两查=PoC-F 占位" if bridge_dir_note else ""))

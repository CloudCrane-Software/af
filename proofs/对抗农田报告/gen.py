#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""afv1-farm 生成器 v1.0（seed=20261024，可复现）。

三策略：
  A 合法基线（10 份）——对照组。
  B 系统性变异：字段类型翻转 / 边界值 / 空集 / 嵌套深化 / Unicode / 超长标识符 / YAML 怪癖。
  C 定向攻击：逐不变量（路径别名多写者 / 上下文段重复引用 / 自验证 / eval_input 泄漏 /
    route / CEL 子集边界 / 逃逸数据流 / 信封语义）。
  D 随机组合爆破：seed 可复现，mutation 清单入 manifest。

用法：python3 gen.py <outdir>
manifest.jsonl 每份一行：{file, group, strategy, expect, note}
expect ∈ PASS | REJECT | null(探针)。裁定与 expect 不一致 → 分诊队列。
"""
import copy
import json
import os
import random
import sys

import yaml

sys.setrecursionlimit(100000)

SEED = 20261024
GEN_VERSION = "afv1-farm-gen/1.0 (seed=20261024; pyyaml-6.0.3)"
U64MAX = 18446744073709551615
U32MAX = 4294967295

SAMPLES = []  # (name, payload(dict|str), meta)


def add(name, payload, expect=None, note="", strategy=""):
    SAMPLES.append((name, payload, {"expect": expect, "note": note, "strategy": strategy}))


def dump(obj) -> str:
    return yaml.safe_dump(obj, sort_keys=False, allow_unicode=True,
                          default_flow_style=False, width=10 ** 6)


# ---------------------------------------------------------------- 基线骨架

def t_in():
    return {"name": "T_in", "schema": {"type": "object", "properties": {
        "q": {"type": "string"},
        "tags": {"type": "array", "items": {"type": "string"}}}}}


def t_out(name="T_out"):
    return {"name": name, "schema": {"type": "object", "properties": {"r": {"type": "string"}}}}


def t_item():
    return {"name": "T_item", "schema": {"type": "object", "properties": {"x": {"type": "string"}}}}


def sec_input(fields=("q",), quota=100, **kw):
    s = {"from": "input", "fields": list(fields), "quota_tokens": quota}
    s.update(kw)
    return s


def sec_store(store, policy="full", quota=200, **kw):
    s = {"from": "store", "store": store, "policy": policy, "quota_tokens": quota}
    s.update(kw)
    return s


def sec_arg(name="item", quota=50, **kw):
    s = {"from": "arg", "name": name, "quota_tokens": quota}
    s.update(kw)
    return s


def sec_edge(port, quota=80, **kw):
    s = {"from": "edge", "port": port, "quota_tokens": quota}
    s.update(kw)
    return s


def human_decl():
    return {"station": "st1", "role": "reviewer",
            "resume": {"schema": {"type": "object",
                                  "properties": {"approved": {"type": "boolean"}}}}}


def store_decl(name, writer, stype="kv", eval_input=None, retention=None, **kw):
    d = {"name": name, "type": stype, "writer": writer, "lifecycle": "run",
         "retention": retention or {"max_items": 16}}
    if eval_input is not None:
        d["eval_input"] = eval_input
    d.update(kw)
    return d


def node(id_, kind="task", **kw):
    d = {"id": id_, "kind": kind}
    d.update(kw)
    return d


def doc(meta=None, types=None, stores=None, nodes=None, edges=None, policies=None):
    d = {
        "version": 1,
        "meta": {"name": "farm_g", "graph_version": "1", "entry": "n1",
                 "input": "T_in", "output": {"ref": "T_out", "from": "n1"}},
        "types": types if types is not None else [t_in(), t_out()],
        "stores": stores or [],
        "nodes": nodes if nodes is not None else [
            node("n1", uses={"model": "m@1"}, emits={"out": "T_out"},
                 context={"sections": [sec_input()]})],
        "edges": edges or [],
        "policies": policies if policies is not None else {},
    }
    d["meta"].update(meta or {})
    return d


# ---------------------------------------------------------------- A 基线

def group_A():
    add("A01_single", doc(), "PASS", "单 agent 节点合法基线", "baseline")
    add("A02_chain", doc(
        meta={"entry": "n3", "output": {"ref": "T_out", "from": "n3"}},
        nodes=[
            node("n1", uses={"model": "m@1"}, emits={"p": "T_in"},
                 context={"sections": [sec_input()]}),
            node("n2", consumes={"go": "T_in"}, emits={"p2": "T_out"},
                 context={"sections": [sec_edge("go")]}),
            node("n3", consumes={"go": "T_out"}, emits={"p3": "T_out"},
                 context={"sections": [sec_edge("go")]}),
        ],
        edges=[{"from": "n1", "output": "p", "to": "n2", "input": "go"},
               {"from": "n2", "output": "p2", "to": "n3", "input": "go"}]),
        "PASS", "三节点线性链", "baseline")

    g = doc(meta={"entry": "n1", "output": {"ref": "T_out", "from": "lp"}},
            types=[t_in(), t_out(), t_item()], stores=[store_decl("S", "lp.w")],
            nodes=[
                node("n1", uses={"model": "m@1"}, emits={"out": "T_item"},
                     context={"sections": [sec_input()]}),
                node("lp", emits={"fin": "T_out"}, loop={
                    "body": {"nodes": [
                        node("w", writes=["S"], emits={"o": "T_out"}),
                        node("chk", kind="check", uses={"unit": "g@1"},
                             consumes={"go": "T_out"}),
                    ], "edges": [
                        {"from": "w", "output": "o", "to": "chk", "input": "go"}]},
                    "max_iterations": 3,
                    "carried": [{"store": "S", "max_items": 8}],
                    "safety_points": [{"node": "chk", "kind": "check"}],
                    "args": {"probe": "n1.out"},
                    "exit_when": "attempt.passed",
                }),
            ],
            edges=[{"from": "n1", "output": "out", "to": "lp", "input": "go"}])
    add("A03_loop", g, "PASS", "loop+carried+安全点+exit_when", "baseline")

    add("A04_map", doc(
        meta={"entry": "src", "output": {"ref": "T_out", "from": "sink"}},
        types=[t_in(), t_out(), t_item()],
        nodes=[
            node("src", emits={"items": "list<T_item>"}),
            node("mp", map={"over": "src.items", "body": {"nodes": [
                node("worker", uses={"model": "m@1"}, emits={"r": "T_out"},
                     context={"sections": [sec_arg()]}),], "edges": []}}),
            node("sink", consumes={"go": "signal"}, emits={"fin": "T_out"}),
        ],
        edges=[{"from": "src", "output": "items", "to": "mp", "input": "go"},
               {"from": "mp", "output": "done", "to": "sink", "input": "go"}]),
        "PASS", "map over list 端口 + done:signal 边", "baseline")

    g = doc(meta={"entry": "router", "output": {"ref": "T_out", "from": "n2"}},
            nodes=[
                node("router", emits={"p": "T_in"}),
                node("n2", consumes={"go": "T_in"}, emits={"p2": "T_out"},
                     context={"sections": [sec_edge("go")]}),
                node("hr", human=human_decl()),
            ],
            edges=[{"from": "router", "output": "p", "to": "n2", "input": "go"}])
    g["nodes"][0]["route"] = {"rules": [{"when": "resume.approved", "goto": "n2"}],
                              "default": "exit"}
    add("A05_human_route", g, "PASS", "human+route(规则+exit)", "baseline")

    add("A06_escape_flow", doc(
        meta={"entry": "pre", "output": {"ref": "T_out", "from": "n1"}},
        nodes=[
            node("pre", emits={"p0": "T_in"}),
            node("esc", uses={"model": "m@1"}, emits={"p": "T_in"},
                 consumes={"go": "T_in"},
                 context={"sections": [sec_edge("go"), sec_input()]},
                 escape={"kind": "dynamic_tool_registry", "impl_ref": "dyn@1",
                         "reason": "r", "owner": "team", "share_budget_pct": 10}),
            node("n1", consumes={"go": "T_in"}, emits={"p2": "T_out"}),
        ],
        edges=[{"from": "pre", "output": "p0", "to": "esc", "input": "go"},
               {"from": "esc", "output": "p", "to": "n1", "input": "go"}]),
        "PASS", "escape 四字段+kind，带出入边数据流", "baseline")

    add("A07_eval_legal", doc(
        meta={"entry": "W", "output": {"ref": "T_out", "from": "W"}},
        stores=[store_decl("answer", "W", stype="document", eval_input=True)],
        nodes=[
            node("W", uses={"model": "m@1"}, writes=["answer"], emits={"fin": "T_out"},
                 context={"sections": [sec_input()]}),
            node("ev", kind="check", uses={"unit": "grader@1"}, reads=["answer"],
                 consumes={"go": "T_out"}),
        ],
        edges=[{"from": "W", "output": "fin", "to": "ev", "input": "go"}]),
        "PASS", "eval_input 读者⊆{写者, check+unit}", "baseline")

    add("A08_nested_loop", doc(
        meta={"entry": "lp1", "output": {"ref": "T_out", "from": "lp1"}},
        types=[t_in(), t_out()], stores=[store_decl("S", "lp1.lp2.wi")],
        nodes=[
            node("lp1", emits={"fin": "T_out"}, loop={
                "body": {"nodes": [
                    node("lp2", emits={"o2": "T_out"}, loop={
                        "body": {"nodes": [
                            node("wi", writes=["S"], emits={"o": "T_out"},
                                 human=human_decl())], "edges": []},
                        "max_iterations": 2,
                        "carried": [{"store": "S", "max_items": 4}],
                        "safety_points": [{"node": "wi", "kind": "human"}]}),
                    node("chk2", kind="check", uses={"unit": "g@1"},
                         consumes={"go": "T_out"}),
                ], "edges": [{"from": "lp2", "output": "o2", "to": "chk2", "input": "go"}]},
                "max_iterations": 2,
                "carried": [{"store": "S", "max_items": 4}],
                "safety_points": [{"node": "chk2", "kind": "check"}]}),
        ]),
        "PASS", "双层 loop 嵌套（安全点各在本层体）", "baseline")

    g = doc(meta={"entry": "L", "output": {"ref": "T_out", "from": "L"}},
            types=[t_in(), t_out(), t_item()], stores=[store_decl("S", "L.join")],
            nodes=[
                node("L", emits={"fin": "T_out"}, loop={
                    "body": {"nodes": [
                        node("src", emits={"items": "list<T_item>"}),
                        node("mp", map={"over": "src.items", "body": {"nodes": [
                            node("worker", uses={"model": "m@1"}, emits={"r": "T_out"},
                                 context={"sections": [sec_arg()]}),], "edges": []}}),
                        node("join", consumes={"go": "signal"}, writes=["S"],
                             emits={"o": "T_out"}, human=human_decl()),
                    ], "edges": [
                        {"from": "src", "output": "items", "to": "mp", "input": "go"},
                        {"from": "mp", "output": "done", "to": "join", "input": "go"}]},
                    "max_iterations": 2,
                    "carried": [{"store": "S", "max_items": 4}],
                    "safety_points": [{"node": "join", "kind": "human"}]}),
            ])
    add("A09_map_in_loop", g, "PASS", "loop⊃map，汇聚单写，human 安全点", "baseline")

    add("A10_route_multexit", doc(
        meta={"entry": "r", "output": {"ref": "T_out", "from": "n1"}},
        nodes=[
            node("r", uses={"model": "m@1"}, emits={"p": "T_in"},
                 context={"sections": [sec_input()]},
                 route={"rules": [
                     {"when": "steps.turns > 3", "goto": "exit"},
                     {"when": "input.q == 'stop'", "goto": "exit"},
                     {"when": "input.q != 'go'", "goto": "n1"}],
                     "default": "exit"}),
            node("n1", consumes={"go": "T_in"}, emits={"p2": "T_out"}),
        ],
        edges=[{"from": "r", "output": "p", "to": "n1", "input": "go"}]),
        "PASS", "多 exit 规则良性 + 全覆盖", "baseline")


# ---------------------------------------------------------------- B 变异

_DEL = object()


def set_path(d, path, val):
    parts = path.split("/")
    cur = d
    for p in parts[:-1]:
        cur = cur[int(p)] if isinstance(cur, list) else cur[p]
    last = parts[-1]
    if val is _DEL:
        if isinstance(cur, list):
            del cur[int(last)]
        else:
            cur.pop(last, None)
    else:
        if isinstance(cur, list):
            cur[int(last)] = val
        else:
            cur[last] = val


def mut(name, base, mutations, expect=None, note=""):
    g = copy.deepcopy(base)
    for path, val in mutations:
        set_path(g, path, val)
    add(name, g, expect, note or f"变异 {mutations}", "mutate")


def loop_base():
    """带 loop 的合法基线（store S 写者 lp，体内 w 写 + chk 验证面）。"""
    return doc(meta={"entry": "n1", "output": {"ref": "T_out", "from": "lp"}},
               types=[t_in(), t_out(), t_item()], stores=[store_decl("S", "lp.w")],
               nodes=[
                   node("n1", uses={"model": "m@1"}, emits={"out": "T_item"},
                        context={"sections": [sec_input()]}),
                   node("lp", emits={"fin": "T_out"}, loop={
                       "body": {"nodes": [
                           node("w", writes=["S"], emits={"o": "T_out"}),
                           node("chk", kind="check", uses={"unit": "g@1"})],
                           "edges": []},
                       "max_iterations": 3,
                       "carried": [{"store": "S", "max_items": 8}],
                       "safety_points": [{"node": "chk", "kind": "check"}],
                       "args": {"probe": "n1.out"},
                       "exit_when": "attempt.passed"}),
               ])


def deep_schema(depth):
    s = {"type": "string"}
    for _ in range(depth):
        s = {"type": "object", "properties": {"a": s}}
    return s


def group_B():
    base = doc()
    lb = loop_base()

    # B1 字段类型翻转（40）
    mut("B01_version_str", base, [("version", "1")], "REJECT", "version 字符串→E0002")
    mut("B02_version_bool", base, [("version", True)], "REJECT")
    mut("B03_version_float", base, [("version", 1.5)], "REJECT")
    mut("B04_meta_name_int", base, [("meta/name", 123)], "REJECT")
    mut("B05_graph_version_null", base, [("meta/graph_version", None)], "REJECT")
    mut("B06_entry_int", base, [("meta/entry", 123)], "REJECT")
    mut("B07_input_int", base, [("meta/input", 123)], "REJECT")
    mut("B08_output_ref_int", base, [("meta/output/ref", 123)], "REJECT")
    mut("B09_output_from_int", base, [("meta/output/from", 123)], "REJECT")
    mut("B10_type_schema_str", base, [("types/0/schema", "x")], None,
        "schema 为标量串仍被接受（model.rs 注记「内联 JSON Schema 对象」无形状强制——复核存档）")
    mut("B11_node_kind_int", base, [("nodes/0/kind", 123)], "REJECT")
    mut("B12_uses_model_int", base, [("nodes/0/uses/model", 123)], "REJECT")
    mut("B13_writes_scalar", base, [("nodes/0/writes", "S")], "REJECT")
    mut("B14_reads_int", base, [("nodes/0/reads", 123)], "REJECT")
    mut("B15_emits_int", base, [("nodes/0/emits/out", 123)], "REJECT")
    mut("B16_emits_frombody_plain", base, [("nodes/0/emits/out", {"from_body": "x.y"})],
        "REJECT", "无构造体节点带 from_body→E2106")
    mut("B17_emits_inline_object", base, [("nodes/0/emits/out", {"type": "object"})],
        "REJECT", "内联对象 schema→E1003")
    mut("B18_emits_signal", base, [("nodes/0/emits/out", "signal"),
                                   ("meta/output/ref", "signal")], None,
        "signal 作输出口径探针")
    mut("B19_section_from_int", base, [("nodes/0/context/sections/0/from", 123)], "REJECT")
    mut("B20_quota_str", base, [("nodes/0/context/sections/0/quota_tokens", "100")], "REJECT")
    mut("B21_assert_int", base, [("nodes/0/assert", 123)], "REJECT")
    mut("B22_ops_op_junk", base, [("nodes/0/ops", [{"op": "transmogrify"}])], "REJECT",
        "ops 算子不在词表→E1004")
    mut("B23_ops_param_unclosed", base, [("nodes/0/ops", [{"op": "truncate"}])], None,
        "E2209 参数闭包未实装——预期漏网（文档化边界，复核存档）")
    mut("B24_human_station_int", base, [("nodes/0/human",
        {"station": 123, "role": "r", "resume": {"schema": {"type": "object"}}})], "REJECT")
    mut("B25_escape_pct_neg", base, [("nodes/0/escape",
        {"kind": "env_adapter", "impl_ref": "x@1", "reason": "r", "owner": "t", "share_budget_pct": -1})], "REJECT")
    mut("B26_escape_kind_junk", base, [("nodes/0/escape",
        {"kind": "wat", "impl_ref": "x@1", "reason": "r", "owner": "t", "share_budget_pct": 1})], "REJECT")
    mut("B27_escape_missing", base, [("nodes/0/escape", {"impl_ref": "x@1"})], "REJECT")
    mut("B28_budget_dim_junk", base, [("policies/budgets",
        [{"id": "b", "dimension": "money", "scope": "plan", "cap": 1}])], "REJECT")
    mut("B29_budget_scope_junk", base, [("policies/budgets",
        [{"id": "b", "dimension": "turns", "scope": "galaxy:x", "cap": 1}])], "REJECT")
    mut("B30_budget_cap_zero", base, [("policies/budgets",
        [{"id": "b", "dimension": "turns", "scope": "plan", "cap": 0}])], "REJECT")
    mut("B31_stop_outcome_junk", base, [("policies/stop",
        [{"when": "steps.turns > 0", "outcome": "maybe"}])], "REJECT")
    mut("B32_failure_class_junk", base, [("policies/failures", [{"class": "meteor"}])], "REJECT")
    mut("B33_retry_neg", base, [("policies/failures",
        [{"class": "timeout", "retry": {"max": -1, "backoff_ms": 1}}])], "REJECT")
    mut("B34_eval_input_str", base, [("stores", [store_decl("S", "n1", eval_input="true")])],
        "REJECT")
    mut("B35_eval_input_int", base, [("stores", [store_decl("S", "n1", eval_input=1)])], "REJECT")
    mut("B36_store_type_junk", base, [("stores", [{"name": "S", "type": "message_log_typo",
        "writer": "n1", "lifecycle": "run", "retention": {"max_items": 1}}])], "REJECT")
    mut("B37_lifecycle_junk", base, [("stores", [{"name": "S", "type": "kv", "writer": "n1",
        "lifecycle": "forever", "retention": {"max_items": 1}}])], "REJECT")
    mut("B38_retention_neg", base, [("stores", [store_decl("S", "n1", retention={"max_items": -1})])],
        "REJECT")
    mut("B39_writer_int", base, [("stores", [store_decl("S", 123)])], "REJECT")
    mut("B40_selector_noname", base, [("meta/topology_selector",
        {"rules": [{"when": "input.q == 'a'", "use": "g"}], "fallback": "g"})], "REJECT",
        "非 name@version 形 + 默认空声明集→E2001")
    mut("B41_selector_cel_junk", base, [("meta/topology_selector",
        {"rules": [{"when": "(((", "use": "g@1"}], "fallback": "g@1"})], "REJECT",
        "selector when CEL 判定")
    mut("B42_edge_from_int", base, [("edges",
        [{"from": 1, "output": "p", "to": "n1", "input": "go"}])], "REJECT")
    mut("B43_node_id_int", base, [("nodes/0/id", 123)], "REJECT")
    mut("B44_unknown_field_node", base, [("nodes/0/sneaky", 1)], "REJECT")
    mut("B45_unknown_top_key", base, [("stowaway", [])], "REJECT", "信封未知键→E0004")

    # B2 边界值（13）
    mut("B46_quota_zero", base, [("nodes/0/context/sections/0/quota_tokens", 0)], "REJECT")
    mut("B47_quota_u64max", base, [("nodes/0/context/sections/0/quota_tokens", U64MAX)], None,
        "u64 上界探针")
    mut("B48_quota_inf", base, [("nodes/0/context/sections/0/quota_tokens", float("inf"))],
        "REJECT", ".inf→u64 失败")
    mut("B49_quota_nan", base, [("nodes/0/context/sections/0/quota_tokens", float("nan"))],
        "REJECT")
    mut("B50_retention_zero", base, [("stores", [store_decl("S", "n1", retention={"max_items": 0})])],
        None, "retention Some(0) 记有界——值域空洞探针（复核存档）")
    mut("B51_maxiter_zero", lb, [("nodes/1/loop/max_iterations", 0)], "REJECT", "E2002")
    mut("B52_maxiter_u32p1", lb, [("nodes/1/loop/max_iterations", U32MAX + 1)], "REJECT",
        "u32 溢出→E1003")
    mut("B53_cap_u64max", base, [("policies/budgets",
        [{"id": "b", "dimension": "turns", "scope": "plan", "cap": U64MAX}])], None)
    mut("B54_retry_u32max", base, [("policies/failures",
        [{"class": "timeout", "retry": {"max": U32MAX, "backoff_ms": U64MAX}}])], None)
    mut("B55_escape_pct_u32max", base, [("nodes/0/escape",
        {"kind": "meta_loop", "impl_ref": "x@1", "reason": "r", "owner": "t", "share_budget_pct": U32MAX})], None,
        "逃逸预算份额无合计核对探针")
    g = copy.deepcopy(base)
    g["stores"] = [store_decl("S", "n1")]
    g["nodes"][0]["context"]["sections"] = [sec_store("S", policy="latest_k", quota=10, args={"k": 0})]
    add("B56_args_k_zero", g, "REJECT", "latest_k args.k=0→E2401", "mutate")
    g = copy.deepcopy(base)
    g["stores"] = [store_decl("S", "n1")]
    g["nodes"][0]["context"]["sections"] = [sec_store("S", policy="latest_k", quota=10, args={"k": -1})]
    add("B57_args_k_neg", g, "REJECT", None, "mutate")
    mut("B58_carried_unbounded", lb, [("nodes/1/loop/carried", [{"store": "S"}])], "REJECT",
        "carried 无上界→E2002")

    # B3 空集/缺段（12）
    mut("B59_types_empty", base, [("types", [])], "REJECT", "meta.input 未登记")
    mut("B60_nodes_empty", base, [("nodes", [])], "REJECT", "entry 悬空")
    g = copy.deepcopy(base); g.pop("meta")
    add("B61_meta_absent", g, "REJECT", "缺 meta→E1002", "mutate")
    g = copy.deepcopy(base); g.pop("version")
    add("B62_version_absent", g, "REJECT", "缺 version→E0001", "mutate")
    g = copy.deepcopy(base); g["nodes"][0]["context"]["sections"] = []
    add("B63_sections_empty_llm", g, "PASS", "无入边 LLM 节点空 sections 合法", "mutate")
    g = copy.deepcopy(base); g["nodes"][0]["emits"] = {}
    add("B64_emits_empty", g, "REJECT", "output.from 无端口发射→E2202", "mutate")
    g = copy.deepcopy(lb); g["nodes"][1]["loop"]["safety_points"] = []
    add("B65_safety_empty", g, "REJECT", "E2004 空安全点", "mutate")
    g = copy.deepcopy(lb); g["nodes"][1]["loop"]["carried"] = []
    add("B66_carried_empty", g, "REJECT", "E2211 覆盖缺失", "mutate")
    g = copy.deepcopy(lb); g["nodes"][1]["loop"]["body"]["nodes"] = []
    add("B67_body_empty", g, "REJECT", "空体→E2107 安全点悬空", "mutate")
    g = copy.deepcopy(lb)
    g["nodes"][1]["loop"]["body"]["nodes"] = []
    g["nodes"][1]["loop"]["safety_points"] = []
    add("B68_body_sp_empty", g, "REJECT", "空体+空安全点→E2004", "mutate")
    g = copy.deepcopy(base); g["nodes"][0]["writes"] = []
    g["stores"] = [store_decl("S", "n1")]
    add("B69_zero_writer", g, "PASS", "零写者不报（inv2.rs 文档化边界，复核存档）", "mutate")
    g = copy.deepcopy(base)
    g["edges"] = [{"from": "ghost", "output": "p", "to": "n1", "input": "go"}]
    add("B70_edge_ghost", g, "REJECT", "E2001 边悬空", "mutate")

    # B4 嵌套深化（7）
    def nested_loop(levels):
        g = doc(meta={"entry": "L0", "output": {"ref": "T_out", "from": "L0"}},
                types=[t_in(), t_out()], stores=[store_decl("S", "L0")], nodes=[])
        # wk 不写 store（零写者=文档化不报），使深嵌套探针不被写者噪声污染。
        inner = {"nodes": [node("wk", emits={"o": "T_out"}),
                           node("ck", kind="check", uses={"unit": "g@1"})], "edges": []}
        cur = inner
        for i in range(levels - 1, -1, -1):
            sp_target = "ck" if i == levels - 1 else f"L{i+1}.ck"
            lp = node(f"L{i}", emits={"fin": "T_out"}, loop={
                "body": cur, "max_iterations": 2,
                "carried": [{"store": "S", "max_items": 4}],
                "safety_points": [{"node": sp_target, "kind": "check"}]})
            cur = {"nodes": [lp, node("ck", kind="check", uses={"unit": "g@1"})],
                   "edges": [{"from": f"L{i}", "output": "fin", "to": "ck", "input": "go"}]}
        g["nodes"] = cur["nodes"]
        g["edges"] = cur["edges"] if False else []
        return g
    add("B71_loop_depth3", nested_loop(3), "PASS", "3 层 loop（零写者合法口径）", "mutate")
    add("B72_loop_depth5", nested_loop(5), "PASS", "5 层 loop", "mutate")
    add("B73_loop_depth8", nested_loop(8), "PASS", "8 层 loop", "mutate")
    g = copy.deepcopy(base)
    g["types"].append({"name": "Deep", "schema": deep_schema(100)})
    g["nodes"][0]["emits"] = {"out": "Deep"}
    g["meta"]["output"]["ref"] = "Deep"
    add("B74_schema_depth100", g, "REJECT", "serde_yaml 递归限（≈128）优雅 E0001——鲁棒性正面证据", "mutate")
    g = copy.deepcopy(base)
    g["types"].append({"name": "Deep", "schema": deep_schema(200)})
    g["nodes"][0]["emits"] = {"out": "Deep"}
    add("B75_schema_depth200", g, "REJECT", "serde_yaml 递归限优雅 E0001", "mutate")
    add("B76_yaml_flow_depth5000",
        "version: 1\nmeta: " + "{'x': " * 5000 + "1" + "}" * 5000 + "\n",
        None, "YAML 流深 5000——栈探针（可能 crash）", "mutate")
    add("B77_yaml_flow_depth50000",
        "version: 1\nmeta: " + "{'x': " * 50000 + "1" + "}" * 50000 + "\n",
        None, "YAML 流深 50000——栈溢出探针", "mutate")

    # B5 Unicode（7）
    g = copy.deepcopy(base); g["nodes"][0]["id"] = "节点甲"
    g["meta"]["entry"] = "节点甲"; g["meta"]["output"]["from"] = "节点甲"
    add("B78_unicode_id", g, "PASS", "中文节点 id", "mutate")
    g = copy.deepcopy(base); g["nodes"][0]["id"] = "n🚀e"
    g["meta"]["entry"] = "n🚀e"; g["meta"]["output"]["from"] = "n🚀e"
    add("B79_emoji_id", g, "PASS", "emoji id", "mutate")
    g = copy.deepcopy(base); g["types"][0]["name"] = "输入甲"
    g["meta"]["input"] = "输入甲"
    add("B80_unicode_type", g, "PASS", "中文类型名", "mutate")
    g = copy.deepcopy(base); g["nodes"][0]["id"] = "Ａ.Ｂ"
    g["meta"]["entry"] = "Ａ.Ｂ"; g["meta"]["output"]["from"] = "Ａ.Ｂ"
    add("B81_fullwidth_dot_id", g, "REJECT",
        "全角点号 id：entry 精确命中、output.from 半角点号分段→悬空（C-AL5 同族对照）", "mutate")
    g = copy.deepcopy(base); g["nodes"][0]["id"] = "a​b"
    g["meta"]["entry"] = "a​b"; g["meta"]["output"]["from"] = "a​b"
    add("B82_zwsp_id", g, "PASS", "零宽空格 id（字节级不可视）", "mutate")
    g = copy.deepcopy(base)
    g["policies"] = {"stop": [{"when": "input.问题 == '好'", "outcome": "completed"}]}
    add("B83_unicode_cel_field", g, "REJECT", "CEL 合成绑定无「问题」→E2203", "mutate")
    g = copy.deepcopy(base)
    g["nodes"][0]["context"]["sections"] = [
        {"from": "template", "template_ref": "pkg.模板.少样本", "quota_tokens": 10}]
    add("B84_unicode_template_ref", g, "PASS", "中文 template_ref 不核对目标", "mutate")

    # B6 超长标识符（5）
    lid = "L" + "o" * 1000
    g = copy.deepcopy(base); g["nodes"][0]["id"] = lid
    g["meta"]["entry"] = lid; g["meta"]["output"]["from"] = lid
    add("B85_id_1k", g, "PASS", "1KB id", "mutate")
    lid = "L" + "o" * 100000
    g = copy.deepcopy(base); g["nodes"][0]["id"] = lid
    g["meta"]["entry"] = lid; g["meta"]["output"]["from"] = lid
    add("B86_id_100k", g, "PASS", "100KB id（无长度上限探针）", "mutate")
    g = copy.deepcopy(base); g["types"][0]["name"] = "T" + "y" * 100000
    g["meta"]["input"] = g["types"][0]["name"]
    add("B87_typename_100k", g, "PASS", "100KB 类型名", "mutate")
    g = copy.deepcopy(lb); g["stores"][0]["writer"] = "a" + ".b" * 50000
    add("B88_writer_path_100kseg", g, "REJECT", "5 万段写者路径→E2001（split 性能探针）", "mutate")
    cel = "steps.turns + " * 150000
    g = copy.deepcopy(base)
    g["policies"] = {"stop": [{"when": cel[:900000] + "1", "outcome": "completed"}]}
    add("B89_cel_900kb_chain", g, None, "150 万段 + 链——解析长/深探针（可能 crash/超时）", "mutate")

    # B7 YAML 怪癖（16，raw 文本）
    raws = [
        ("B90_dup_version_keys",
         "version: 1\nversion: 2\nmeta: {name: g, graph_version: '1', entry: n1, "
         "input: T_in, output: {ref: T_out, from: n1}}\n",
         None, "version 键重复——serde_yaml 行为探针"),
        ("B91_multidoc", "version: 1\n---\nversion: 1\n", "REJECT", "多文档→E0001 归并"),
        ("B92_bom", "﻿version: 1\n", None, "BOM 探针"),
        ("B93_crlf", "version: 1\r\n", None, "CRLF"),
        ("B94_tab_indent", "version: 1\nmeta:\n\tname: g\n", "REJECT", "tab 缩进→语法错"),
        ("B95_empty_file", "", "REJECT", "空文件→E0001"),
        ("B96_comment_only", "# 什么都没有\n", "REJECT", "纯注释→E0001"),
        ("B97_null_root", "~\n", "REJECT", "null 根→E0001"),
        ("B98_seq_root", "- a\n- b\n", "REJECT", "序列根→E0001"),
        ("B99_version_hex", "version: 0x1\n", None, "0x1 整数性探针"),
        ("B100_version_plus", "version: +1\n", None, "+1 探针"),
        ("B101_version_underscore", "version: 1_000\n", None, "下划线整数探针"),
        ("B102_anchor_alias",
         "version: 1\nmeta: &m {name: g, graph_version: '1', entry: n1, input: T_in, "
         "output: {ref: T_out, from: n1}}\nmeta2: *m\n",
         "REJECT", "锚点+未知键 E0004"),
        ("B103_merge_key",
         "version: 1\nbase: &b {name: g}\nmeta:\n  <<: *b\n  graph_version: '1'\n"
         "  entry: n1\n  input: T_in\n  output: {ref: T_out, from: n1}\n",
         "REJECT", "merge 键语义探针"),
        ("B104_tagged_version", "version: !!str 1\n", "REJECT", "tag 字符串化→E0002"),
        ("B105_doc_marker_only", "---\n", "REJECT", "仅文档标记"),
    ]
    for name, text, expect, note in raws:
        add(name, text, expect, note, "mutate")


# ---------------------------------------------------------------- C 定向攻击
# hunt = 真·猎杀样本（读码构造，预期按不变量应拒但预测验证器放行）
# fk   = 误杀探针（预期按直觉应过但预测验证器拒绝）
# ctl  = 对照（证明 harness 对该不变量的敏感度）

def group_C():
    # ===== C-AL 路径别名（点号 id 与构造体路径同形 → Index 键冲突互覆盖）=====
    def alias_doc(root_twin_writes=None, root_twin_emits=None, root_twin_kind="check",
                  twin_extra=None):
        """a(loop, 体内 b 写 S) + 点号根节点 "a.b" 双胞胎。文档序：a 先、双胞胎后
        → ix.nodes["a.b"] 被根双胞胎覆盖（mod.rs walk: BTreeMap::insert 无重复侦测）。"""
        twin = node("a.b", kind=root_twin_kind, emits={"out": "T_out"})
        if root_twin_writes is not None:
            twin["writes"] = root_twin_writes
        if root_twin_emits is not None:
            twin["emits"] = root_twin_emits
        if twin_extra:
            twin.update(twin_extra)
        return doc(meta={"entry": "n1", "output": {"ref": "T_out", "from": "a.b"}},
                   types=[t_in(), t_out(), t_item()], stores=[store_decl("S", "a.b")],
                   nodes=[
                       node("n1", uses={"model": "m@1"}, emits={"out": "T_item"},
                            context={"sections": [sec_input()]}),
                       node("a", loop={
                           "body": {"nodes": [
                               node("b", kind="check", writes=["S"], emits={"o": "T_out"})],
                               "edges": []},
                           "max_iterations": 2,
                           "carried": [{"store": "S", "max_items": 4}],
                           "safety_points": [{"node": "b", "kind": "check"}]}),
                       twin,
                   ],
                   edges=[{"from": "n1", "output": "out", "to": "a", "input": "go"}])

    add("C_AL01_writer_alias_root_over_body", alias_doc(root_twin_writes=["S"]),
        None, "hunt：双写者（体内 b 每迭代写 + 根 a.b 写）——根双胞胎覆盖索引后 INV-2 "
              "只见一个写者且路径与声明相同 → 预测 E2311/E2312 全哑", "alias")
    add("C_AL02_writer_alias_body_wins",
        (lambda g: (g["nodes"].reverse(), g)[1])(alias_doc(root_twin_writes=["S"])),
        None, "hunt：文档序反转（双胞胎先、a 后）→ 体内 b 覆盖索引；b 写 S 路径恰同声明 "
              "→ E2311 哑；E2211 由 carried 满足", "alias")

    def alias_map_doc():
        """m(map, 体内 b 写 S) + 点号根 "m.b"（不写）。writer "m.b" 解析进 map 体，
        但索引里 "m.b" = 根双胞胎（无 writes）→ 零写者 + E2312 全哑。"""
        return doc(meta={"entry": "n1", "output": {"ref": "list<T_item>", "from": "n1"}},
                   types=[t_in(), t_out(), t_item()], stores=[store_decl("S", "m.b")],
                   nodes=[
                       node("n1", uses={"model": "m@1"}, emits={"out": "list<T_item>"},
                            context={"sections": [sec_input()]}),
                       node("m", map={"over": "n1.out", "body": {"nodes": [
                           node("b", kind="check", writes=["S"])], "edges": []}}),
                       node("m.b", kind="task"),
                   ],
                   edges=[{"from": "n1", "output": "out", "to": "m", "input": "go"}])
    add("C_AL03_map_writer_hidden", alias_map_doc(), None,
        "hunt：map 体写者被点号根双胞胎覆盖 → E2312（N 并行实例并发写）完全不可见", "alias")

    add("C_AL04_entry_output_asym", doc(
        meta={"entry": "weird.name", "output": {"ref": "T_out", "from": "weird.name"}},
        nodes=[node("weird.name", uses={"model": "m@1"}, emits={"out": "T_out"},
                    context={"sections": [sec_input()]})]),
        None, "fk：entry 按精确 id 命中（scopes ids 表），output.from 按点号分段解析 → "
              "同一节点一个收一个拒（误杀候选，refs.rs:30 vs refs.rs:37 口径不一致）", "alias")

    g = doc(meta={"entry": "a.b", "output": {"ref": "T_out", "from": "a.b"}},
            types=[t_in(), t_out()], nodes=[
                node("n0", emits={"seed": "T_in"}),
                node("a", loop={"body": {"nodes": [
                    node("b", emits={"o": "T_out"}, human=human_decl())], "edges": []},
                    "max_iterations": 1,
                    "safety_points": [{"node": "b", "kind": "human"}]}),
                node("a.b", kind="task", emits={"o": "T_out"}, human=human_decl()),
            ],
            edges=[{"from": "n0", "output": "seed", "to": "a", "input": "go"}])
    add("C_AL05_output_from_shadowed_body", g, None,
        "hunt：体内 b（真发射者）被点号根 a.b 覆盖 → output.from 判到无 emits 的根双胞胎 "
        "→ 预测 E2202 误杀（真实发射者存在）", "alias")

    def alias_verifier_doc():
        """c(loop, 体内 v(kind=check) 自环 v→v) + 点号根 "c.v" 双胞胎 → E2321 哑。"""
        return doc(meta={"entry": "n1", "output": {"ref": "T_out", "from": "n1"}},
                   types=[t_in(), t_out()], nodes=[
                       node("n1", uses={"model": "m@1"}, emits={"out": "T_out"},
                            context={"sections": [sec_input()]}),
                       node("c", loop={
                           "body": {"nodes": [
                               node("v", kind="check", consumes={"p": "boolean"},
                                    emits={"o": "T_out"})],
                               "edges": [{"from": "v", "output": "passed", "to": "v",
                                          "input": "p"}]},
                           "max_iterations": 1,
                           "safety_points": [{"node": "v", "kind": "check"}]}),
                       node("c.v", kind="check"),
                   ],
                   edges=[{"from": "n1", "output": "out", "to": "c", "input": "go"}])
    add("C_AL06_verifier_selfloop_hidden", alias_verifier_doc(), None,
        "hunt：自验证环 v→v 被点号根双胞胎覆盖 → E2321 哑（生成即验收潜入）", "alias")

    def alias_eval_doc(reader_kind="task", twin="shadow"):
        """store secret(eval_input, writer W)；体内 r 经 section 读 secret（越权读者）
        + 点号根 "c.r" 双胞胎（不读）→ 读者被覆盖 → E2331 哑。"""
        twin_node = node("c.r", kind="task", human=human_decl())
        return doc(meta={"entry": "W", "output": {"ref": "T_out", "from": "W"}},
                   types=[t_in(), t_out()],
                   stores=[store_decl("secret", "W", eval_input=True)],
                   nodes=[
                       node("W", uses={"model": "m@1"}, writes=["secret"],
                            emits={"fin": "T_out"}, context={"sections": [sec_input()]}),
                       node("c", loop={
                           "body": {"nodes": [
                               node("r", kind=reader_kind,
                                    context={"sections": [sec_store("secret")]})],
                               "edges": []},
                           "max_iterations": 1,
                           "safety_points": [{"node": "r", "kind": "human"}]}),
                       twin_node,
                   ])
    g = alias_eval_doc()
    g["nodes"][2]["human"] = human_decl()  # 体内 r 需 human 标记当安全点
    add("C_AL07_eval_reader_hidden", g, None,
        "hunt：eval_input 越权读者（体内 r）被点号根双胞胎覆盖 → E2331 哑", "alias")

    add("C_AL08_failures_route_to_body_node", doc(
        meta={"entry": "n1", "output": {"ref": "T_out", "from": "n1"}},
        policies={"failures": [{"class": "tool_error", "route": "c.b"}]},
        nodes=[
            node("n1", uses={"model": "m@1"}, emits={"out": "T_out"},
                 context={"sections": [sec_input()]}),
            node("c", loop={"body": {"nodes": [
                node("b", kind="check", uses={"unit": "g@1"})], "edges": []},
                "max_iterations": 1,
                "safety_points": [{"node": "b", "kind": "check"}]}),
        ]),
        None, "hunt：failures.route 解析到体内深层节点（ix.nodes 路径键含体内路径）"
              "→ 预测放行；失败重路由目标应限根图节点（复核存档）", "alias")

    add("C_AL09_budget_scope_into_body", doc(
        meta={"entry": "n1", "output": {"ref": "T_out", "from": "n1"}},
        policies={"budgets": [{"id": "b", "dimension": "turns", "scope": "node:c.b", "cap": 3}]},
        nodes=[
            node("n1", uses={"model": "m@1"}, emits={"out": "T_out"},
                 context={"sections": [sec_input()]}),
            node("c", loop={"body": {"nodes": [
                node("b", kind="check", uses={"unit": "g@1"})], "edges": []},
                "max_iterations": 1,
                "safety_points": [{"node": "b", "kind": "check"}]}),
        ]),
        None, "hunt：预算作用域 node:c.b 解析进体内 → 预测放行（复核存档：作用域是否应限根图）", "alias")

    add("C_AL10_cel_dotted_id_unreachable", doc(
        meta={"entry": "n1", "output": {"ref": "T_out", "from": "n1"}},
        nodes=[
            node("n1", uses={"model": "m@1"}, emits={"out": "T_out"},
                 context={"sections": [sec_input(), sec_edge("go")], },
                 route={"rules": [{"when": "x.y.out == 'z'", "goto": "n2"}],
                        "default": "exit"}),
            node("n2", emits={"fin": "T_out"}),
            node("x.y", emits={"out": "T_out"}),
        ],
        edges=[{"from": "n1", "output": "out", "to": "n2", "input": "go"},
               {"from": "n2", "output": "fin", "to": "n1", "input": "go"}]),
        None, "fk：点号 id 节点在 CEL 词表登记为根名『x.y』，但 CEL 解析 x.y.out 时根是 x "
              "→ 预测 E2203 误杀（节点无法被 CEL 引用）", "alias")

    add("C_AL11_loopmap_same_bodyid", doc(
        meta={"entry": "n1", "output": {"ref": "list<T_item>", "from": "n1"}},
        types=[t_in(), t_out(), t_item()], stores=[store_decl("S", "c.b")],
        nodes=[
            node("n1", uses={"model": "m@1"}, emits={"out": "list<T_item>"},
                 context={"sections": [sec_input()]}),
            node("c", loop={
                "body": {"nodes": [node("b", kind="check", writes=["S"])], "edges": []},
                "max_iterations": 1,
                "carried": [{"store": "S", "max_items": 4}],
                "safety_points": [{"node": "b", "kind": "check"}]},
                map={"over": "n1.out", "body": {"nodes": [
                    node("b", kind="check", writes=["S"])], "edges": []}}),
        ],
        edges=[{"from": "n1", "output": "out", "to": "c", "input": "go"}]),
        None, "hunt：同构造节点 loop+map 双体各含同 id 写者 b（双 scope 无重复侦测）→ "
              "路径同形『c.b』索引互覆盖；map 体后走 → 幸存者是 map 体节点 → 预测 E2312 单发，"
              "但 loop 体写者不可见（写者集合被吞）", "alias")

    def alias_safety_doc(body_kind="task", twin_kind="check"):
        """c(loop, 体内 v) + 点号根 "c.v"（kind=twin_kind）。安全点按路径解析后 nodes.get『c.v』命中双胞胎 → kind 核对判错对象。"""
        return doc(meta={"entry": "n1", "output": {"ref": "T_out", "from": "n1"}},
                   types=[t_in(), t_out()], nodes=[
                       node("n1", uses={"model": "m@1"}, emits={"out": "T_out"},
                            context={"sections": [sec_input()]}),
                       node("c", loop={
                           "body": {"nodes": [
                               node("v", kind=body_kind, human=human_decl())],
                               "edges": []},
                           "max_iterations": 1,
                           "safety_points": [{"node": "v", "kind": "check"}]}),
                       node("c.v", kind=twin_kind),
                   ])
    add("C_AL12_safetypoint_wrong_target_fk", alias_safety_doc("check", "task"), None,
        "fk：体内 v 真是 check（合法安全点），解析命中的是 kind=task 双胞胎 → 预测 E2322 误杀", "alias")
    add("C_AL13_safetypoint_fake_pass", alias_safety_doc("task", "check"), None,
        "hunt：体内 v 是 task（非验证面），解析命中 kind=check 双胞胎 → 预测 E2322 哑"
        "（无效安全点放行）", "alias")

    add("C_AL14_frombody_shadow_fk", doc(
        meta={"entry": "src", "output": {"ref": "T_out", "from": "sink"}},
        types=[t_in(), t_out(), t_item()],
        nodes=[
            node("src", emits={"items": "list<T_item>"}),
            node("mp", map={"over": "src.items", "body": {"nodes": [
                node("b", emits={"r": "T_out"})], "edges": []}},
                emits={"fwd": {"from_body": "b.r"}}),
            node("sink", consumes={"go": "signal"}, emits={"fin": "T_out"}),
            node("mp.b", kind="task", emits={"zz": "T_item"}),
        ],
        edges=[{"from": "src", "output": "items", "to": "mp", "input": "go"},
               {"from": "mp", "output": "done", "to": "sink", "input": "go"}]),
        "PASS", "fk：from_body b.r 解析经 scope_id→path→nodes.get『mp.b』命中点号根双胞胎"
                "（无 r 端口）→ 预测 E2106 误杀（体内 b 的 r 端口真实存在）", "alias")

    # ===== C-MW 多写者 =====
    def two_writer(doc_mut=None, writer="w1"):
        g = doc(meta={"entry": "w1", "output": {"ref": "T_out", "from": "w1"}},
                types=[t_in(), t_out()], stores=[store_decl("S", writer)],
                nodes=[
                    node("w1", uses={"model": "m@1"}, writes=["S"], emits={"fin": "T_out"},
                         context={"sections": [sec_input()]}),
                    node("w2", writes=["S"], emits={"o": "T_out"}),
                ])
        if doc_mut:
            doc_mut(g)
        return g
    add("C_MW01_two_root_writers", two_writer(), "REJECT",
        "ctl：双根写者→E2311", "multi-writer")
    add("C_MW02_writer_mismatch", two_writer(writer="w2"), "REJECT",
        "ctl：声明与实际不符→E2311", "multi-writer")
    add("C_MW03_zero_writer", two_writer(
        lambda g: (g["nodes"][1].pop("writes"), g["stores"][0].__setitem__("writer", "w1"))),
        "PASS", "ctl：零写者不报（inv2.rs 文档化边界，复核存档）", "multi-writer")
    g = loop_base()
    g["nodes"][1]["loop"]["carried"] = []
    add("C_MW04_loop_writer_uncarried", g, "REJECT", "ctl：体内写未登记 carried→E2211",
        "multi-writer")
    add("C_MW05_loop_writer_carried", loop_base(), "PASS", "ctl：carried 覆盖合法", "multi-writer")
    g = doc(meta={"entry": "n1", "output": {"ref": "T_out", "from": "n1"}},
            types=[t_in(), t_out()], stores=[store_decl("S", "m.b")],
            nodes=[
                node("n1", uses={"model": "m@1"}, emits={"out": "T_item"},
                     context={"sections": [sec_input()]}),
                node("m", map={"over": "n1.out", "body": {"nodes": [
                    node("b", writes=["S"])], "edges": []}}),
            ],
            edges=[{"from": "n1", "output": "out", "to": "m", "input": "go"}])
    add("C_MW06_map_writer", g, "REJECT", "ctl：map 体写体外 store→E2312", "multi-writer")
    g = copy.deepcopy(g)
    g["nodes"][1]["map"]["carried"] = [{"store": "S", "max_items": 4}]
    add("C_MW07_map_writer_carried", g, "REJECT",
        "ctl：carried 不豁免 E2312（map 构造无 carried 字段→应 serde 拒或字段无效）", "multi-writer")
    g = doc(meta={"entry": "L", "output": {"ref": "T_out", "from": "L"}},
            types=[t_in(), t_out()], stores=[store_decl("S", "L.inn.w")],
            nodes=[
                node("L", emits={"fin": "T_out"}, loop={
                    "body": {"nodes": [
                        node("inn", emits={"o": "T_out"}, loop={
                            "body": {"nodes": [
                                node("w", writes=["S"], emits={"o": "T_out"},
                                     human=human_decl())], "edges": []},
                            "max_iterations": 1,
                            "carried": [{"store": "S", "max_items": 4}],
                            "safety_points": [{"node": "w", "kind": "human"}]}),
                        node("ck", kind="check", uses={"unit": "g@1"})],
                        "edges": [{"from": "inn", "output": "o", "to": "ck", "input": "go"}]},
                    "max_iterations": 1,
                    "carried": [{"store": "S", "max_items": 4}],
                    "safety_points": [{"node": "ck", "kind": "check"}]}),
            ])
    g["nodes"][0]["loop"]["body"]["nodes"][0]["human"] = human_decl()
    add("C_MW08_nested_writer_both_carried", g, "PASS",
        "ctl：深层写者，两层 carried 均覆盖", "multi-writer")
    g2 = copy.deepcopy(g)
    g2["nodes"][0]["loop"]["body"]["nodes"][0]["loop"]["carried"] = []
    add("C_MW09_nested_writer_inner_missing", g2, "REJECT",
        "ctl：内层 carried 缺→E2211", "multi-writer")
    g = two_writer()
    g["nodes"].append(copy.deepcopy(g["nodes"][0]))  # id 重复 w1
    g["nodes"][2]["writes"] = ["S"]
    add("C_MW10_dup_id_second_writes", g, "REJECT",
        "ctl：重复 id 首声明胜；第二份写者不可见→E2003+E2311（w2 仍在）", "multi-writer")
    g = doc(meta={"entry": "w", "output": {"ref": "T_out", "from": "w"}},
            types=[t_in(), t_out()], stores=[store_decl("S", "w")],
            nodes=[
                node("w", uses={"model": "m@1"}, writes=["S"], emits={"fin": "T_out"},
                     context={"sections": [sec_input()]}),
                node("w.x", writes=["S"], emits={"o": "T_out"}),
            ])
    add("C_MW11_dotted_distinct_paths", g, "REJECT",
        "ctl：w 与 w.x 路径不同 → 双写者 E2311 照报（别名对照组）", "multi-writer")
    g = copy.deepcopy(loop_base())
    g["stores"][0]["writer"] = "lp."
    add("C_MW12_writer_trailing_dot", g, "REJECT", "ctl：尾点写者路径解析失败→E2001", "multi-writer")
    g = copy.deepcopy(loop_base())
    g["nodes"][1]["writes"] = ["S", "S"]
    add("C_MW13_writes_dup_entry", g, "REJECT",
        "ctl：同一节点 writes 重复条目→仍单写者（E2311 声明不符照报）", "multi-writer")

    # ===== C-CX 上下文装配 / INV-1 =====
    def chain_llm(ctx=None, reads=None, uses=None, consumer_ctx=None):
        """n1 --out:T_in--> n2。n2 可配 uses/reads/context。"""
        n2 = node("n2", consumes={"go": "T_in"}, emits={"p2": "T_out"})
        if uses:
            n2["uses"] = uses
        if reads is not None:
            n2["reads"] = reads
        if ctx is not None:
            n2["context"] = {"sections": ctx}
        return doc(meta={"entry": "n1", "output": {"ref": "T_out", "from": "n2"}},
                   types=[t_in(), t_out()],
                   nodes=[node("n1", uses={"model": "m@1"}, emits={"out": "T_in"},
                               context={"sections": [sec_input()]}), n2],
                   edges=[{"from": "n1", "output": "out", "to": "n2", "input": "go"}])
    add("C_CX01_llm_inedge_no_ctx", chain_llm(uses={"model": "m@1"}), "REJECT",
        "ctl：LLM-bound 入边端口零引用→E2304", "context")
    add("C_CX02_llm_port_double_ref", chain_llm(
        ctx=[sec_edge("go"), sec_edge("go")], uses={"model": "m@1"}), "REJECT",
        "ctl：同端口被两段引用→E2304（恰一）", "context")
    add("C_CX03_llm_port_single_ref", chain_llm(
        ctx=[sec_edge("go")], uses={"model": "m@1"}), "PASS", "ctl：恰一引用合法", "context")
    add("C_CX04_nonllm_inedge_no_ctx", chain_llm(), "PASS",
        "ctl：非 LLM 节点入边不判 E2304（G3 收窄口径验证）", "context")
    add("C_CX05_nonllm_ctx_zero_edge_ref", chain_llm(ctx=[sec_input()]), "PASS",
        "ctl：非 LLM 带 context 无 edge 段合法", "context")
    add("C_CX06_llm_reads_declared", chain_llm(
        uses={"model": "m@1"}, reads=["S"], ctx=[sec_edge("go")]), "REJECT",
        "ctl：LLM-bound 声明 reads→E2303", "context")
    g = chain_llm(uses={"model": "m@1"}, reads=["ghost"], ctx=[sec_edge("go")])
    add("C_CX07_llm_reads_undeclared", g, "REJECT",
        "ctl：reads 未声明 store→仅 E2001（E2303 抑制口径验证）", "context")
    add("C_CX08_llm_reads_empty", chain_llm(
        uses={"model": "m@1"}, reads=[], ctx=[sec_edge("go")]), "PASS",
        "ctl：reads 空表合法", "context")
    add("C_CX09_edge_port_not_in_inports", chain_llm(
        ctx=[sec_edge("nowhere")], uses={"model": "m@1"}), "REJECT",
        "ctl：edge 段端口不在入边集→E2301", "context")
    add("C_CX10_store_section_undeclared", chain_llm(
        ctx=[sec_store("nope")], uses={"model": "m@1"}), "REJECT",
        "ctl：store 段未声明→E2301", "context")
    g = chain_llm(ctx=[sec_arg("item")], uses={"model": "m@1"})
    add("C_CX11_arg_at_root", g, "REJECT", "ctl：根图 arg 形参表空→E2402", "context")
    g = doc(meta={"entry": "src", "output": {"ref": "T_out", "from": "sink"}},
            types=[t_in(), t_out(), t_item()],
            nodes=[
                node("src", emits={"items": "list<T_item>"}),
                node("mp", map={"over": "src.items", "body": {"nodes": [
                    node("worker", uses={"model": "m@1"}, emits={"r": "T_out"},
                         context={"sections": [sec_arg(), sec_arg("item")]}),],
                    "edges": []}}),
                node("sink", consumes={"go": "signal"}, emits={"fin": "T_out"}),
            ],
            edges=[{"from": "src", "output": "items", "to": "mp", "input": "go"},
                   {"from": "mp", "output": "done", "to": "sink", "input": "go"}])
    add("C_CX12_arg_item_in_map", g, "PASS", "ctl：map 体 arg item 合法（重复段同形参）", "context")
    g = chain_llm(ctx=[{"from": "template", "template_ref": "t@1", "quota_tokens": 10,
                        "vars": {"v": "input.nofield"}}], uses={"model": "m@1"})
    add("C_CX13_vars_input_dangling", g, "REJECT", "ctl：vars input.悬空→E2403", "context")
    g = chain_llm(ctx=[{"from": "template", "template_ref": "t@1", "quota_tokens": 10,
                        "policy": "project"}], uses={"model": "m@1"})
    add("C_CX14_template_with_policy", g, "REJECT", "ctl：template 段带 policy→E2401", "context")
    g = chain_llm(ctx=[{"from": "input", "fields": ["q"], "quota_tokens": 10,
                        "policy": "latest_k"}], uses={"model": "m@1"})
    add("C_CX15_input_policy_latest_k", g, "REJECT", "ctl：input 段非 project policy→E2401", "context")
    g = chain_llm(ctx=[sec_edge("go"),
                       {"from": "store", "store": "S", "policy": "none", "quota_tokens": 0}],
                  uses={"model": "m@1"})
    g["stores"] = [store_decl("S", "n1")]
    add("C_CX16_store_none_quota0", g, "PASS", "ctl：store policy:none quota 0 例外合法", "context")
    g = chain_llm(ctx=[{"from": "input", "fields": ["q"], "quota_tokens": 0}],
                  uses={"model": "m@1"})
    add("C_CX17_input_quota0", g, "REJECT", "ctl：非 store 段 quota 0→E2302", "context")
    g = chain_llm(ctx=[sec_edge("go"), {"from": "input", "fields": [], "quota_tokens": 10}],
                  uses={"model": "m@1"})
    add("C_CX18_input_fields_empty", g, "PASS", "ctl：fields 空表（空投影）合法口径探针", "context")
    g = doc(meta={"entry": "n1", "output": {"ref": "T_out", "from": "n2"}},
            types=[t_in(), t_out()],
            nodes=[
                node("n1", emits={"p": "T_in"}), node("n1b", emits={"p": "T_in"}),
                node("n2", uses={"model": "m@1"}, consumes={"go": "T_in", "go2": "T_in"},
                     emits={"p2": "T_out"},
                     context={"sections": [sec_edge("go"), sec_edge("go2")]}),
            ],
            edges=[{"from": "n1", "output": "p", "to": "n2", "input": "go"},
                   {"from": "n1b", "output": "p", "to": "n2", "input": "go"}])
    add("C_CX19_two_edges_same_input_port", g, "REJECT",
        "hunt：两条入边同端口名 go（in_ports 去重成 1 个端口）+ 两段各引 go/go2 → go2 不在 "
        "in_ports → 预测 E2301 错位（端口名合并语义探针）", "context")
    g = copy.deepcopy(g)
    g["nodes"][2]["context"]["sections"] = [sec_edge("go")]
    add("C_CX20_two_edges_one_section", g, None,
        "hunt：同端口双入边 + 单段引用 → refs=1 → 预测 PASS（两生产者数据共用一段，复核口径）", "context")

    # ===== C-IV3 自验证 =====
    def selfloop(kind="check", human=None):
        n = node("v", kind=kind, consumes={"p": "boolean"}, emits={"o": "T_out"})
        if human:
            n["human"] = human
        return doc(meta={"entry": "v", "output": {"ref": "T_out", "from": "v"}},
                   types=[t_in(), t_out()], nodes=[n],
                   edges=[{"from": "v", "output": "passed", "to": "v", "input": "p"}])
    add("C_IV31_check_selfloop", selfloop("check"), "REJECT", "ctl：check 自环→E2321", "inv3")
    add("C_IV32_gate_selfloop", selfloop("gate"), "REJECT", "ctl：gate 自环→E2321", "inv3")
    add("C_IV33_human_selfloop", selfloop("task", human_decl()), "REJECT",
        "ctl：human 标记自环→E2321", "inv3")
    g = doc(meta={"entry": "c", "output": {"ref": "T_out", "from": "c"}},
            types=[t_in(), t_out()],
            nodes=[node("c", emits={"fin": "T_out"}, loop={
                "body": {"nodes": [
                    node("v", kind="check", consumes={"p": "boolean"}, emits={"o": "T_out"})],
                    "edges": [{"from": "v", "output": "passed", "to": "v", "input": "p"}]},
                "max_iterations": 1,
                "safety_points": [{"node": "v", "kind": "check"}]})])
    add("C_IV34_selfloop_in_body", g, "REJECT", "ctl：体内自环同码", "inv3")
    g = doc(meta={"entry": "chk", "output": {"ref": "T_out", "from": "chk"}},
            types=[t_in(), t_out()],
            nodes=[
                node("chk", kind="check", consumes={"p": "T_in"}, emits={"mid": "T_out"}),
                node("t", consumes={"go": "T_out"}, emits={"o": "T_in"}),
            ],
            edges=[{"from": "chk", "output": "mid", "to": "t", "input": "go"},
                   {"from": "t", "output": "o", "to": "chk", "input": "p"}])
    add("C_IV35_indirect_selfverify_documented", g, "PASS",
        "ctl：间接自验证环（check→transform→check）v0.1 文档化不查（inv3.rs 模块注记）", "inv3")
    g = doc(meta={"entry": "a", "output": {"ref": "T_out", "from": "b"}},
            types=[t_in(), t_out()],
            nodes=[
                node("a", kind="check", consumes={"pa": "T_out"}, emits={"oa": "T_out"}),
                node("b", kind="check", consumes={"pb": "T_out"}, emits={"ob": "T_out"}),
            ],
            edges=[{"from": "a", "output": "oa", "to": "b", "input": "pb"},
                   {"from": "b", "output": "ob", "to": "a", "input": "pa"}])
    add("C_IV36_mutual_pair", g, "PASS", "ctl：互验对（无直接自环）合法", "inv3")
    g = doc(meta={"entry": "L", "output": {"ref": "T_out", "from": "L"}},
            types=[t_in(), t_out()],
            nodes=[node("L", emits={"fin": "T_out"}, loop={
                "body": {"nodes": [
                    node("gt", kind="gate", uses={"model": "m@1"}),
                    node("hk", kind="task", human=human_decl()),
                    node("ck", kind="check", uses={"unit": "g@1"})], "edges": []},
                "max_iterations": 1,
                "safety_points": [{"node": "gt", "kind": "gate"},
                                  {"node": "hk", "kind": "human"},
                                  {"node": "ck", "kind": "check"}]})])
    add("C_IV37_safety_kinds_matrix", g, "PASS", "ctl：三值 kind 全对位", "inv3")
    g = copy.deepcopy(g)
    g["nodes"][0]["loop"]["safety_points"] = [{"node": "gt", "kind": "check"}]
    add("C_IV38_safety_kind_mismatch", g, "REJECT", "ctl：gate 节点配 check 安全点→E2322", "inv3")
    g = doc(meta={"entry": "L", "output": {"ref": "T_out", "from": "L"}},
            types=[t_in(), t_out()],
            nodes=[
                node("outer", kind="check", uses={"unit": "g@1"}),
                node("L", emits={"fin": "T_out"}, loop={
                    "body": {"nodes": [node("w", emits={"o": "T_out"})], "edges": []},
                    "max_iterations": 1,
                    "safety_points": [{"node": "outer", "kind": "check"}]}),
            ])
    add("C_IV39_safety_to_outer", g, "REJECT", "ctl：安全点指体外→E2107", "inv3")
    g = doc(meta={"entry": "esc", "output": {"ref": "T_out", "from": "esc"}},
            types=[t_in(), t_out()],
            nodes=[node("esc", kind="check", uses={"unit": "g@1"},
                        escape={"kind": "meta_loop", "impl_ref": "m@1", "reason": "r",
                                "owner": "t", "share_budget_pct": 5},
                        consumes={"p": "boolean"}),
                   ],
            edges=[{"from": "esc", "output": "passed", "to": "esc", "input": "p"}])
    add("C_IV40_escape_verifier_selfloop", g, "REJECT",
        "ctl：escape+check 验证面自环照报 E2321（逃逸节点带数据流）", "inv3")

    # ===== C-IV4 评估输入外部化 =====
    def eval_doc(reader=None, writer="W", evaluator=None):
        """store answer(eval_input, writer)；可配读者/评估器。"""
        nodes = [node("W", uses={"model": "m@1"}, writes=["answer"], emits={"fin": "T_out"},
                      context={"sections": [sec_input()]})]
        if evaluator:
            ev = node("ev", kind="check", uses={"unit": "grader@1"}, consumes={"go": "T_out"})
            ev.update(evaluator)
            nodes.append(ev)
        if reader:
            rd = node("rd", emits={"o": "T_out"})
            rd.update(reader)
            nodes.append(rd)
        return doc(meta={"entry": "W", "output": {"ref": "T_out", "from": "W"}},
                   types=[t_in(), t_out()],
                   stores=[store_decl("answer", writer, stype="document", eval_input=True)],
                   nodes=nodes,
                   edges=([{"from": "W", "output": "fin", "to": "ev", "input": "go"}]
                          if evaluator else []))
    add("C_IV41_reader_via_reads", eval_doc(reader={"reads": ["answer"]}), "REJECT",
        "ctl：越权读者 reads→E2331", "inv4")
    add("C_IV42_reader_via_section", eval_doc(
        reader={"context": {"sections": [sec_store("answer")]}}), "REJECT",
        "ctl：越权读者 section→E2331", "inv4")
    add("C_IV43_evaluator_ok", eval_doc(
        evaluator={"reads": ["answer"]}), "PASS", "ctl：check+unit 读者合法", "inv4")
    add("C_IV44_gate_unit_reader", eval_doc(
        evaluator=None, reader={"kind": "gate", "uses": {"unit": "g@1"},
                                "reads": ["answer"]}), "REJECT",
        "ctl：gate+unit 非评估器→E2331", "inv4")
    add("C_IV45_check_nounit_reader", eval_doc(
        reader={"kind": "check", "reads": ["answer"]}), "REJECT",
        "ctl：check 缺 unit 非评估器→E2331", "inv4")
    g = eval_doc()
    g["nodes"][0].pop("uses"); g["nodes"][0].pop("context")
    g["nodes"][0]["reads"] = ["answer"]
    add("C_IV46_writer_reads_own", g, "PASS", "ctl：非 LLM 写者自读合法", "inv4")
    g = doc(meta={"entry": "W", "output": {"ref": "T_out", "from": "W"}},
            types=[t_in(), t_out()], stores=[store_decl("answer", "W", eval_input=True)],
            nodes=[
                node("W", uses={"model": "m@1"}, writes=["answer"], emits={"fin": "T_out"},
                     context={"sections": [sec_input()]}),
                node("c", loop={"body": {"nodes": [
                    node("r", kind="task", reads=["answer"],
                         human=human_decl())], "edges": []},
                    "max_iterations": 1,
                    "safety_points": [{"node": "r", "kind": "human"}]}),
            ])
    add("C_IV47_body_reader", g, "REJECT", "ctl：体内越权读者→E2331", "inv4")
    g = eval_doc()
    g["nodes"][0].pop("uses"); g["nodes"][0].pop("context")
    g["nodes"][0]["reads"] = ["answer"]  # 写者经 reads 取参考答案后由 emits 转发
    g["nodes"][0]["emits"] = {"fin": "T_out", "leak": "T_out"}
    g["nodes"].append(node("downstream", consumes={"go": "T_out"}, emits={"o": "T_out"}))
    g["edges"].append({"from": "W", "output": "leak", "to": "downstream", "input": "go"})
    add("C_IV48_writer_port_forward_leak", g, None,
        "hunt：eval_input 经写者 reads 取入后由 emits 端口转发图下游——端口级数据流静态不可见 "
        "→ 预测 PASS（复核存档：评估输入间接泄漏面）", "inv4")
    g = eval_doc(evaluator={"reads": ["answer"], "emits": {"leak": "T_out"}})
    g["nodes"].append(node("downstream", consumes={"go": "T_out"}, emits={"o": "T_out"}))
    g["edges"].append({"from": "ev", "output": "leak", "to": "downstream", "input": "go"})
    add("C_IV49_evaluator_port_forward_leak", g, None,
        "hunt：评估器读参考答案后 emits 转发（评估器豁免 + 端口转发）→ 预测 PASS（复核存档）", "inv4")
    g = eval_doc(reader={"reads": ["answer"]})
    g["stores"][0]["eval_input"] = False
    add("C_IV50_eval_false", g, "PASS", "ctl：eval_input=false 不判", "inv4")
    g = eval_doc()
    g["nodes"].append(node("w2", writes=["answer"], emits={"o": "T_out"}))
    add("C_IV51_eval_two_writers", g, "REJECT", "ctl：eval store 双写者→E2311", "inv4")
    g = doc(meta={"entry": "W1", "output": {"ref": "T_out", "from": "W1"}},
            types=[t_in(), t_out()],
            stores=[store_decl("a1", "W1", eval_input=True),
                    store_decl("a2", "W2", eval_input=True)],
            nodes=[
                node("W1", writes=["a1"], reads=["a2"], emits={"fin": "T_out"}),
                node("W2", writes=["a2"], emits={"o": "T_out"}),
            ])
    add("C_IV52_cross_writer_read", g, "REJECT",
        "ctl：W1 读 W2 的 eval store→E2331（写者豁免仅限本 store）", "inv4")
    g = doc(meta={"entry": "L", "output": {"ref": "T_out", "from": "L"}},
            types=[t_in(), t_out()], stores=[store_decl("answer", "L.w", eval_input=True)],
            nodes=[
                node("L", emits={"fin": "T_out"}, loop={
                    "body": {"nodes": [
                        node("w", writes=["answer"], reads=["answer"],
                             human=human_decl())], "edges": []},
                    "max_iterations": 1,
                    "carried": [{"store": "answer", "max_items": 2}],
                    "safety_points": [{"node": "w", "kind": "human"}]}),
            ])
    add("C_IV53_writer_in_body_selfread", g, "PASS",
        "ctl：写者在体内自读（路径串相等豁免）合法", "inv4")
    g = doc(meta={"entry": "m", "output": {"ref": "T_out", "from": "m.w"}},
            types=[t_in(), t_out(), t_item()],
            stores=[store_decl("answer", "m.w", eval_input=True)],
            nodes=[
                node("src", emits={"items": "list<T_item>"}),
                node("m", map={"over": "src.items", "body": {"nodes": [
                    node("w", writes=["answer"], emits={"fin": "T_out"})], "edges": []}}),
            ],
            edges=[{"from": "src", "output": "items", "to": "m", "input": "go"}])
    add("C_IV54_writer_in_map_body", g, "REJECT",
        "ctl：写者在 map 体（每实例一写）→E2312；E2331 豁免照旧", "inv4")
    add("C_IV55_eval_never_read", eval_doc(), "PASS",
        "ctl：eval store 无人读（无阅读义务）合法", "inv4")

    # ===== C-RT route =====
    def route_doc(rules, default, edges=None, extra=None):
        r = node("r", uses={"model": "m@1"}, emits={"p": "T_in"},
                 context={"sections": [sec_input()]},
                 route={"rules": rules, "default": default})
        if extra:
            r.update(extra)
        return doc(meta={"entry": "r", "output": {"ref": "T_out", "from": "t"}},
                   types=[t_in(), t_out()],
                   nodes=[r, node("t", consumes={"go": "T_in"}, emits={"fin": "T_out"})],
                   edges=edges if edges is not None else
                   [{"from": "r", "output": "p", "to": "t", "input": "go"}])
    add("C_RT01_uncovered", route_doc([], "exit"), "REJECT",
        "ctl：出边目标未被覆盖→E2101", "route")
    add("C_RT02_goto_dangling", route_doc([{"when": "steps.turns > 1", "goto": "ghost"}], "t"),
        "REJECT", "ctl：goto 悬空→E2102", "route")
    add("C_RT03_dup_rule_target", route_doc(
        [{"when": "steps.turns > 1", "goto": "t"},
         {"when": "steps.turns > 2", "goto": "t"}], "exit"), "REJECT",
        "ctl：同目标双规则→E2103", "route")
    add("C_RT04_rule_default_dup", route_doc(
        [{"when": "steps.turns > 1", "goto": "t"}], "t"), "REJECT",
        "ctl：规则与 default 同目标→E2103（对称缺陷同码）", "route")
    add("C_RT05_multi_exit", route_doc(
        [{"when": "steps.turns > 1", "goto": "exit"},
         {"when": "steps.turns > 2", "goto": "exit"}], "t"), "PASS",
        "ctl：多 exit 规则良性", "route")
    g = route_doc([{"when": "steps.turns > 1", "goto": "exit"}], "exit",
                  edges=[{"from": "r", "output": "p", "to": "exit", "input": "go"}])
    g["nodes"].append(node("exit", consumes={"go": "T_in"}, emits={"fin": "T_out"}))
    g["meta"]["output"]["from"] = "exit"
    add("C_RT06_exit_named_node", g, None,
        "hunt：真实节点就叫 exit + exit 关键字并存——route.rs 归一化对『exit』串双重语义："
        "出边目标 exit 被 exit 分支的 guard_i 覆盖（『exit 分支不产边』注释在节点同名时失真）"
        "→ 预测 PASS（复核存档）", "route")
    g = route_doc([{"when": "steps.turns > 1", "goto": "ghost"}], "exit")
    add("C_RT07_suppression_probe", g, "REJECT",
        "ctl：E2102 在场时 E2101 整体抑制（码集单发验证）", "route")
    add("C_RT08_rules_empty_covered", route_doc([], "t"), "PASS",
        "ctl：无规则 default 全覆盖", "route")
    add("C_RT09_route_on_edgeless", route_doc(
        [{"when": "steps.turns > 1", "goto": "t"}], "exit", edges=[]), "REJECT",
        "ctl：无出边节点 route→E2102（t 不在本地目标）", "route")
    g = doc(meta={"entry": "L", "output": {"ref": "T_out", "from": "L"}},
            types=[t_in(), t_out()],
            nodes=[node("L", emits={"fin": "T_out"}, loop={
                "body": {"nodes": [
                    node("w", emits={"o": "T_out"}),
                    node("ck", kind="check", uses={"unit": "g@1"})],
                    "edges": [{"from": "w", "output": "o", "to": "ck", "input": "go"}]},
                "max_iterations": 2,
                "exit_when": "attempt.passed && steps.turns < 9",
                "safety_points": [{"node": "ck", "kind": "check"}]})])
    add("C_RT10_exit_when_cel", g, "PASS", "ctl：exit_when CEL 位点合法", "route")
    g = route_doc([{"when": "r.p == 'x'", "goto": "t"}], "exit")
    add("C_RT11_self_root_in_route", g, "PASS", "ctl：route when 引自身节点根合法", "route")
    add("C_RT12_goto_same_node_word", route_doc(
        [{"when": "steps.turns > 1", "goto": "same_node"}], "t"), "REJECT",
        "ctl：same_node 是 failures 关键字非 route 目标→E2102", "route")

    # ===== C-CE CEL 保守子集 =====
    def cel_doc(expr, site="stop"):
        g = doc()
        if site == "stop":
            g["policies"] = {"stop": [{"when": expr, "outcome": "completed"}]}
        elif site == "assert":
            g["nodes"][0]["assert"] = expr
        elif site == "route":
            g["nodes"][0]["route"] = {"rules": [{"when": expr, "goto": "n1"}],
                                      "default": "exit"}
        return g
    add("C_CE01_ternary", cel_doc("input.q == 'a' ? true : false"), None,
        "hunt：三元条件式——提案 §1.4 不在保守子集；若 functions() 不登记则漏 → 预测 PASS", "cel")
    add("C_CE02_list_literal", cel_doc("[1, 2] == [1, 2]"), None,
        "hunt：列表字面量是否被算子白名单拦", "cel")
    add("C_CE03_map_literal", cel_doc("{'a': 1} == {'a': 1}"), None,
        "hunt：映射字面量", "cel")
    add("C_CE04_has_macro", cel_doc("has(input.q)"), None,
        "hunt：has() 宏（§1.4 宏不在子集）", "cel")
    add("C_CE05_indexing", cel_doc("input.tags[0] == 'x'"), None,
        "hunt：下标访问（§1.4 下标不在子集）——A10 已回避，此处专测", "cel")
    add("C_CE06_in_op", cel_doc("input.q in ['a', 'b']"), "PASS",
        "ctl：@in 白名单内", "cel")
    add("C_CE07_size_fn", cel_doc("size(input.tags) > 0"), "REJECT",
        "ctl：size()→E1052", "cel")
    add("C_CE08_contains_meth", cel_doc("input.q.contains('a')"), "REJECT",
        "ctl：方法调用→E1052", "cel")
    add("C_CE09_unknown_root", cel_doc("nosuch.x == 1"), "REJECT", "ctl：未知根→E2203", "cel")
    add("C_CE10_steps_badfield", cel_doc("steps.nosuchfield > 0"), "PASS",
        "ctl：字段级词汇不核（求值错误不判——RESULT-M4 文档化偏差，复核存档）", "cel")
    add("C_CE11_empty_expr", cel_doc(""), "REJECT", "ctl：空表达式→E1051", "cel")
    add("C_CE12_trailing_op", cel_doc("1 +"), "REJECT",
        "ctl：尾运算符→E1051（antlr panic 兜底路径）", "cel")
    add("C_CE13_garbage", cel_doc("((("), "REJECT", "ctl：畸形→E1051", "cel")
    add("C_CE14_parens_50", cel_doc("(" * 50 + "true" + ")" * 50), None,
        "探针：括号深 50", "cel")
    add("C_CE15_parens_2000", cel_doc("(" * 2000 + "true" + ")" * 2000), None,
        "探针：括号深 2000（可能栈溢出 crash）", "cel")
    add("C_CE16_parens_100000", cel_doc("(" * 100000 + "true" + ")" * 100000), None,
        "探针：括号深 100000（预计栈溢出 crash——catch_unwind 兜不住）", "cel")
    add("C_CE17_bang_10000", cel_doc("!" * 10000 + "true"), None,
        "探针：万重否定（antlr 错误恢复路径）", "cel")
    add("C_CE18_or_chain_50k", cel_doc("true || " * 50000 + "true"), None,
        "探针：5 万段析取链（递归下降深度）", "cel")
    add("C_CE19_plus_chain_200k", cel_doc("1+" * 200000 + "1"), None,
        "探针：20 万段加链", "cel")
    add("C_CE28_parens_500", cel_doc("(" * 500 + "true" + ")" * 500), None,
        "探针：括号深 500（崩溃阈值夹逼）", "cel")
    add("C_CE29_parens_1000", cel_doc("(" * 1000 + "true" + ")" * 1000), None,
        "探针：括号深 1000（崩溃阈值夹逼）", "cel")
    add("C_CE20_arith_nonbool", cel_doc("1 + 2"), "REJECT",
        "ctl：确定性非布尔→E1053", "cel")
    add("C_CE21_div_zero", cel_doc("1 / 0 > 0"), "PASS",
        "ctl：除零→求值错误不判（文档化边界）", "cel")
    add("C_CE22_unicode_literal", cel_doc("input.q == '中文值'"), "PASS",
        "ctl：unicode 字面量合法", "cel")
    add("C_CE23_inputs_binding", cel_doc("inputs.go == 'x'", site="assert"), None,
        "ctl：assert inputs 绑定（go 端口类型 string→'' == 'x' false→bool）", "cel")
    add("C_CE24_reserved_shadow_node", doc(
        meta={"entry": "n1", "output": {"ref": "T_out", "from": "n1"}},
        nodes=[
            node("input", uses={"model": "m@1"}, emits={"out": "T_out"},
                 context={"sections": [sec_input()]},
                 route={"rules": [{"when": "input.q == 'a'", "goto": "n1"}],
                        "default": "exit"}),
        ]), None,
        "ctl：节点 id 撞保留字 input——全局词表覆盖节点根（RESULT-M4 保留字纪律）", "cel")
    add("C_CE25_global_roots", cel_doc("last_tool.ok == true && last_message.has_tool_calls"),
        "PASS", "ctl：全局五根合法", "cel")
    add("C_CE26_huge_string_literal", cel_doc("input.q == '" + "x" * 1000000 + "'"), None,
        "探针：1MB 字面量", "cel")
    add("C_CE27_nested_exit_when_deep", doc(
        meta={"entry": "L", "output": {"ref": "T_out", "from": "L"}},
        types=[t_in(), t_out()],
        nodes=[node("L", emits={"fin": "T_out"}, loop={
            "body": {"nodes": [node("ck", kind="check", uses={"unit": "g@1"})],
                     "edges": []},
            "max_iterations": 1,
            "exit_when": "attempt.passed",
            "safety_points": [{"node": "ck", "kind": "check"}]})]), "PASS",
        "ctl：exit_when attempt.passed 最小形", "cel")

    # ===== C-ES 逃逸 =====
    def esc_doc(esc=None, kind="task", inside_loop=False):
        e = {"kind": "dynamic_tool_registry", "impl_ref": "d@1", "reason": "r",
             "owner": "team", "share_budget_pct": 10}
        e.update(esc or {})
        if inside_loop:
            return doc(meta={"entry": "L", "output": {"ref": "T_out", "from": "L"}},
                       types=[t_in(), t_out()],
                       stores=[store_decl("S", "L.w")] if False else [store_decl("S", "L.w")],
                       nodes=[node("L", emits={"fin": "T_out"}, loop={
                           "body": {"nodes": [
                               node("w", kind=kind, escape=e, writes=["S"],
                                    emits={"o": "T_out"}, human=human_decl())], "edges": []},
                           "max_iterations": 1,
                           "carried": [{"store": "S", "max_items": 4}],
                           "safety_points": [{"node": "w", "kind": "human"}]})])
        return doc(meta={"entry": "esc", "output": {"ref": "T_out", "from": "esc"}},
                   types=[t_in(), t_out()],
                   nodes=[node("esc", kind=kind, escape=e, uses={"model": "m@1"},
                               emits={"fin": "T_out"}, context={"sections": [sec_input()]})])
    add("C_ES01_legal", esc_doc(), "PASS", "ctl：escape 全字段合法", "escape")
    add("C_ES02_pct_zero", esc_doc({"share_budget_pct": 0}), None,
        "探针：份额 0（无正性核对——复核存档）", "escape")
    add("C_ES03_pct_u32max", esc_doc({"share_budget_pct": U32MAX}), None,
        "探针：份额 u32max（多逃逸合计不核对——复核存档）", "escape")
    add("C_ES04_kind_omitted", esc_doc(None), "PASS", "ctl：kind 缺省（v0.1 可选）", "escape")
    add("C_ES05_kind_junk", esc_doc({"kind": "wormhole"}), "REJECT",
        "ctl：kind 不在七值词表→E1004", "escape")
    add("C_ES06_on_plain_task", esc_doc(kind="task"), "PASS",
        "ctl：escape 挂普通 task（构造字段与 kind 正交口径）", "escape")
    add("C_ES07_in_loop_carried", esc_doc(inside_loop=True), "PASS",
        "ctl：逃逸节点在 loop 体写 carried store 合法", "escape")

    # ===== C-EV 信封/形状语义 =====
    g = doc(); g["version"] = 2
    add("C_EV01_version_2", g, "REJECT", "ctl：E0003", "envelope")
    g = doc(); g["version"] = True
    add("C_EV02_version_bool", g, "REJECT", "ctl：E0002", "envelope")
    g = doc(); g["version"] = 1.0
    add("C_EV03_version_float_1", g, "REJECT", "ctl：1.0 浮点→E0002（非整数路径）", "envelope")
    g = doc(); g["extra"] = 1
    add("C_EV04_unknown_key", g, "REJECT", "ctl：E0004", "envelope")
    g = doc(); g["Meta"] = {}
    add("C_EV05_case_diff_key", g, "REJECT", "ctl：大小写变体键→E0004 且信封短路", "envelope")
    g = doc()
    g["stores"] = [store_decl("S", "n1", value="list<signal>")]
    add("C_EV06_store_value_list_signal", g, "PASS",
        "ctl：value list<signal> 泛型载荷 signal 内建可解析 → 合法", "envelope")


# ---------------------------------------------------------------- D 随机爆破
MUT_LOG = []

def _pick_scalar(rng):
    return rng.choice([
        123, -1, 0, 1.5, True, None, "", "中文🚀", "x" * 500,
        [], {}, [1, "a"], {"a": 1}, U32MAX, U64MAX, float("inf"),
    ])

def _walk_positions(d, prefix="", out=None):
    if out is None:
        out = []
    if isinstance(d, dict):
        for k, v in d.items():
            p = f"{prefix}/{k}"
            out.append((p, v))
            _walk_positions(v, p, out)
    elif isinstance(d, list):
        for i, v in enumerate(d):
            p = f"{prefix}/{i}"
            out.append((p, v))
            _walk_positions(v, p, out)
    return out


def _get(d, path):
    cur = d
    for p in path.split("/")[1:]:
        cur = cur[int(p)] if isinstance(cur, list) else cur[p]
    return cur


def group_D(rng, count):
    baselines = []
    for name, payload, meta in SAMPLES:
        if meta["strategy"] == "baseline" and not isinstance(payload, str):
            baselines.append(copy.deepcopy(payload))
    id_pool = ["n1", "n2", "w1", "ghost", "a.b", "m.b", "c.v", "exit", "节点甲", "x" * 300, ""]
    words = ["same_node", "next_iteration", "exit", "plan", "latest_k", "project",
             "full", "none", "retrieve", "summarize", "check", "gate", "task", "map",
             "transform", "human", "kv", "message_log", "run", "iteration", "persistent"]
    cel_junk = ["(((", "1 +", "&&&", "@", "a b c", "!" * 200, "(" * 60 + "1" + ")" * 60,
                "input.", "steps.", "'unterminated", "\\", "1 in", "size(", "has("]

    for i in range(count):
        g = copy.deepcopy(rng.choice(baselines))
        muts = []
        for _ in range(rng.randint(1, 3)):
            op = rng.choice(["flip", "del", "dup_elem", "swap_ids", "rewire",
                             "retarget", "cel_junk", "word", "zero", "enum_flip",
                             "dup_key_name", "port_rename"])
            pos = _walk_positions(g)
            if not pos:
                continue
            p, v = rng.choice(pos)
            try:
                if op == "flip":
                    _set(g, p, _pick_scalar(rng))
                    muts.append(f"flip@{p}")
                elif op == "del":
                    _del(g, p)
                    muts.append(f"del@{p}")
                elif op == "dup_elem" and isinstance(v, list) and v:
                    v.insert(rng.randrange(len(v)), copy.deepcopy(rng.choice(v)))
                    muts.append(f"dup_elem@{p}")
                elif op == "swap_ids":
                    ids = [pp for pp, vv in pos if pp.endswith("/id") and isinstance(vv, str)]
                    if len(ids) >= 2:
                        a, b = rng.sample(ids, 2)
                        va, vb = _get(g, a), _get(g, b)
                        _set(g, a, vb); _set(g, b, va)
                        muts.append(f"swap_ids {va}<->{vb}")
                elif op == "rewire":
                    _set(g, p, rng.choice(id_pool))
                    muts.append(f"rewire@{p}={id_pool if False else ''}")
                elif op == "retarget" and isinstance(v, str) and len(v) < 100:
                    _set(g, p, rng.choice(id_pool + words))
                    muts.append(f"retarget@{p}")
                elif op == "cel_junk" and isinstance(v, str) and len(v) < 200 and \
                        any(k in p for k in ("when", "assert", "goto", "use", "writer", "entry")):
                    _set(g, p, v + rng.choice(cel_junk))
                    muts.append(f"cel_junk@{p}")
                elif op == "word" and isinstance(v, str) and len(v) < 60:
                    _set(g, p, rng.choice(words))
                    muts.append(f"word@{p}")
                elif op == "zero" and isinstance(v, (int, float)) and not isinstance(v, bool):
                    _set(g, p, rng.choice([0, -1, U32MAX, U64MAX, -U64MAX]))
                    muts.append(f"zero@{p}")
                elif op == "enum_flip" and isinstance(v, str) and len(v) < 40:
                    _set(g, p, rng.choice(words + ["", " jaws ", "ΛΛ", "null"]))
                    muts.append(f"enum_flip@{p}")
                elif op == "dup_key_name" and isinstance(v, str) and len(v) < 60:
                    _set(g, p, _get(g, rng.choice(pos)[0])
                         if isinstance(_get(g, rng.choice(pos)[0]), str) else v)
                    muts.append(f"dup_key_name@{p}")
                elif op == "port_rename" and isinstance(v, str) and len(v) < 60:
                    _set(g, p, rng.choice(["go", "passed", "done", "out", "p", "signal", ""]))
                    muts.append(f"port_rename@{p}")
            except Exception:
                continue
        add(f"D_{i:03d}_blast", g, None,
            "随机爆破 " + "; ".join(muts) if muts else "随机爆破(空变异)", "blast")


def _set(d, path, val):
    parts = path.split("/")
    cur = d
    for p in parts[1:-1]:
        cur = cur[int(p)] if isinstance(cur, list) else cur[p]
    last = parts[-1]
    if isinstance(cur, list):
        cur[int(last)] = val
    else:
        cur[last] = val


def _del(d, path):
    parts = path.split("/")
    cur = d
    for p in parts[1:-1]:
        cur = cur[int(p)] if isinstance(cur, list) else cur[p]
    last = parts[-1]
    if isinstance(cur, list):
        cur.pop(int(last))
    else:
        cur.pop(last, None)


# ---------------------------------------------------------------- main

def main():
    outdir = sys.argv[1] if len(sys.argv) > 1 else "samples"
    os.makedirs(outdir, exist_ok=True)
    group_A()
    group_B()
    group_C()
    rng = random.Random(SEED)
    fixed = len(SAMPLES)
    target = 500
    group_D(rng, max(0, target - fixed))
    total = len(SAMPLES)
    assert total == target, f"样本数 {total} != 500"
    manifest = []
    for name, payload, meta in SAMPLES:
        if isinstance(payload, str):
            text = payload
        else:
            try:
                text = dump(payload)
            except Exception as e:
                raise RuntimeError(f"dump 失败 {name}: {type(e).__name__} {e}") from e
        fname = f"{name}.yaml"
        with open(os.path.join(outdir, fname), "w", encoding="utf-8") as f:
            f.write(text)
        manifest.append({"file": fname, "group": name.split("_")[0],
                         "strategy": meta["strategy"], "expect": meta["expect"],
                         "note": meta["note"]})
    with open(os.path.join(outdir, "..", "manifest.jsonl"), "w", encoding="utf-8") as f:
        for m in manifest:
            f.write(json.dumps(m, ensure_ascii=False) + "\n")
    from collections import Counter
    print(f"GEN_OK version={GEN_VERSION}")
    print("total", total)
    print("by_strategy", dict(Counter(m["strategy"] for m in manifest)))
    print("by_group", dict(Counter(m["group"] for m in manifest)))
    print("expect_pass", sum(1 for m in manifest if m["expect"] == "PASS"))
    print("expect_reject", sum(1 for m in manifest if m["expect"] == "REJECT"))
    print("expect_none", sum(1 for m in manifest if m["expect"] is None))


if __name__ == "__main__":
    main()

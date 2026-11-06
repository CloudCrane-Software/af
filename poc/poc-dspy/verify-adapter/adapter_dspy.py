#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""薄适配层（Python/DSPy 侧）：Rust 下发任务 JSON → 动态构造 dspy 程序 → 确定性执行 → dump_state 回传。

边界约定（细节见 verify-adapter/REPORT.md）：
- 本脚本不做 IR 语义校验：字段名齐全性、顺序对齐、无未知字段、demos 数量一致等
  一律由 Rust 校验器负责（DSPy 自身 delete 静默 no-op、存档无字段名、load 按位置 zip）。
- prefix 通过 json_schema_extra 设置：dspy 3.4.1 的 InputField(prefix=...) 已弃用且无效，
  而官方 load_state 恢复 prefix 走的正是 json_schema_extra["prefix"]（signature.py:542）。
- 执行不联网：DummyLM 按 execute 列表顺序回放 lm_script（键 = 字段名，非 prefix），
  每个执行项恰好一次 LM 调用；这是真实 forward 路径（经 ChatAdapter 解析）。
- chaos 在执行之后、dump 之前施加，模拟"Python 侧程序被漂移后回传"；
  全部走 dspy 真实改写 API（delete/insert/append + demos 直接追加），不伪造 JSON。

用法：adapter_dspy.py <task.json>  → stdout 输出单个 JSON（canonical：sort_keys + 紧凑分隔符）
"""
import json
import sys

import dspy
from dspy import InputField, OutputField
from dspy.utils import DummyLM


def build_signature(sig_ir):
    """IR signature → dspy.Signature 子类（字段名/顺序/desc 来自 IR，prefix 走 json_schema_extra）。"""
    ns = {"__annotations__": {f["name"]: str for f in sig_ir["fields"]}}
    for f in sig_ir["fields"]:
        cls = InputField if f["kind"] == "input" else OutputField
        ns[f["name"]] = cls(desc=f["desc"])
    sig = type("DynSig", (dspy.Signature,), ns).with_instructions(sig_ir["instructions"])
    for f in sig_ir["fields"]:
        sig.fields[f["name"]].json_schema_extra["prefix"] = f["prefix"]
    return sig


def input_names(sig_ir):
    return [f["name"] for f in sig_ir["fields"] if f["kind"] == "input"]


def output_names(sig_ir):
    return [f["name"] for f in sig_ir["fields"] if f["kind"] == "output"]


def build(node, reg, prefix):
    """递归构造非根节点。reg[路径] = {kind, ir, obj, predict}；路径 = 父路径.子id，
    根节点(kind=module)的 id 不进入路径（与 dump_state 键一致）。"""
    here = f"{prefix}{node['id']}"
    kind = node["kind"]
    if kind == "module":
        obj = dspy.Module()
        for ch in node.get("children", []):
            setattr(obj, ch["id"], build(ch, reg, here + "."))
        return obj
    if kind not in ("Predict", "ChainOfThought"):
        raise ValueError(f"未知模块类型: {kind}")
    sig = build_signature(node["signature"])
    obj = dspy.Predict(sig) if kind == "Predict" else dspy.ChainOfThought(sig)
    predict_obj = obj if kind == "Predict" else obj.predict
    for d in node.get("demos", []):
        predict_obj.demos.append(
            dspy.Example(**{**d["inputs"], **d["outputs"]}).with_inputs(*input_names(node["signature"]))
        )
    reg[here] = {"kind": kind, "ir": node, "obj": obj, "predict": predict_obj}
    return obj


def eff_output_names(entry):
    """生效输出字段名：IR 声明 + ChainOfThought 内建派生的 reasoning。"""
    names = output_names(entry["ir"]["signature"])
    if entry["kind"] == "ChainOfThought":
        names = names[:-1] + ["reasoning"] + names[-1:]
    return names


def apply_chaos(chaos, reg):
    """在执行后、dump 前施加漂移，全部走 dspy 真实改写 API（模拟 Python 侧程序被改）。"""
    if not chaos:
        return
    entry = reg[chaos["module"]]
    target = entry["predict"]
    sig = target.signature
    op = chaos["op"]
    if op == "delete_field":
        new_sig = sig.delete(chaos["field"])  # dspy 真实 delete API
    elif op == "swap_fields":
        a, b = chaos["a"], chaos["b"]
        fa = sig.fields[a]
        names = list(sig.fields.keys())
        a_first = names.index(a) < names.index(b)  # a 原本在 b 前？
        shortened = sig.delete(a)  # 先删 a
        pos = list(shortened.fields.keys()).index(b) + (1 if a_first else 0)
        new_sig = shortened.insert(pos, a, fa, type_=str)
        # 隔离变量：只留"顺序"这一个漂移维度，补回 a 的元数据（insert 不保 json_schema_extra）
        new_sig.fields[a].json_schema_extra["prefix"] = fa.json_schema_extra["prefix"]
        new_sig.fields[a].json_schema_extra["desc"] = fa.json_schema_extra.get("desc", "")
    elif op == "add_field":
        new_sig = sig.append(chaos["name"], OutputField(desc=chaos["desc"]))  # dspy 真实 append API
    elif op == "add_demo":
        target.demos.append(
            dspy.Example(**chaos["demo"]).with_inputs(*chaos["input_names"])
        )
        return
    else:
        raise ValueError(f"未知 chaos op: {op}")
    target.signature = new_sig


def main():
    with open(sys.argv[1], encoding="utf-8") as f:
        task = json.load(f)
    reg = {}
    root_ir = task["program"]["root"]
    if root_ir["kind"] != "module":
        raise ValueError("program.root 必须是 kind=module 的容器")
    root = dspy.Module()
    for ch in root_ir.get("children", []):
        setattr(root, ch["id"], build(ch, reg, ""))

    # 执行：所有 lm_script 按序拼成一个 DummyLM 回放脚本；每个 execute 项 = 一次真实 forward
    exec_outputs = []
    if task.get("execute"):
        lm_script = []
        for e in task["execute"]:
            if len(e["lm_script"]) != 1:
                raise ValueError("约定：每个 execute 项恰好一次 LM 调用（lm_script 长度 = 1）")
            lm_script.extend(e["lm_script"])
        with dspy.settings.context(lm=DummyLM(lm_script)):
            for e in task["execute"]:
                entry = reg[e["module"]]
                pred = entry["obj"](**e["inputs"])
                outs = {k: str(getattr(pred, k)) for k in eff_output_names(entry)}
                exec_outputs.append({"module": e["module"], "outputs": outs})
                if e.get("append_result_as_demo"):
                    demo = {**e["inputs"], **outs}
                    entry["predict"].demos.append(
                        dspy.Example(**demo).with_inputs(*input_names(entry["ir"]["signature"]))
                    )

    apply_chaos(task.get("chaos"), reg)

    resp = {
        "task_id": task["task_id"],
        "state": root.dump_state(),
        "exec_outputs": exec_outputs,
    }
    sys.stdout.write(json.dumps(resp, ensure_ascii=False, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()

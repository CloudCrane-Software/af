# -*- coding: utf-8 -*-
"""PoC (id=poc-dspy): 验证 DSPy 3.4.1 可作 JIT 的 IR 底座（不调 LLM）。

验证命题：'四元组 = schema 化数据 + 可枚举改写 + JSON 存取'
  ① 定义 Signature（输入输出字段 + 指令）——schema 化数据
  ② 7 个改写 API 各调一次，断言返回"新类型"且原类型不被就地修改——可枚举改写
  ③ Module dump_state/load_state 往返一致
  ④ JSON 序列化存文件再加载，比较等价——JSON 存取
  ⑤ teleprompt 优化器清单（配合 shell: grep __all__）
全程不配置、不调用任何 LLM。
"""
import inspect
import json
import sys

import dspy
from dspy import InputField, OutputField, Signature

PASS, FAIL = [], []


def check(name, cond, detail=""):
    (PASS if cond else FAIL).append(name)
    mark = "PASS" if cond else "FAIL"
    print(f"[{mark}] {name}" + (f"  | {detail}" if detail else ""))
    return cond


print(f"== env: dspy=={dspy.__version__}, python={sys.version.split()[0]} ==")

# ---------- ① 定义 Signature：schema 化数据 ----------
class QA(Signature):
    """根据上下文回答问题。"""  # 这一行就是 instructions
    context: str = InputField(desc="相关背景资料")
    question: str = InputField(desc="用户的问题")
    answer: str = OutputField(desc="一句话答案")

sig = QA  # Signature 子类本身即 IR 节点

check("①-1 input_fields 可枚举",
      list(QA.input_fields.keys()) == ["context", "question"],
      f"{list(QA.input_fields.keys())}")
check("①-2 output_fields 可枚举",
      list(QA.output_fields.keys()) == ["answer"],
      f"{list(QA.output_fields.keys())}")
check("①-3 instructions 来自 docstring",
      QA.instructions == "根据上下文回答问题。", repr(QA.instructions))
check("①-4 字段带类型+desc（schema 元数据完整）",
      QA.fields["context"].annotation is str
      and QA.fields["answer"].json_schema_extra["desc"] == "一句话答案")
check("①-5 Signature.dump_state 可 dump",
      isinstance(sig.dump_state(), dict), str(sig.dump_state())[:80])

# ---------- ② 7 个改写 API：全部返回新类型 ----------
orig_instr = QA.instructions
orig_inputs = list(QA.input_fields.keys())
orig_outputs = list(QA.output_fields.keys())

def is_new_type(r, orig):
    return inspect.isclass(r) and issubclass(r, Signature) and r is not orig

def unmutated():
    return (QA.instructions == orig_instr
            and list(QA.input_fields.keys()) == orig_inputs
            and list(QA.output_fields.keys()) == orig_outputs)

# 2.1 with_instructions
s1 = QA.with_instructions("改写后的总指令：只输出 JSON。")
check("②-1 with_instructions 返回新类型", is_new_type(s1, QA))
check("②-1a 新类型指令生效", s1.instructions == "改写后的总指令：只输出 JSON。")
check("②-1b 原类型未被修改", unmutated())

# 2.2 append_instructions
s2 = QA.append_instructions("追加：答案不超过 20 字。")
check("②-2 append_instructions 返回新类型", is_new_type(s2, QA))
check("②-2a 指令为追加而非替换（分隔符为空行 \\n\\n）",
      s2.instructions == orig_instr + "\n\n追加：答案不超过 20 字。", repr(s2.instructions))
check("②-2b 原类型未被修改", unmutated())

# 2.3 with_updated_fields
s3 = QA.with_updated_fields("question", desc="更新后的问题描述")
check("②-3 with_updated_fields 返回新类型", is_new_type(s3, QA))
check("②-3a 新类型字段 desc 已更新",
      s3.fields["question"].json_schema_extra["desc"] == "更新后的问题描述")
check("②-3b 原类型字段 desc 不变",
      QA.fields["question"].json_schema_extra["desc"] == "用户的问题")

# 2.4 prepend（前置一个输出字段）
s4 = QA.prepend("confidence", OutputField(desc="置信度 0~1"))
check("②-4 prepend 返回新类型", is_new_type(s4, QA))
check("②-4a 新类型 outputs 前插成功",
      list(s4.output_fields.keys()) == ["confidence", "answer"],
      f"{list(s4.output_fields.keys())}")
check("②-4b 原类型 outputs 不变", list(QA.output_fields.keys()) == orig_outputs)

# 2.5 append（尾部追加输入字段）
s5 = QA.append("hint", InputField(desc="可选提示"))
check("②-5 append 返回新类型", is_new_type(s5, QA))
check("②-5a 新类型 inputs 尾插成功",
      list(s5.input_fields.keys()) == ["context", "question", "hint"],
      f"{list(s5.input_fields.keys())}")
check("②-5b 原类型 inputs 不变", list(QA.input_fields.keys()) == orig_inputs)

# 2.6 delete
s6 = QA.delete("answer")
check("②-6 delete 返回新类型", is_new_type(s6, QA))
check("②-6a 新类型已删 answer", "answer" not in s6.output_fields)
check("②-6b 原类型仍含 answer", "answer" in QA.output_fields)

# 2.7 insert（指定位置插入输入字段）
s7 = QA.insert(1, "lang", InputField(desc="目标语言"), type_=str)
check("②-7 insert 返回新类型", is_new_type(s7, QA))
check("②-7a 新类型 inputs 按位插入",
      list(s7.input_fields.keys()) == ["context", "lang", "question"],
      f"{list(s7.input_fields.keys())}")
check("②-7b 原类型 inputs 不变", list(QA.input_fields.keys()) == orig_inputs)

# 改写可链式组合（JIT pipeline 关键性质）
chain = QA.append("hint", InputField()).with_instructions("链式指令").delete("hint")
check("②-8 改写可链式组合且返回新类型", is_new_type(chain, QA)
      and chain.instructions == "链式指令" and "hint" not in chain.input_fields)

# 语义副作用探察：delete 不存在的字段（对 IR 校验器有影响）
s6b = QA.delete("不存在的字段")
check("②-9 坑：delete 缺失字段不报错（静默 no-op）",
      s6b.output_fields.keys() == QA.output_fields.keys(),
      "对严格 IR 需在 Rust 侧先校验字段存在")

# ---------- ③ Module dump_state/load_state 往返 ----------
class MyProgram(dspy.Module):
    def __init__(self):
        super().__init__()
        self.answerer = dspy.Predict(QA)               # 顶层 Predict
        self.cot = dspy.ChainOfThought(QA)             # 嵌套 Module（内含 Predict）
        self.brief = dspy.Predict("question -> brief") # 字符串 mini-syntax 签名

prog = MyProgram()
# 塞入非空状态（demos），保证往返测试有内容可比
prog.answerer.demos = [dspy.Example(context="ctx-A", question="q-A", answer="a-A").with_inputs("context", "question")]
prog.cot.predict.demos = [dspy.Example(context="ctx-B", question="q-B", reasoning="r-B", answer="a-B").with_inputs("context", "question")]
prog.brief.demos = [dspy.Example(question="q-A", brief="b-A").with_inputs("question")]

state1 = prog.dump_state()  # 默认 json_mode=True
prog2 = MyProgram()         # 全新实例（状态为空/默认）
prog2.load_state(state1)
state2 = prog2.dump_state()
check("③-1 dump→load→dump 往返一致（含嵌套 Module+非空 demos）", state1 == state2)

# 自身幂等：load 回原实例再 dump 也一致
prog.load_state(state1)
check("③-2 load 幂等（load 两次后 dump 不变）", prog.dump_state() == state1)

# Predict 状态里确实携带 schema（signature 指令/字段）——IR 完整性证据
p_state = prog.answerer.dump_state()
sig_state = p_state["signature"]
check("③-3 Predict 状态含 schema（instructions + 每字段 prefix/description）",
      sig_state["instructions"] == orig_instr
      and [f["prefix"] for f in sig_state["fields"]] == ["Context:", "Question:", "Answer:"]
      and [f["description"] for f in sig_state["fields"]] == ["相关背景资料", "用户的问题", "一句话答案"],
      f"keys={sorted(p_state.keys())}")
# 已发现的坑：signature 状态不含字段名，load 靠位置 zip 恢复（signature.py:524-541）
check("③-4 坑：Signature 状态不含字段名（仅 prefix/description，位置恢复）",
      all("name" not in f for f in sig_state["fields"]),
      "Rust 侧 IR 必须显式携带字段名，不能指望 DSPy 存档兜底")

# ---------- ④ JSON 序列化：存文件→加载→等价 ----------
blob = json.dumps(state1, ensure_ascii=False, sort_keys=True)
path = __file__.rsplit("/", 1)[0] + "/poc_program_state.json"
with open(path, "w", encoding="utf-8") as f:
    f.write(blob)
with open(path, encoding="utf-8") as f:
    loaded = json.load(f)
prog3 = MyProgram()
prog3.load_state(loaded)
blob2 = json.dumps(prog3.dump_state(), ensure_ascii=False, sort_keys=True)
check("④-1 JSON 存文件→加载→再序列化 字节级等价", blob2 == blob,
      f"{len(blob)} bytes")
with open(path, encoding="utf-8") as f:
    check("④-2 文件内容可直接 json.load 且为纯 JSON", json.load(f) == state1)

# ---------- ⑤ teleprompt 优化器清单 ----------
from dspy.teleprompt import __all__ as tp_all
print(f"[INFO] dspy.teleprompt.__all__ ({len(tp_all)}): {sorted(tp_all)}")
expect_all = {"AvatarOptimizer", "BetterTogether", "BootstrapFewShot",
              "BootstrapFinetune", "COPRO", "Ensemble", "GEPA", "KNNFewShot",
              "MIPROv2", "BootstrapFewShotWithRandomSearch",
              "BootstrapFewShotWithOptuna", "LabeledFewShot", "InferRules",
              "SIMBA", "bootstrap_trace_data"}  # 与 grep __init__.py:36-52 一致
check("⑤-1 teleprompt.__all__ 与 grep 源码清单一致（14 优化器+1 函数，Teleprompter 基类不在 __all__）",
      expect_all == set(tp_all), f"{len(tp_all)} 个符号")
# 优化器均可实例化候选（仅列名，不 compile——compile 需要 metric/LM）
check("⑤-2 优化器均为可枚举符号（import 可达）",
      all(hasattr(__import__("dspy.teleprompt", fromlist=[n]), n) for n in tp_all))

# ---------- 汇总 ----------
print("\n==== 汇总 ====")
print(f"PASS {len(PASS)} / 总 {len(PASS)+len(FAIL)}")
if FAIL:
    print("FAILED:", FAIL)
    sys.exit(1)
print("ALL CHECKS PASSED")

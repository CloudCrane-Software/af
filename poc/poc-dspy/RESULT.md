# PoC 结果：DSPy IR 结构验证（id=poc-dspy）

## ① 结论

**通过**（36/36 断言全部真实执行通过，exit 0）。

命题 **"四元组 = schema 化数据 + 可枚举改写 + JSON 存取" 成立**。环境：dspy 3.4.1 / Python 3.14.4，全程未配置、未调用任何 LLM。

**边界建议：IR 校验器放 Rust。** DSPy 改写 API 是纯函数式且语义简单（Python 侧安全），但校验恰恰是它缺的（delete 缺失字段静默 no-op；存档不含字段名）。IR 的 source of truth、字段名校验、改写合法性检查应放 Rust，Python 侧 DSPy 只作执行/优化引擎。

## ② 做了什么

产物：`poc_dspy_ir.py`（36 项断言）、`poc_program_state.json`（dump 出的纯 JSON 存档）。

1. **①定义 Signature**：`QA(Signature)`，2 输入字段（context/question, str+desc）+ 1 输出字段（answer）+ docstring 指令；验证 `input_fields`/`output_fields`/`instructions`/`fields` 元数据全部可枚举。
2. **②7 个改写 API 各调一次**（`with_instructions` / `append_instructions` / `with_updated_fields` / `prepend` / `append` / `delete` / `insert`，均为 `dspy/signatures/signature.py` 的 classmethod，L278/307/336/362/389/416/451）：逐一断言 ①返回新类型（class 且非原对象）②语义生效 ③原类型零修改；另测链式组合、以及 `delete` 缺失字段的静默行为。
3. **③Module 往返**：自写 `MyProgram(dspy.Module)`（Predict + ChainOfThought 嵌套 + 字符串 mini-syntax 签名三个子模块），填入非空 demos 后 `dump_state→load_state→dump_state` 逐字段相等；另验 load 幂等、Predict 状态携带 schema。
4. **④JSON 存取**：`dump_state()`（默认 json_mode）→ `json.dumps` → 写 `poc_program_state.json` → 读回 → 载入全新实例 → 再 dump → **字节级等价**。
5. **⑤优化器清单**：脚本内读 `dspy.teleprompt.__all__`，shell `grep -n -A18 "^__all__" teleprompt/__init__.py`（L36-52）双重确认一致。

## ③ 证据

运行：`.venv/bin/python poc_dspy_ir.py` → `PASS 36 / 总 36`，`ALL CHECKS PASSED`。摘录：

```
== env: dspy==3.4.1, python=3.14.4 ==
[PASS] ①-3 instructions 来自 docstring  | '根据上下文回答问题。'
[PASS] ②-1 with_instructions 返回新类型
[PASS] ②-2a 指令为追加而非替换（分隔符为空行 \n\n）
[PASS] ②-4a 新类型 outputs 前插成功  | ['confidence', 'answer']
[PASS] ②-7a 新类型 inputs 按位插入  | ['context', 'lang', 'question']
[PASS] ②-8 改写可链式组合且返回新类型
[PASS] ②-9 坑：delete 缺失字段不报错（静默 no-op）
[PASS] ③-1 dump→load→dump 往返一致（含嵌套 Module+非空 demos）
[PASS] ③-4 坑：Signature 状态不含字段名（仅 prefix/description，位置恢复）
[PASS] ④-1 JSON 存文件→加载→再序列化 字节级等价  | 1033 bytes
[INFO] dspy.teleprompt.__all__ (15): ['AvatarOptimizer', 'BetterTogether',
  'BootstrapFewShot', 'BootstrapFewShotWithOptuna', 'BootstrapFewShotWithRandomSearch',
  'BootstrapFinetune', 'COPRO', 'Ensemble', 'GEPA', 'InferRules', 'KNNFewShot',
  'LabeledFewShot', 'MIPROv2', 'SIMBA', 'bootstrap_trace_data']
==== 汇总 ====
PASS 36 / 总 36
```

grep 证据：`teleprompt/__init__.py:36-52` 的 `__all__` 与运行时清单一致（14 优化器 + 1 函数 `bootstrap_trace_data`；`Teleprompter` 基类**不在** `__all__`）。存档实例：`answerer.signature = {"fields":[{"description":"相关背景资料","prefix":"Context:"},...],"instructions":"根据上下文回答问题。"}`。

## ④ 对最终方案的反馈

**命题成立**，三个支腿都有硬证据：
- *schema 化数据*：Signature 子类即 IR 节点，字段/类型/desc/instructions 全可枚举（①-1~5）。
- *可枚举改写*：7 API 全部不可变、返回新类型、原类型零扰动、可链式（②-1~8）——天然适配 JIT 的"每步改写产出新 IR 版本"。
- *JSON 存取*：`dump_state` 默认纯 JSON，嵌套 Module + demos 往返字节级一致（③-1、④-1）。

**边界建议（IR 校验器放 Rust），理由三条：**
1. **DSPy 放弃了校验职责**：`delete` 对缺失字段 `pop(name, None)` 静默成功（signature.py:441-444）；`load_state` 按位置 `zip(..., strict=False)` 恢复字段（signature.py:538），字段错位不报错。严格 IR 需要的"删必须存在、改写后名字唯一"在 Python 侧不存在，只能 Rust 做。
2. **DSPy 存档有损、不可作 IR source of truth**：`Signature.dump_state()` 只存每字段的 `prefix`/`description`，**不含字段名**（signature.py:524-534，③-4）。字段名只活在 Python 代码结构里。Rust 内核必须显式持有完整 IR（含字段名），向下发 JSON 给 Python 重建，而不是反向"从 DSPy 导出 IR"。
3. **改写语义简单，双实现无漂移风险**：7 个 API 一一对应纯字典操作（copy→改→`Signature(fields, instructions)` 重建），Rust 复刻无难点；接口命名照抄这 7 个名字即可。

**接口建议**：Rust 定 IR JSON schema（signature: instructions + fields[{name,type,desc,kind}]；program: 模块树+每节点 signature 引用）；Python 薄适配层只做 `Rust JSON → dspy.Signature/Module 动态构造 → (可选优化) → 状态回传`，回传后由 Rust 校验。DSPy 的 `Module.load_state` 先 deepcopy 试跑再应用（base_module.py:160-169，失败不污染原对象），事务语义值得 Rust 端照抄。

**顺手发现的坑**：`append_instructions` 分隔符是空行 `\n\n` 不是 `\n`；`insert` 支持负索引、越界抛 ValueError；`GEPA` 在 Python≥3.15 无 gepa 包时是会 raise 的占位类（`__init__.py:21-33` 的条件导入）——版本升级要盯。

## ⑤ 遗留问题

1. **优化器 compile 路径未验证**（需 metric/LM，PoC 约束不调 LLM）——仅确认 14 个优化器符号可枚举可达，`MIPROv2` 等能否无 LM 构造未测。
2. **`dspy.ProgramOfThought` 未测**（ask 二选一，选了自写 Module + Predict/ChainOfThought，后者覆盖嵌套结构更有代表性）。
3. **Rust IR schema 细节未定**：demos（few-shot 样例）是否入 IR、LM 配置如何隔离在存档外（本次 `lm` 字段为 null），需在正式设计时拍板。
4. **性能未测**：每次改写全量拷贝 fields dict，JIT 高频链式改写在 Python 侧的 GC 压力未知——若 Rust 主导改写则此问题消解。
5. 环境为 Python 3.14.4，dspy 3.4.1 部分新特性（如 GEPA 真包）在 3.15+ 行为不同，固定 Python 版本时注意。

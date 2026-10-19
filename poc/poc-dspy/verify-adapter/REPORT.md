# 验证报告：DSPy 适配层一次真实往返（id=dspy-adapter）

前置阅读：`AF/poc/poc-dspy/RESULT.md`（结论：IR 真相源与校验器放 Rust；坑：delete 缺字段静默 no-op、存档不含字段名、load_state 按位置 zip 错位不报错）。
环境：dspy 3.4.1 / Python 3.14.4（`AF/poc/poc-dspy/.venv`，未装新包）、rustc/cargo 1.99.1、全程未配置未调用任何真实 LLM。

## ① 结论

**通过**（最终套件 `cargo run -- all` 6/6 案例全部真实执行通过，exit 0）。

DSPy 适配层一次真实往返已完整打通并验证：**Rust 定义 IR 并下发执行任务 JSON → Python 薄适配层动态构造 dspy 程序、经 DummyLM 走真实 forward（不联网）→ dump_state 回传 → Rust 9 规则校验器判 VALID**。4 类漂移（删字段/交换字段顺序/多塞字段/多塞 demo）注入后校验器逐个抓住，且规则归因精确；对照实验复演了 DSPy 自身对同类漂移完全静默（delete 缺字段 no-op、load_state 接受错位顺序均不报错）。同一 IR 两次往返响应 990 字节**字节级相等**（Rust 内存比对 + shell `cmp` + sha256 三方一致）。

最关键的决策输入：**IR schema v1 字段清单已可定稿**（见 ④-1），**字段名齐全性、位置对齐、无未知字段、demos 数量/键名、执行输出预期这五类校验必须放 Rust**（见 ④-2）——其中位置对齐是唯一能抓住"字段顺序漂移"的手段，而 DSPy 存档根本没有字段名，Python 侧无从校验。

## ② 步骤（做了什么）

任务目录：`AF/poc/poc-dspy/verify-adapter/`（独立 cargo 工程，Cargo.toml 带空 `[workspace]` 表脱离 AF workspace）。

产物：
- `src/main.rs`（720 行）：IR schema serde 类型、基线 IR/任务生成、Python 子进程编排、9 规则校验器、6 案例 runner、字节级比对
- `adapter_dspy.py`（151 行）：Python 薄适配层（构造/执行/漂移注入/回传，不做任何一致性校验）
- `out/`：`ir_base.json`、`task_*.json`×6、`resp_*.json`×6、`roundtrip_{1,2}.json`

流程（对应任务步骤）：
1. **①IR schema + 任务生成**：Rust 侧定义 IR（模块树：根容器 `module` → 子节点 `Predict`/`ChainOfThought`，签名含字段名/类型/顺序/demos），生成执行任务（`program` + `execute`：每项一次 LM 调用的确定性 `lm_script` + `append_result_as_demo` 语义 + 可选 `chaos` 注入），写 `out/task_*.json`。
2. **②Python 执行**：`adapter_dspy.py <task.json>`——动态构造 Signature（`__annotations__` + `InputField/OutputField(desc)`，prefix 经 `json_schema_extra` 设置）、裸 `dspy.Module()` 容器按树 setattr、`DummyLM` 按 execute 顺序回放、真实 `forward` 经 ChatAdapter 解析、`dump_state()` 以 canonical JSON（`sort_keys` + 紧凑分隔符 + `ensure_ascii=False`）写 stdout。
3. **③Rust 校验器**：R9 条规则（R0 回显、R1 模块集合、R2 字段数、R3 位置对齐、R4 无未知字段、R5 demos 数量、R6 demos 键名=字段名、R7 执行输出=lm_script 预期、R8 执行产物 demo 存在、R9 instructions），Rust 从 IR 推导期望模型（含 CoT 派生 `reasoning` 字段规则）。
4. **④漂移实验**：4 个 chaos 案例由 Rust 驱动，Python 用 **dspy 真实改写 API**（`delete`/`insert`/`append` + demos 追加）在执行后、dump 前注入漂移，模拟"Python 侧程序被优化器/bug 改动后回传"；校验器须逐个抓住指定主规则。
5. **⑤字节级比对**：`twice` 案例以同一任务文件跑两次 Python，比对原始 stdout 字节。
6. **对照实验**：用本会话同一签名直接复演 DSPy 两类静默坑，作为校验器必要性的反证。

## ③ 证据（命令原文 + 输出摘录）

**构建**：
```
$ source ~/.cargo/env && cd verify-adapter && cargo build -j4
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.07s   # 退出码 0
```

**全量套件**（`cargo run -q -j4 -- all`，exit 0；完整输出存于会话，摘要）：
```
== case base ==
  [ ok ] R0_envelope ... [ ok ] R8_appended_demo   （10 条规则全 ok）
  VERDICT: VALID（期望 VALID）
  raw 响应 989 字节 → out/resp_base.json
  case base: PASS
== case chaos-delete-field ==
  [CATCH] R2_field_count | [answerer] 字段数 2 ≠ 期望 3（实得["背景:", "问题:"] 期望["背景:", "问题:", "答案:"] 缺["答案:"]）；
  VERDICT: INVALID（期望 INVALID 且抓到 R2_field_count）fired=["R2_field_count"]
  case chaos-delete-field: PASS
== case chaos-swap-fields ==
  [CATCH] R3_field_position | [answerer] 位置0 prefix "问题:" ≠ 期望 "背景:"(字段 "context")；
          [answerer] 位置0 description "用户的问题" ≠ 期望 "相关背景资料"(字段 "context")；
          [answerer] 位置1 prefix "背景:" ≠ 期望 "问题:"(字段 "question")；
          [answerer] 位置1 description "相关背景资料" ≠ 期望 "用户的问题"(字段 "question")；
  VERDICT: INVALID（期望 INVALID 且抓到 R3_field_position）fired=["R3_field_position"]
  case chaos-swap-fields: PASS
== case chaos-add-field ==
  [CATCH] R2_field_count | [answerer] 字段数 4 ≠ 期望 3（实得["背景:", "问题:", "答案:", "Bonus:"] 期望["背景:", "问题:", "答案:"] 缺[]）；
  [CATCH] R4_unknown_field | [answerer] 未知字段 prefix ["Bonus:"]；
  VERDICT: INVALID（期望 INVALID 且抓到 R4_unknown_field）fired=["R2_field_count", "R4_unknown_field"]
  case chaos-add-field: PASS
== case chaos-add-demo ==
  [CATCH] R5_demo_count | [answerer] demos 3 ≠ 期望 2（IR 1 + 执行追加 1）；
  VERDICT: INVALID（期望 INVALID 且抓到 R5_demo_count）fired=["R5_demo_count"]
  case chaos-add-demo: PASS
== case twice（同一 IR 两次往返，字节级比对） ==
  第 1 次响应 990 字节；第 2 次响应 990 字节；字节级相等 = true
  case twice: PASS
==== 汇总 ====
PASS 6/总 6
ALL CASES PASSED
```

**字节级比对独立复核**（shell 层，与 Rust 内存比对互证）：
```
$ cmp out/roundtrip_1.json out/roundtrip_2.json && echo "CMP: identical"
CMP: identical
$ sha256sum out/roundtrip_1.json out/roundtrip_2.json
5c33c6baf1706a0cb30c0daf1a4370a0c03fd141a1e35aabeaa809fb6221fb70  out/roundtrip_1.json
5c33c6baf1706a0cb30c0daf1a4370a0c03fd141a1e35aabeaa809fb6221fb70  out/roundtrip_2.json
```

**回传实物**（`out/resp_base.json`，989 字节；可见 DSPy 存档无字段名、只有 prefix/description；CoT 状态键为 `solver.predict`、含派生 `Reasoning:` 字段；执行产物 demo 已入 `answerer.demos`）：
```json
{"exec_outputs":[{"module":"answerer","outputs":{"answer":"A-EXEC-1"}},
 {"module":"solver","outputs":{"answer":"A-EXEC-2","reasoning":"R-EXEC-2"}}],
 "state":{"answerer":{"demos":[{"answer":"aA","context":"cA","question":"qA"},
   {"answer":"A-EXEC-1","context":"C1","question":"Q1"}],"lm":null,
   "signature":{"fields":[{"description":"相关背景资料","prefix":"背景:"},
   {"description":"用户的问题","prefix":"问题:"},{"description":"一句话答案","prefix":"答案:"}],
   "instructions":"根据上下文回答问题。"},"traces":[],"train":[]},
  "solver.predict":{... "fields":[...,{"description":"${reasoning}","prefix":"Reasoning:"},...] ...}},
 "task_id":"roundtrip-base"}
```

**漂移实物**（`out/resp_chaos-swap-fields.json` 中 answerer 的字段顺序已被 dspy 改写 API 实际改乱，校验器抓到的就是它）：
```
"fields":[{"description":"用户的问题","prefix":"问题:"},{"description":"相关背景资料","prefix":"背景:"},{"description":"一句话答案","prefix":"答案:"}]
```

**对照实验：DSPy 自身对同类漂移完全静默**（本会话复演，exit 0）：
```
$ ./.venv/bin/python - <<'EOF'   # 同一三字段签名
坑1 delete(不存在) 字段数: 3 == 原: 3 → 静默 no-op, 无异常
坑2 load_state(错位顺序) 后 prefix 顺序: ['问题:', '背景:', '答案:'] → DSPy 未报错，静默接受错位
EOF
```
即：dspy 的 `Signature.delete("不存在的字段")` 返回 3 字段无异常；`Predict.load_state` 喂入 `[问题:, 背景:, 答案:]` 错位顺序直接接受。**同样这两类漂移，Rust 校验器分别以 R2/R3 抓住**——这是"校验必须在 Rust"的直接证据。

**AF 根 Cargo.toml 未被我修改**：工作开始时记录 sha256=`1626bf6b...e79`（members 含 `poc/poc-cas-rust/casr`）；工作期间观察到并行任务两次改动它（先后出现/移除 poc-otel、poc-cas-rust 条目，结束时 sha256=`baf540b1...d64b`，members=`["crates/*"]`）；全程 `grep -c "verify-adapter\|dspy-adapter-verify"` = 0，我未对该文件做任何写操作，最终内容无指向本任务目录的条目。本工程 Cargo.toml 自带空 `[workspace]` 表，独立构建成功即是脱钩证明。

无常驻进程：Python 均为一次性子进程（`Command::output()` 同步等待），无 server/worker/db 需要清理。

## ④ 对方案的反馈/决策输入

**④-1 IR schema v1 定稿建议（字段清单）**

```
ProgramIr  { ir_version: u32, name: string, root: ModuleNode }
ModuleNode { id: string, kind: "module"|"Predict"|"ChainOfThought",
             signature?: SignatureIr,        // 仅 Predict/ChainOfThought
             demos?: [Demo],                 // 仅 Predict/ChainOfThought
             children?: [ModuleNode] }       // 仅 module
SignatureIr{ instructions: string, fields: [Field] }   // fields 顺序 = 注册顺序，即 prompt 顺序
Field      { name: string,        // 签名内唯一，Rust 校验；DSPy 存档不含它
             kind: "input"|"output",
             type: "str",         // v1 仅 str（可扩展位）
             desc: string,        // → dump 的 description，辅助对齐
             prefix: string }     // → dump 的 prefix，位置对齐主键；必须签名内唯一
Demo       { inputs: {name→value}, outputs: {name→value} }  // inputs+outputs 键并集 = 生效字段名
```
配套规则（均已实测）：
- **路径规则**：根容器 id 不进入路径；子节点路径 = 父路径`.`id；**状态键** = 路径，ChainOfThought 加后缀 `.predict`。
- **CoT 派生规则**：`ChainOfThought` 在**最后一个 output 字段之前**内建插入 `reasoning`（prefix `Reasoning:`、desc `${reasoning}`）。Rust 期望模型必须编码此规则（本次 `solver.predict` 实测 4 字段顺序：背景:/问题:/Reasoning:/答案:）。
- **prefix 约束**：非空且签名内唯一——这是"位置对齐"可检测性的前提；不建议依赖 dspy 的 `infer_prefix` 派生（其正则转换难复刻），IR 显式携带 prefix。
- **执行任务**：`{task_id, program, execute: [{module, inputs, lm_script(1 项/调用，键=字段名), append_result_as_demo}], chaos?}`。

**④-2 适配层边界：哪些校验必须在 Rust（9 规则即清单）**

| 必须在 Rust | 规则 | 为什么 Python/DSPy 做不了（本任务实证） |
|---|---|---|
| 字段名齐全/唯一 | R2+R6 | DSPy 存档只有 prefix/description，无字段名（resp_base.json 实物）；字段名唯一真相只在 Rust IR |
| 位置对齐（顺序漂移） | R3 | DSPy `load_state` 按位 zip 静默接受错位（对照实验坑2）；swap 后只有 prefix/description 逐位比对能抓 |
| 无未知字段 | R4 | dspy `append` 不设限，多塞字段静默生效 |
| demos 数量一致 | R5 | DSPy 不校验 demos 增删；且数量期望=IR demos+执行追加语义，语义只在 Rust |
| demos 键名=字段名 | R6 | demo 是字段名唯一能流回 Rust 的通道，是"存档无字段名"的补偿校验 |
| 执行输出=确定性预期 | R7 | lm_script 语义由任务下发，预期只有 Rust 能算 |
| 执行产物入 demo | R8 | `append_result_as_demo` 语义在 Rust |
| 模块集合恰等 | R1 | 防 Python 侧多构造/漏构造模块 |
| delete 必须存在等改写预检 | （改写器侧，见⑤-1） | dspy `delete` 缺字段静默 no-op（对照实验坑1） |

Python 薄适配层职责收敛为：构造（按 IR 字面重建）→ 执行（确定性回放）→ 漂移注入（仅测试用）→ 回传，**不做任何一致性判断**。Python 报错只允许发生在"构造不合法"（如未知 kind），一致性漂移一律由 Rust 判 INVALID。

**④-3 实现要点（踩坑即决策输入）**
1. dspy 3.4.1 `InputField(prefix=...)` **已弃用且无效**（运行时 DeprecationWarning 实证）；prefix 的合法通道是 `field.json_schema_extra["prefix"]`（与官方 `load_state` 恢复前缀同一机制，signature.py:542）。适配层照此实现。
2. DummyLM 回放脚本的键、forward 的 kwargs 都必须是**字段名**（不是 prefix）——用 prefix 会触发"Input contains fields not in signature"警告且解析失败（实测）。
3. 用 dspy `delete`+`insert` 实现 swap 的位置公式：a 原在 b 前时插到 `index(b)+1`，否则 `index(b)`。第一版写成 `index(b)` 一刀切导致 swap 变 no-op——**校验器正确地没有误报**（顺序未变即 VALID），修正后 R3 精确报出两个位置的错位。这同时验证了校验器不误报。
4. R2 与 R3 互补：删字段时剩余字段位置仍对齐，R3 不响，靠 R2 计数抓（fired 恰为 `[R2_field_count]`）；多塞字段时 R2+R4 双响。规则归因清晰，便于上游定位漂移类型。
5. "执行成功 ≠ 结构没漂移"：chaos 在执行**之后**注入，全部案例 R7（执行输出）仍 ok——说明只校验执行结果会漏掉结构漂移，**回传 state 必须全量结构校验**。
6. 事务性：dspy `Module.load_state` 先 deepcopy 试跑再应用（base_module.py:157-169）的语义值得 Rust 端照抄（PoC 已证，本任务未重复验证）。

## ⑤ 遗留问题

1. **Rust 侧改写器未实现**：本任务只验证了"校验边界"，7 个改写 API 的 Rust 复刻（含 delete-必须存在、字段名唯一性预检）留给正式实现；届时 ④-2 最后一行才有落点。
2. **type 仅验证 "str"**：int/float/list 等类型扩展未测；Demo/inputs 值当前按字符串反序列化，扩展类型需同步改 Rust 响应类型。
3. **嵌套深度**：容器树验证到"根+子节点+CoT 内建 predict"（状态键两层点路径）；更深层容器（grandchild）路径规则同构但未实测。
4. **反向重建未测**：闭环目前是"构造→执行→dump→校验"；"从回传 state 在 Python 侧 load_state 重建程序再 dump 比对"未做（PoC ③-1 已证 dspy 能力，适配层集成留待后续）。
5. **环境**：/tmp（tmpfs）86% 满，bash 每次退出报 `pwd: 写入错误: 超出磁盘配额`——不影响命令退出码（各命令 exit code 均独立验证），但环境侧需清理。
6. AF 根 Cargo.toml 期间被并行任务修改两次（详见③末），与我无关且最终无我方条目；如需审计以 sha256 记录为准。
7. 性能未测：单次往返秒级（含 python 启动+dspy import），高频 JIT 场景数据缺失。

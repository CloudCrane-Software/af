# RESULT-UPDATE — T8（波次1⑥）：CUE 预编译 CI 化 + CEL 内嵌（crates/af-graph 侧落地）

对应总纲裁决：①引用完整性检查从 CUE 移入装载器（错误码 E2001）、CUE 侧去掉 or() 组合；②版本信封=拒绝式（属 verify-load 装载器，af-graph 不涉及，见 ④-5）；③引入 cel-interpreter 做内嵌谓词求值最小封装。本次只动 `crates/af-graph/`，**未改根 Cargo.toml / workspace.dependencies**（正被另一任务重构）。

## ① 结论

**通过**。两件事落地，三门质量门全绿：

1. **E2001 引用完整性检查（错误码化）**：给 af-graph 校验器引入 `ErrorCode` 分类（`src/validate.rs:40`），`Violation` 增 `error_code` 字段（`src/validate.rs:91`），14 处违例全部赋稳定码；**所有"引用的目标不存在"类违例统一为 E2001（DANGLING_REFERENCE）**，并新增**工件名级**检查——`map.over` 的 "节点.工件" 前缀节点必须已声明（`src/validate.rs:236-266`），收口 `af-graph/RESULT.md` ⑤ 遗留"工件名级引用完整性未做"里**可判定的那一半**。
2. **CEL 内嵌谓词求值最小封装**：`cel-interpreter` **0.10.0**（crates.io 最新稳定）加进 af-graph 自己的 `Cargo.toml`；新增 `src/predicate.rs` 提供 `Predicate`（compile + eval/eval_with + 错误统一传递 `PredicateError`）。

**关键发现（安全相关）**：`cel-parser` 0.10.1 对"尾部运算符 / 空输入"等畸形串（`"1 +"`、`"count >"`、`""`）会触发 antlr4rust 错误恢复路径的**内部 panic**，而非返回 `ParseErrors`（实测见 ③-4）。谓词串可能来自配方/配置等不可信来源，内嵌求值器必须不崩溃——故 `compile`/`eval` 均以 `catch_unwind` 兜底，把这类 panic 归一成 `PredicateError` 向上传递（`src/predicate.rs:43-46`），不击穿调用方。

质量门（全真跑，命令与输出见 ③）：`cargo check -p af-graph` 绿；`cargo test -p af-graph` **29 passed / 0 failed**（validate 21 + predicate 8）；`cargo clippy --all-targets -p af-graph -- -D warnings` **零警告**。

## ② 步骤

1. 读背景：`poc/poc-cue/RESULT.md`（CEL/CUE 职责切分、④-6 运行时谓词走 CEL）、`poc/poc-cue/verify-load/REPORT.md`（E2001 定义、④-1 引用完整性移到装载器、④-2 CEL 内嵌、loader/src/error.rs 分类学）、`crates/af-graph/RESULT.md`（五规则校验器、⑤ 遗留"工件名级未做"）、`ci/run.sh` step4（cue vet/export+结构断言门）。
2. 查 crates.io 稀疏索引确认 `cel-interpreter` 最新稳定 = **0.10.0**；下载 `cel-interpreter-0.10.0.crate` 核对 API：`Program::compile/execute`、`Context::default/add_variable_from_value`、`Value`、`ResolveResult=Result<Value,ExecutionError>`、`ParseErrors`。
3. `crates/af-graph/Cargo.toml` 增 `cel-interpreter = "0.10"`（**只加本 crate**；serde/serde_json 已是 `{ workspace = true }`，保持不动，不碰根）。
4. `src/validate.rs`：新增 `ErrorCode` 枚举 + `code()/name()`（E2001–E2007，E2001–E2004 与 verify-load 分类学语义一致，E2005–E2007 为图模板域扩展）；`Violation` 增 `error_code` 字段，`Display` 带码；14 处 `out.push` 全部赋码；**新增 map.over 前缀节点 E2001 检查**。
5. `src/lib.rs`：`pub mod predicate;`，re-export `Predicate/PredicateError/ErrorCode`。
6. 新增 `src/predicate.rs`：CEL 最小封装 + `catch_unwind` 兜底。
7. 测试：`tests/validate.rs` +5 例（E2001 正/反/拒绝），`tests/predicate.rs` +8 例（CEL 正/反/错误传递）。
8. 修一处**预存** `useless_format`（max_iterations 消息行，`-D warnings` 要求；原 af-graph RESULT.md 记"clippy 未装"，故此前未暴露）。
9. 跑三门 + `cargo fmt -p af-graph`。

## ③ 证据

**依赖解析（crates.io 稀疏索引）**
```
$ curl -s https://index.crates.io/ce/l-/cel-interpreter | ...（取非 yanked 的最大版本）
all stable (non-yanked): ['0.1.1' ... '0.9.1', '0.10.0']   # 最新稳定 = 0.10.0
```
`Cargo.lock` 落：`cel-interpreter v0.10.0 source = "registry+https://github.com/rust-lang/crates.io-index"`。

**三门（全真跑，af-graph 作用域）**
```
$ cargo check -p af-graph
    Checking af-graph v0.1.0 (.../crates/af-graph)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s   # 绿

$ cargo test -p af-graph
     Running tests/predicate.rs  ->  running 8 tests;  test result: ok. 8 passed; 0 failed
     Running tests/validate.rs   ->  running 21 tests; test result: ok. 21 passed; 0 failed
   （lib / bin / doc-tests 各 0 tests, ok）                                  # 合计 29 passed / 0 failed

$ cargo clippy --all-targets -p af-graph -- -D warnings
    Checking af-graph v0.1.0 (.../crates/af-graph)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s   # 零警告
```
clippy 首跑报 1 处 `error: useless use of format!`（`src/validate.rs` max_iterations 行）——**预存**告警（非本次新增），改为 `.to_string()` 后复跑零警告。`cargo fmt -p af-graph -- --check` 复跑 `FMT-CLEAN`。

**新增 E2001 测试（tests/validate.rs，均 passed）**
- `legal_templates_have_no_e2001`（正例：合法模板无 E2001）
- `dangling_edge_ref_is_e2001`（illegal-3 边端点悬空 → E2001/DANGLING_REFERENCE）
- `map_over_dangling_node_is_e2001`（工件名级：over 前缀改幽灵节点 → E2001）
- `map_over_malformed_is_e2001`（over 缺 '.' → E2001 格式非法）
- `duplicate_node_id_is_e2003_not_e2001`（反例：重复 id 是 E2003，**不**是 E2001）

**CEL 测试（tests/predicate.rs，均 passed）**：正例 `compiles_and_evaluates_true` / `gate_like_predicate_over_event_data` / `eval_with_reuses_native_context`；反例 `evaluates_false` / `non_boolean_result_is_rejected` / `eval_error_undeclared_variable_is_passed_through`；错误传递 `compile_error_native_is_passed_through`（native ParseErrors 路径）+ `compile_panic_is_caught_and_passed_through`（panic 兜底路径）。

**③-4 cel-parser 0.10.1 panic 探针（临时测试，跑完即删）**——同一封装下对畸形串逐一 `catch_unwind(Program::compile)`：
```
PANIC : "1 +"  "count >"  ""  "   "  "'unterminated"  "1 =="  "true &&"     # 尾部运算符/空/未闭合串
Err   : "(1 + 2"  "foo("  "1 +* 2"  "1 2 3"  "@bad"  "{1: 2"  "[1, 2"  "?x"  # antlr4rust 正常报 Syntax error
```
结论：尾部缺操作数 / 空输入命中 antlr4rust 未实现的错误节点路径 → `unreachable!` panic。封装层 `catch_unwind` 归一为 `PredicateError::Compile`，测试 `compile_panic_is_caught_and_passed_through` 证明不击穿。

## ④ 反馈

1. **E2001 是跨域共享语义码，成立**：装载器里是 combo→规则 id / andon→评估函数 / required→工件字段（`verify-load/loader/src/error.rs` `DanglingReference`），图模板里同强度覆盖 节点/子图/工件(map.over)/安全点——同一"引用的目标必须存在"不变量，两域复用 E2001 便于内核统一日志/断言。
2. **E2001 纯度需守住**：重复 id 不是"目标不存在"，归 E2003（`DuplicateId`）；测试 `duplicate_node_id_is_e2003_not_e2001` 双向断言（是 E2003 且无任何 E2001）。af-graph 原有规则③把"重复 id"和"悬空引用"混在 `UndeclaredReference` 下，现已用 `error_code` 字段把它们分开（`Rule` 仍保留作 af-graph 内部分类，`ErrorCode` 作外部契约码，二者正交）。
3. **cel-parser 0.10.1 的上游 panic 是真缺陷，计划层应记录**：内嵌 CEL 求值器面对不可信谓词串必须 `catch_unwind` 兜底（本次已做）；建议后续评估上报上游 / 钉版本 / 或换 `cel-parser` 修复版。注意 `catch_unwind` 不捕获 `panic=abort`——本 workspace 默认 unwind（根 `Cargo.toml` 无 `[profile]` 覆盖），若将来改 abort 需重新评估此兜底。
4. **工件名级只做了一半（诚实交代）**：`map.over` 的**前缀节点存在性**已查（E2001）；但 `edge.output/input` 的**工件存在性/类型**需要一张"节点产出/消费工件表"（`af-graph/RESULT.md` ⑤ 遗留，模型未建 `Node` 的产出/消费声明），本次未做——留给需要时机械补上。
5. **版本信封=拒绝式不落在 af-graph**：那是 verify-load 装载器对 `schemaVersion` 整数大版本的门禁（未知版本 E0001、payload 解析前门禁）。af-graph 的版本是一等必填 `graph_version: String`（serde 管缺失，非拒绝式整数信封），本次按"最小可用"不引入信封层；E2001 与 CEL 是本次两个落点。
6. **CUE 侧去掉 or() 的收益在 verify-load REPORT ④-1 已量化**（千条 vet 88.9s→1.06s）：af-graph 侧不存在该放大（图模板是几十节点级，无 per-rule 析取），但 E2001 作为装载器兜底与 CUE per-rule closed schema 形成"形状双保险 + 语义最终防线"的分工，建议最终方案固化这条边界。

## ⑤ 遗留

- `edge.output/input` 工件级存在性/类型表未建（需给 `Node` 加"产出/消费工件"声明后才能查；`map.over` 前缀节点已查）。
- `cel-parser` 上游 panic 缺陷仅在封装层兜底、未根治；`catch_unwind` 不覆盖 `panic=abort`（当前 unwind，改配置需复评）。
- CEL 谓词尚未接进 af-graph 模型（如 gate 节点 `condition` 字段）——本次按任务"最小可用封装"定位为独立模块；CEL 表达式串的"参数闭合"静态校验（`Program::references()`）留待与 CUE 侧配合时补。
- `cel-interpreter` 拉入的依赖树（antlr4rust / chrono / regex / uuid 等）对 `deny.toml`（licenses/bans/sources）的合规性**未跑 `cargo deny`** 验证。
- 环境性插曲（非本次改动引入）：并发任务重构 workspace 期间，`crates/af-workorder/Cargo.toml` 一度带 git 冲突标记致 workspace manifest 加载失败，重试后恢复；本次所有 cargo 门均在恢复后真跑通过。

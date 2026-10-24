# RESULT-P1 —— af-bench 吸收 · 任务包 P1（纯函数层最小可绿）

> 身份：吸收施工员 P1（P2-F03），2026-10-24。工作树 `af-wt-phase2-execline`（分支
> `phase2-execline`），权威施工图 `计划/研究组/af-bench吸收方案-20261024.md` §5-P1。
> 施工形态：ADR-007 先红后绿——RED `1111c60`（测试 + `unimplemented!()`/占位常量桩，
> p1_suite 0/15、p1_comparison 2/9 红）→ GREEN `97b7da6`（原样搬入转绿）。
> af-bench 本体零改动（`git diff 44b7f17..HEAD -- crates/af-bench` 为空）；
> af-workorder/af-engine 零改动；未碰 P2-P6 内容；未写 计划/变更日志.md。

## 1. 搬了哪些函数（源 文件:行号 → 目标位置）

af-bench 源 = 本工作树 `crates/af-bench/src/`（与主仓三文件逐字节一致，本任务 `diff` 实测）。

| 搬入物 | af-bench 源 | af-execline 落位 | 备注 |
|---|---|---|---|
| `SwevTask`（含 `hints_text` serde default） | swev.rs:16-27 | `suite/swev.rs:21` | 体逐字节一致；`#[serde(default)]`/`#[allow(dead_code)]` 随迁 |
| `SwevTask::prompt` | swev.rs:29-39 | `suite/swev.rs:37` | 体逐字节一致（fix2 不注入 hints 语义） |
| `load_swev_tasks` | swev.rs:41-52 | `suite/swev.rs:45` | 体逐字节一致（含错误文案） |
| `diff_touches_tests` | swev.rs:119-139 | `suite/swev.rs:60` | 体逐字节一致（gate 记账，非判分） |
| `SWEV_SYSTEM_PROMPT` | main.rs:543 | `suite/swev.rs:84` | 逐字节（测量语义载荷不动） |
| `SWEV_INSTANCE_PROTOCOL` | main.rs:545-566 | `suite/swev.rs:90` | 逐字节 |
| `ARCHAEOLOGY_PATTERNS` | main.rs:568-577 | `suite/swev.rs:113` | 逐字节 |
| `TbTask` | tb.rs:12-15 | `suite/tb.rs:21` | 体一致（原本就无 derive，不新增） |
| `load_tb_tasks` | tb.rs:17-36 | `suite/tb.rs:26` | 体逐字节一致（含目录形状校验与文案） |
| `shell_quote` | tb.rs:66-68 | `suite/tb.rs:48` | 私有→pub（P2/P5 复用），体不动 |
| `split_rc` | tb.rs:70-92 | `suite/tb.rs:56` | 私有→pub，体不动 |
| 护栏常量 `SWEV_AGENT_CONTAINER_MEM/PIDS`、`TB_CONTAINER_MEM/PIDS` | tb.rs:115-120（校准注释 101-114） | `suite/tb.rs:93-97`（注释随迁 tb.rs:73-92） | 值与注释逐字（2026-10-21 anolis 事故固化） |
| `tail` | tb.rs:299-305 | `suite/tb.rs:100` | 私有→pub，体不动 |
| `bash_tool_decl` | tb.rs:307-324 | `suite/tb.rs:113` | 体逐字节一致（workdir 注入语义） |
| `Profile`/`profile()` | main.rs:156-161, 163-327 | `profile.rs:21,27` | 体逐字节一致（10 档案 key_cmd/端点字段/注释/未知档案文案逐字） |
| `resolve_key()` | main.rs:329-350 | `profile.rs:194` | 体逐字节一致（<16 字符拒绝等） |
| `status_label` | main.rs:579-588 | `suite/mod.rs:18` | 体逐字节一致（六态映射） |
| `next_attempt_index` | main.rs:590-610 | `suite/mod.rs:34` | 体逐字节一致（D-20 递增不覆写） |

未搬（属 P2-P6，本包不做）：`swev::setup/capture_diff/verify/sha256_hex/tb_ssh`（P2/P3）、
`tb::ssh/TbEnv` 生命周期（P2/P5）、`bench_order`/`run_swev`/`run_tb`/results 行装配（P4/P5）、
`run_injec`（本期不搬）。判分三件套契约（task_json/eval.log/agent.diff）接口未动。

## 2. 对照测试清单（同输入同输出的证明形态）

`tests/p1_suite.rs`（15 例，验收 + 随迁 + 黄金值）与 `tests/p1_comparison.rs`（9 例，对照）。

**活源对拍**（`#[path]` 把 af-bench tb.rs/swev.rs **原文件**编译进测试二进制作旧路基准，
新旧同输入逐例比对；工作树 af-bench 与主仓逐字节一致，故基准即主仓行为）：

| 测试 | 覆盖 | 断言形态 |
|---|---|---|
| `cmp_diff_touches_tests_battery` | `diff_touches_tests` | 15 例电池（b/ 前缀/四类配置/大小写/`latest` 子串边界/单 token/连续 b//多命中保序）新旧 `Vec` 全等 + 非空转防护 |
| `cmp_swev_task_load_and_prompt` | `load_swev_tasks`/`SwevTask::prompt` | 同 fixture 双读六字段逐个相等 + prompt 相等（含换行/引号/非 ASCII）；缺文件、坏 JSON 的 Err 文案逐字节相等 |
| `cmp_tb_load_tasks` | `load_tb_tasks` | 同 fixture 双读 task_id/prompt 相等；缺 Dockerfile/test.sh 的 Err 文案逐字节相等 |
| `cmp_bash_tool_decl` | `bash_tool_decl` | `/testbed`、`/app`、空 workdir 下新旧 `serde_json::Value` 全等 |
| `cmp_guardrail_constants` | 四护栏常量 | 新旧全等（af-bench 自带的 tb.rs:326-341 守卫测试也随 `#[path]` 编译在本二进制内运行通过） |

**源文本比对**（main.rs 依赖面大无法整体编译进本 crate；从 af-execline 侧常量/返回值
**重建源码字面量**（`{:?}` / 拼回 `r#"…"#` / 数组排版）→ 断言 main.rs 原文含它，
差一个字节即失配）：

| 测试 | 覆盖 |
|---|---|
| `cmp_profile_table_against_bench_source` | 10 档案的 key_cmd/base_url/model/key_ref 字面量 + 未知档案报错模板（`{other}` 占位原文） |
| `cmp_prompts_and_patterns_against_bench_source` | `SWEV_SYSTEM_PROMPT`（`{:?}` 重建）、`SWEV_INSTANCE_PROTOCOL`（raw 串重建）、`ARCHAEOLOGY_PATTERNS`（数组排版重建） |
| `cmp_status_labels_against_bench_source` | 六标签的 `"<label>".into()` 臂原文在册 |

**黄金值/随迁**（af-bench 侧为模块私有或深嵌 main.rs，无法外对拍；见 §3 偏差 3）：
`shell_quote`/`split_rc`/`tail` 黄金值（p1_suite 三例，从源码逐行推导并经实跑校准——
split_rc 中部标记行一例初版推导漏了保留的前导换行，实跑纠正后入测）；`next_attempt_index`
随迁原单测（main.rs:48-73 逐字，仅临时目录前缀防撞）；护栏常量随迁原单测
（tb.rs:326-341 逐字）；`status_label` 六态构造黄金值；`resolve_key` 用 printf 假命令
（过短拒绝/16 字符放行/非零退出，零真实凭证）；两段提示词黄金值。

## 3. 与方案的偏差（如实登记）

1. **af-harness 依赖自 P4 提前至 P1**（`Cargo.toml` [dependencies]）。方案 P1 写
   "依赖：无（仅 serde/serde_json）"，但 P1 目标内的 `profile()` 返回 af-harness
   `EndpointSpec`、`status_label` 收 `&af_harness::RunStatus`——"原样搬"要求签名不动，
   两条方案约束冲突。取舍：保语义（原样搬），破依赖口径。方向 af-execline←af-harness
   无环（af-harness 不依赖 af-execline，`cargo deny` 四类 ok）；P4 本就要加同一依赖，
   提前不造成返工。代价：`cargo test -p af-execline` 构建链变重（rusqlite bundled/
   ureq/biscuit-auth，R7 已被方案接受）。
2. **随迁测试数量：2 非 3**。方案 §1.3 记 "af-bench 现有 3 个单测"，实测
   `grep '#\[test\]'` 仅 2 个（main.rs:49、tb.rs:334）。两处均已随迁（p1_suite.rs），
   无第三个可迁。
3. **三个私有函数无法活源对拍**。`shell_quote`/`split_rc`/`tail` 在 af-bench 是模块
   私有，`#[path]` 模块外部不可见；`include!` 整文件进同模块又吃不下其 `//!` 文件头
   （rustc E0753，本任务 /tmp 探针实测）。改用黄金值断言（p1_suite.rs）+ 短小纯函数
   风险可控；如需活源对拍需 af-bench 侧提权（改本体，违反本包只读约束，不做）。
4. **`tb::ssh` 私有函数的对照缺口**：属 P2 HostBridge 范围，本包未搬未测（非偏差，
   列此存照——P2 落位时以 FakeBridge 断言命令构造，方案 §5-P2 已有草案）。
5. **第三方 future-incompat 提示**：af-harness→af-credential→biscuit-auth→
   biscuit-quote→proc-macro-error2 v2.0.1 触发 cargo "将被未来版本拒绝"提示
   （`cargo tree -i` 实测溯源）。属第三方包代码、非本包 lint 警告：clippy exit 0、
   af-execline 自身代码 0 警告；af-bench 既有依赖链同款，非本包新引入的第三方 crate。

## 4. 验证记录（本任务实跑，CARGO_TARGET_DIR=~/afbuild-phase2b，未占 af/target）

| 项 | 命令 | 结果 |
|---|---|---|
| 验收命令（任务书原文） | `cd /home/pan-ding/workspace/af-wt-phase2-execline && CARGO_TARGET_DIR=~/afbuild-phase2b cargo test -p af-execline` | **全绿**（exit 0）：lib 14 + integration lifecycle 8 + p1_suite 15 + p1_comparison 9 + doc 0 = **46 passed / 0 failed** |
| RED 基线 | 同上（RED 提交 1111c60 时点） | p1_suite 0/15、p1_comparison 2/9（过的两项：af-bench 自带守卫测试与纯源文本检查）；既有 lib 14、lifecycle 8 不受影响 |
| clippy | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo clippy -p af-execline --all-targets` | af-execline 自身 **0 警告**（exit 0）；仅第三方 proc-macro-error2 future-incompat 提示（§3-5） |
| fmt | `cargo fmt --check --all` | PASS（零 diff） |
| cargo deny | `cargo deny --offline check` | advisories / bans / licenses / sources 四类 ok |
| 迁移源一致性 | `diff` 工作树 vs 主仓 af-bench 三文件 | 逐字节一致（IDENTICAL） |
| af-bench 零改动 | `git diff 44b7f17..HEAD --stat -- crates/af-bench crates/af-workorder crates/af-engine` | 空 |
| git 序列 | `git log --oneline` | 1111c60（RED）→ 97b7da6（GREEN）→ 本文件提交，红→绿成立 |
| 资源纪律 | 开工 `df -h / /tmp` | / 42G 可用（>15G 阈值）；构建只落 ~/afbuild-phase2b；零容器、零 LLM 网络调用、零凭证消耗（resolve_key 测试只用 printf/echo） |

## 5. 交接（P2 起点提示，未越界施工）

- `suite::tb::{shell_quote, split_rc, tail}` 已 pub，P2 HostBridge 真实现（tb.rs:42-64 与
  swev.rs:268-293 收敛）可直接复用；`TbEnv`/`ssh` 待 P2 搬。
- `suite::swev` 常量供 P4 `SwevUnitSpec` 组装；`profile::Profile` 待 P4 接 `AgentRunner`
  key 紧邻现取（main.rs:768-773）。
- 对照测试的 `#[path]` 引用是**活源**：af-bench 若在 P6 前被改动，对照测试会随之以新
  行为为基准（或失配报警）——这正是对拍想要的性质；P6 薄壳化时按方案处理。

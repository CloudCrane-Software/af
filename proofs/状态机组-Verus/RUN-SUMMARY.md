# D 组 · 状态机组（D-01～D-09）Verus 证明 · 运行摘要

- 日期：2026-10-25
- 执行者：Verus证明员-状态机组（wave1）
- 正本：`计划/研究组/形式化/theorems-v1.md` §D 组（9 条，只读，未改动）
- 锚定源码（只读引用，未改动）：
  - `af-wt-phase2-dsl/crates/af-workorder/src/state.rs`（127 行全文核对）
  - `af-wt-phase2-dsl/crates/af-workorder/src/order.rs`（:49-63 类型、:196-257 new、:401-458 门控+迁移）
  - `af-wt-phase2-dsl/crates/af-workorder/src/error.rs`（:10-52 GateError/Missing）
  - 分支核实：`git branch --show-current` → `phase2-g2b`（会话内实跑）
- 交付目录：`计划/研究组/形式化/proofs-wave1/状态机组-Verus/`
  - `state_group_mirror.rs` —— 镜像证明文件（38,322 B）
  - `verus-state-group.log` —— 交付目录内干净复跑的完整内核输出
  - `RUN-SUMMARY.md` —— 本文件

---

## 1 · 内核裁决（唯一裁判，实跑）

```
命令：/dev/shm/dstate/verus-x86-linux/verus --crate-type bin state_group_mirror.rs
退出码：0
裁决行：verification results:: 29 verified, 0 errors
```

同命令在交付目录内干净复跑一次（工作目录 = 本交付目录），再次 `29 verified, 0 errors`
（`verus-state-group.log`）。工具：Verus **0.2026.10.18.7d82994**（`verus --version` 实跑输出），
与 FV-0 PoC（`FV0-工具链PoC-20261024.md`）同版本。

## 2 · 纯度判定（任务令第一问）

| 函数 | 纯度 | 依据（行号锚定） |
|---|---|---|
| `state.rs` 全部五函数（from_state/to_state/allowed_transitions/can_apply/is_terminal） | 纯 | :36-79，enum+match，无 IO 面 |
| `order.rs::release_missing` | 纯 | :405-418，只读 self.line/self.budget_voucher |
| `order.rs::close_missing` | 纯 | :421-427，只读 self.artifacts |
| `order.rs::transition` | 纯 | :432-458，内存状态变更，无时钟/网络/打印 |

**结论：本组 9 条全部为纯判定，不存在「含 IO 只能证判定半边」的条目。**
真实边界是另外三条（见 §5），与 IO 无关。

## 3 · 陈述↔代码冲突清单

**空（零冲突）。** 九条陈述的每一处锚定行均逐行核对成立：
D-01→state.rs:58-67✓；D-02→order.rs:431-440✓+state.rs:69-75✓；D-03→state.rs:65/:77-79✓；
D-04→state.rs:31-33/:70-73✓；D-05→state.rs:36-44（:42 `unreachable!` 审计亲核属实）/:46-54✓；
D-06→state.rs:3-5✓+order.rs:431/:441-451/:403-404/:420-427✓+error.rs:2✓；
D-07→order.rs:452-457/:431/:435/:447（拒绝路径先于 :452）✓；
D-08→state.rs:59-67（逐行无自环）✓；D-09→order.rs:403-418/:404/:420-427/:446-451✓。

## 4 · 条目→证明工件映射（9/9 全 verified ≥ 验收线 5）

| 条目 | 机器化载体（均含于 29 verified） | 关键内核事实 |
|---|---|---|
| D-01 跃迁表恰为声明生命周期 | `proof fn d01_table_is_declared_lifecycle` | 六态 push 链表逐字恒等 + `can_apply(s,t) ⇔ declared_edge(s,t)` 全域（30 例由数据域有限性覆盖） |
| D-02 无非法跃迁 | `fn transition_exec` ensures 第二臂 + `scenario_rejections_and_gates` | ∀输入：`!can_apply ⇒ Err(IllegalTransition{from,to,allowed@==allowed_spec(s)}) ∧ 状态/履历不动`（内核对任意输入验证） |
| D-03 终态吸收 | `proof fn d03_terminal_absorb` + `scenario_terminal_absorb` | Closed/Cancelled 表空、`is_terminal ⇔ {Closed,Cancelled}`、终态任意跃迁可 apply 为 false |
| D-04 Cancel 窗口 | `proof fn d04_cancel_window` + `scenario_terminal_absorb` | `can_apply(s,Cancel) ⇔ s∈{Created,Released,InProgress}`（6 态全域；完成即不可取消） |
| D-05 from_state 全函数性（Cancel 除外） | `proof fn d05_from_state_totality` + `scenario_exec_slices` | ∀t≠Cancel：`can_apply(from_state(t),t)` 且 (from,to)∈合法边且 from≠to；Cancel 分支以 requires 前置排除（边界 B-3） |
| D-06 状态机/门控双轨分离 | `proof fn d06_dual_track_separation` + `transition_exec` ensures + `scenario_rejections_and_gates` | Release 门控 ⇔ (绑定∧凭证存在∧signed)；Close 门控 ⇔ ∃(is_final∧qualified)；清单含产线缺失必居首、长 2 时次位必为凭证侧；IllegalTransition/PreconditionsNotMet 两臂互斥（错误不混淆） |
| D-07 迁移履历完备性 | `transition_exec` ensures（条件保持 `hist_inv`）+ `proof fn d07_history_initial` + `scenario_lifecycle_and_history` | Ok ⇒ 恰追加一条 `{from:s0,to:迁移后}`；Err ⇒ 履历不变；链相接保持；初态（new：Created+空表）满足不变量；4 步场景逐位核对 |
| D-08① 状态机幂等（无自环） | `proof fn d08_no_self_loop` + `scenario_lifecycle_and_history`（重复施加 Complete） | ∀s,t：合法边 ⇒ `to_state(t)≠s`；推论：5 跃迁的 `can_apply(to_state(t),t)` 全 false；执行面演示重复施加必拒且状态/履历不动 |
| D-09 release/close 门控确定性 | `proof fn d09_gate_determinism` + `d06`④ + `scenario_rejections_and_gates`/`scenario_exec_slices` | 8 组合清单逐一钉死；凭证缺席时签名位/凭证号不影响清单；close 只查 ∃(is_final∧qualified)；门控失败 ⇒ `PreconditionsNotMet` 且状态/履历不动 |

工具适配说明：theorems-v1 对 D-09 的工具字段写 Kani（组合断言）；本战役按任务令以 Verus 承载，
用 spec 函数 8 组合逐情形钉死 + 全域 forall 达成同一穷举语义（u64/int 适配原因见 §5-B1'）。

## 5 · 边界登记（如实，三条 + 一项工具注）

- **B-1（镜像边界，全组适用）**：本证明是 state.rs/order.rs 判定核心的**镜像**证明，非 crate
  本体编译（Verus 自带 rustc fork 不含 serde/chrono/thiserror/String·format! 依赖树——FV-0
  §1.2 同因）。镜像逐函数锚定源码行号；抽象仅三处且语义保距：
  voucher_id:String→u64（①不透明标识，清单变体/顺序与其字面值无关）、artifacts→ArtM{is_final,
  qualified}（close 门控只读这两字段，order.rs:422）、line→bool（release 门控只读是否绑定，
  order.rs:407）。建议 FV-1 补「镜像↔实现」对账单测（FV-0 报告同款建议）。
- **B-2（D-08②不在本组）**：执行单元终态幂等属 harness 执行单元语义（run.rs），不在本战役锚定
  文件内；theorems-v1 该条工具字段已自注「②的实现级定理待 harness run.rs 进入验证范围（本战役
  未读）」。本组只证 D-08①，与陈述「两层不合并、不可互相推导」的要求一致。
- **B-3（D-05 unreachable 分支）**：`from_state(Cancel)` 在源码是 `unreachable!`（state.rs:42），
  不证 panic 分支——按 D-05 工具注记以 `requires t != Cancel` 前置排除；结构性债务照录（「若未来
  新增多起点跃迁沿用 from_state 会 panic」），补偿机制 = can_apply 对 Cancel 的三起点特判
  （state.rs:71-73，已由 D-04 证明）。
- **B-1'（工具微观适配，非陈述冲突）**：本版 Verus 的 `int` 类型仅限 ghost 语境、`is` 算符仅收
  非限定变体名、`pub spec fn` 须标 `open`、后置条件对 &mut 参数须 `old()/final()` 消歧——镜像
  的 voucher_id 抽象因此取 u64。全部为载体语法适配，不触及定理内容。

## 6 · 环境与可复现

- 安装路径（tmpfs，磁盘零新增占用；FV-0 配方）：`/dev/shm/dstate/verus-x86-linux/`
- 下载：GitHub release `release/0.2026.10.18.7d82994`，资产 486,844,228 B（与 FV-0 报告记录逐字节
  一致）。注意：FV-0 报告 §5 的 URL 缺 `release/` tag 前缀（按该 URL 实跑得 404 "Not Found"），
  本次以 GitHub API 实查修正；代理 http://100.64.0.8:7890 本窗口 TLS 不通（exit 35/502），改直连
  + aria2c 16 连接（串行 curl 仅 ~35KB/s 不可行）。
- 复现：
  ```
  cd 计划/研究组/形式化/proofs-wave1/状态机组-Verus
  /dev/shm/dstate/verus-x86-linux/verus --crate-type bin state_group_mirror.rs
  # → verification results:: 29 verified, 0 errors（exit 0）
  ```
- 迭代日志：19 轮（/dev/shm/dstate/verus-run1..19.log，tmpfs 重启即失；终版与交付日志同文）。
  主要迭代点均为工具语法/求解器触发词层面（is 限定、open 标注、int ghost-only、nat/int 索引、
  场景拆分控 rlimit、∃ witness 提示），无一轮涉及改定理陈述或改锚定代码。
- 沙盒纪律：迭代沙盒 `~/.cache/dstate-proof/`（源文件初稿）；af-wt-phase2-dsl 工作树零改动
  （本会话对该仓只执行过只读命令：ls/grep/git branch/git status）。

## 7 · 验收对账

- 任务令验收线：≥5 条 verified 或如实边界归因。
- 实绩：**9/9 条全部有内核 verified 的机器化载体**（§4），29 个验证目标 0 错误；
  边界（§5）如实登记，均为登记性质而非不可证失败——无一条目落入「只能证半边」。
- 冲突清单：空（§3）。正本与源码本会话零改动。

## 8 · 附记：并行方分支切换（如实）

会话开始时（10-24 22:0x）`af-wt-phase2-dsl` 在 `phase2-g2b`（`git branch --show-current`
实跑）；10-25 09:04 收尾复核时该工作树已被并行方切到 `phase2-g3-l4`（本会话对该仓仅有
只读命令，未执行任何 checkout/写操作）。已按当前分支重核全部锚定内容：state.rs
（126 行 vs g2b 127 行，差 1 为 EOF 尾行；:1-79 判定面、:82、:84-126 测试逐字一致）
与 order.rs :430-458 transition 锚定段逐字一致——**镜像锚定与证明结论不受影响**。
本组证明的引用基线以任务令指定分支 `phase2-g2b` 的读取为准（读历时内容如上 §3）。

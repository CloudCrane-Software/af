# PoC：风险#5 跨库无原子事务（订单/WIP/Run/lease 分库）——prepared/committed 补偿

> 分支 `feature/risk-cross-db-atomicity`，基线 = 开工时 `origin/product/main`
> （68be206）。执行机 anolis-gpu-01。本目录自足留证：改前/改后故障注入读数、
> 门读数、改动面与如实登记的边界。

## 一、现状读准（改前，已核实）

四类持久面与写入路径（代码位为基线版）：

| 面 | 落库 | 写入路径 | 既有原子性 |
|---|---|---|---|
| 订单 | `<state>/orders/<id>/ledger.jsonl`（af-execline `OrderLedger`，fsync 追加）+ 受理账 `admissions.jsonl`（`admit.rs:974 append_receipt`，fsync） | submit 全生命周期驱动 `driver.rs:543 drive`；受理 `admit.rs:435 admit` | 单库幂等键 `{stable}/{op}`、`{stable}/admit/{seq}/{digest}`；MES 事件有 `emit_mes_if_missing` 补发对账 |
| WIP | WorkOrder 快照随账本行；rework 工单账本（`adjudicate.rs spawn_rework_order`）；源单 cancel 行 | 裁决环 Revise/Reject 分支 | **无**（重复物化原为硬拒绝，重复 abort 报 MissingEdge） |
| Run | `<state>/orders/<id>/runs/` attempt 目录 + `mes-events.jsonl`；af-harness 三 sqlite（harness/effects/trace） | execline 驱动；harness run loop | Run 面已有 IntentKey 重放 + epoch fencing（af-effect-gateway declare→executing→executed），**本批未动** |
| lease | af-station sqlite `task` 表（claim/lease_until/complete，守卫 UPDATE） | 裁决环 `Station::enqueue/claim/complete` | 单库原子；跨库无协调 |

**跨库写链**（本风险的实体）：`adjudicate.rs:405 run_steward_loop` 一次逻辑事务
横跨 裁决文书（文件）→ af-station（lease 库）→ `admit`（订单受理账，经引擎
子进程）→ rework/cancel（WIP 面）→ `Station::complete`（lease 库）。改前唯一
的失败语义是模块文档明写的「认领后不自动回滚，失败面留给运营者，租约到期
惰性回收」——**提交点（admit 落账）之后的崩溃没有任何检出/补偿面**。

（旁注：af-release-gate 的 CONWIP sqlite 是孤立 WIP 面，未接进订单链，无跨库
写路径；af-meter 有单库 outbox。全库无跨库事务/共享连接。）

## 二、改前读数：提交点后崩溃窗的双写实证

场景：Approve 裁决首走镜像（生成→落盘→入队→认领(seq=1 签)→admit 落账）
→ **注入停机**（不调 `station.complete`）→ ManualClock 推进租约到期 →
运维重试 = 再跑 `run_steward_loop`（认领按 rowid 先拿到回炉的旧任务）。

读数（`before/double-write.txt`，BASE 码真跑）：

```text
[before] 首走 admit 已提交: seq=1 key=sid-…/admit/1/bc9f5b…
[before] 崩溃后受理账行数 = 1（任务停在 claimed，等待租约回收）
[before] 重试后受理账行数 = 2
[before]   receipt: seq=1 …/admit/1/bc9f5b…   ← 同一裁决单 adj-wo-poc-before-1
[before]   receipt: seq=2 …/admit/2/9bc231…   ← 二次受理（双写）
[before] 队列: queued=1 claimed=0 done=1      ← 重试自产的新裁决单 adj-2 成队列残留
[before] 残留检出面（意图账/journal）存在 = false
```

改前判读：**同一裁决单被受理两次（seq 1/2 均在账）**，且无任何面能检出
「这笔事务进行到哪了」。（Reject 分支更糟：源单已 Cancelled，重试 abort 报
MissingEdge，任务永远无法 complete。）

## 三、改动面

| 文件 | 行数 | 逐件说明 |
|---|---|---|
| `crates/af-order/src/adjudicate.rs` | +726/−26 | ① 意图账 `AdjudicationJournal`（`prepared`/`committed`/`completed` 三相 append-only JSONL，fsync 追加，落在既有 adjudications 目录——不另造库）；② `run_steward_loop` 接线：提交点前落 `PreparedIntent`（含回执幂等键），分支结算后落 `CommittedFact`，complete 后落 completed；③ **重放幂等臂** `committed_receipt`+`steward_replay`：认领到的裁决单已过提交点则不重签不重受理，按意图账收敛剩余面返回原回执（replayed=true）；④ 分支幂等结算 `settle_decision_branches`（`spawn_rework_order` 同号同源跳过/异源拒绝；`abort_source` 已 Cancelled 即收敛）；⑤ `reconcile_adjudications` 无人值守扫描：检出 PreparedResidue / 分支幂等结算 / committed 线补缺 / completed 线收口，报告 `ReconcileStatus`；⑥ 单测 5 项（journal 相位/坏行 fail-loud/rework 幂等+异源拒/abort 幂等/编号不受 journal 污染） |
| `crates/af-order/src/lib.rs` | +5/−3 | re-export 新公开面（journal 类型与 reconcile） |
| `crates/af-order/tests/adjudicate_fault_injection.rs` | 新文件 769 行 | 故障注入验收 6 项（见下） |

不动：既有测试断言、golden 快照、`ci/clippy-baseline.txt`（md5 f2aa942c…，
`git status ci/` 干净）、crate 集合与根 Cargo.toml members、af-station/af-execline
等其余 crate。

## 四、故障注入验收（改后读数：`after/fault-injection.txt`，6/6 绿）

注入方式：**零生产注入钩子**——测试镜像首走的真实 pub 调用序列并在指定
相位停机（`CrashAt::{PreparedWritten, Admitted, CommittedWritten, TaskCompleted}`），
时钟用 af-station `ManualClock` 推进租约到期（不睡眠），引擎走真子进程、
队列走真 af-station。所有「崩溃」两侧的写入均为 fsync 落盘后才推进，调用间
停机 ≡ 进程级 SIGKILL 的持久面后果。

| 注入点 | 测试 | 改前后果 | 改后读数（断言） |
|---|---|---|---|
| W2：committed 线后、complete 前 | `fault_w2_…` | **受理账 2 笔（双写）**、任务卡 claimed | 重放臂：受理账恒 1 笔、原回执 replayed=true、seq 不变、任务 done、journal completed 收口；重试自产的新裁决单入队（新轮次，by-design） |
| W1：admit 落账后、分支/committed 前 | `fault_w1_…` | 无检出面；rework 可能永不物化 | reconcile 检出（LeaseHeld 登记）+ **分支幂等结算**（rework 落账恰 1 张）+ committed 线补缺；到期后重放臂收敛，rework 不双物化、账仍 1 笔 |
| W0：prepared 后、admit 前 | `fault_w0_…` | 无检出面 | reconcile 报 `PreparedResidue`（无回执不可能双写，留检）；到期后认领按首走真签真受理，恰 1 笔 |
| W3：complete 后、completed 线前 | `fault_w3_…` | 无检出面（任务 done 但无人知晓事务已收口） | reconcile 补 completed 线（ClosedByReconcile）；重复扫描 Converged（幂等 no-op） |
| Reject/abort 臂跨崩溃窗 | `fault_reject_…` | 重试 abort 撞 MissingEdge，任务永卡 | 已 Cancelled 即幂等收敛；cancel 行恒 1 笔；重放按 committed 线复用 aborted=true |
| 编号面 | `journal_files_…` | — | `.journal.jsonl` 不进 `next_adjudication_id` 的 `*.json` 计数（单号不受污染） |

复算口径：每项断言都从**盘上**重载（`OrderLedger::load` / `load_receipts` /
`AdjudicationJournal::open` / `station.get`）后核对，不信任内存视图——即
崩溃后另起进程重开的读数。

## 五、门读数（`gates.txt`，终版码）

| 门 | 命令 | exit | 读数 |
|---|---|---|---|
| 1 | `cargo test --workspace` | 0 | **1520 passed / 0 failed**（153 个测试二进制全 ok） |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 零告警 |
| 3 | `cargo clippy --workspace --all-targets --message-format=json` → `python3 -I ci/clippy_gate.py <jsonl>` | 0 | error 级消息 **0 条**；CLIPPY GATE: PASS（现有 0 警告，基线 0 签名，未动基线） |

复算环境注记：本机 `/data` 分区被多 label 共享打满过一次（linker bus error），
清掉本 label 的 incremental 缓存并以 `CARGO_INCREMENTAL=0` 复跑后三门全绿；
`af-engine` 信任锚（`af-engine.sha256`）在并发 cargo 调用下会被交叉重写——
**门/读数必须串行跑**（本目录读数均为串行终跑）。

## 六、如实登记的边界（做不到的 / 需人类裁决的）

1. **覆盖面**：补偿只落在 steward 闭环（订单受理链 × lease 库 × WIP 面的
   唯一在役跨库写链）。Run 面（af-harness 三 sqlite + MES）已有 IntentKey
   重放 + epoch fencing + outbox 对账（effect-gateway/meter），本批不动；
   af-release-gate（CONWIP）未接进订单链，无跨库写入点，无事可补偿。
2. **reconcile 不代认领**：af-station `claim` 无任务定向（按角色取最老
   queued），代领可能抢到未裁决任务且无释放面（complete 需真受理在账）；
   故 queued+已提交的任务收敛路径 = 下一次 steward 认领触发重放臂
   （`ReconcileStatus::AwaitingClaim` 文档同款理由）。若要无人值守秒级
   收敛，需 af-station 增加定向认领/释放面——**加机制，超出本批**。
3. **注入形态**：进程级 SIGKILL/断电注入未做——本仓库所有持久写入均
   fsync 后才推进，调用间停机与其持久面后果等价；af-harness 的
   `Breakpoint` 式生产内注钩刻意未加（保持零生产面改动）。
4. **多 steward 并发**：两 steward 同时认领同一任务由 af-station 守卫
   UPDATE 原子性接住（af-station 既有并发测试覆盖），本批未加并发注入。
5. **跨 state 根/跨机**：意图账与受理账同目录耦合（同 `<state>` 根），
   跨根搬移订单目录的恢复面沿既有 `instance.id` 家族的运维语义，不在
   本批延伸。

## 七、复现

```bash
export PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=8 LANG=C.UTF-8
cargo test -p af-order --test adjudicate_fault_injection -- --nocapture   # 改后读数
cargo test --workspace                                                   # 门1
cargo clippy --workspace --all-targets -- -D warnings                    # 门2
cargo clippy --workspace --all-targets --message-format=json 2>/dev/null \
  > /tmp/clippy.jsonl && python3 -I ci/clippy_gate.py /tmp/clippy.jsonl  # 门3
```

改前读数复现：BASE（68be206）上以 `before/harness.rs`（本目录留档，临时
测试文件形态）`cargo test -p af-order --test poc_crossdb_before -- --nocapture`。

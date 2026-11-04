# poc-af-trace-fsync — 风险#4：write_critical 的 Ok 只代表入队不代表 fsync

基线：开工时 `origin/product/main`（68be206）。分支 `feature/risk-af-trace-fsync`。
证据目录：`poc/poc-af-trace-fsync/{before,after}/`。

---

## 一、现状读准（逐层结论，改前已核实）

### 1. `write_critical` 返回 `Ok(seq)` 的那一刻，事件到底处在哪个状态

| 层 | 状态 | 依据 |
|---|---|---|
| L1 内存队列 | **已在**：`try_send` 进 `sync_channel(critical_capacity)` 有界缓冲 | `trace.rs` `write_critical`：`tx.try_send(ev)` → `Ok(seq)` |
| L2 append（write(2) 到文件） | 未发生：落盘线程异步 `recv` 后才写 | 消费者在 `spawn_consumer` 里拿镜像锁→`append_line` |
| L3 flush | 未发生（与 L2 同处） | 同上 |
| L4 fsync 到介质 | 未发生 | 同上 |

**实证（改前探针，commit 47e9f9f 之前的产品代码 + 仅新增测试）**：
`before/probe-before.txt`——hold 住消费者时连写 3 条全部 `Ok`，此刻 `critical.jsonl` 行数 = **0**；
释放后 `shutdown()` 返回 `Ok`，文件才有 3 行。**Ok = 仅 L1，盘上没有任何该事件的痕迹。**

**持久确认点实际在哪**：`shutdown()` 返回 `Ok(ShutdownReport)` 时——它 join critical
消费线程（通道清空 ⇒ 每条已 Ok 事件都完成了 append + fsync + 镜像），再对账
`gap_counter == gap_reported == Σ(文件内 gap.count)` 并写 `trace.db`。

**改前的 fsync 强度（strace 实证，`before/strace-fsync-shape.txt`）**：
`critical.jsonl` 上全部是 **fdatasync**（`File::sync_data`，9792 次，flood 测试），零 fsync。
fdatasync 不保证全部 inode 元数据；且**文件首次创建的目录项没有任何 fsync**——掉电窗口内
新建的 `critical.jsonl` 可能连文件名都不在目录里。这就是「落盘确认」声称与实际强度之间的差距。

### 2. 落盘失败 / 队列满 / gap_counter 语义（任务书要求特别核的疑点）

任务书疑点：文档说 critical 是 fail-closed，而「队列满或写盘失败都丢弃并 gap_counter += 1」
听起来是 fail-open。**核实结论：critical 轨不存在 fail-open 路径**，该引文是
`lib.rs` 里 **normal 通道**的语义（两段 doc 相邻，引述时混读）。critical 的实际行为：

- **队列满**：`Err(CriticalFull(event))`，事件原样退还，不计 gap（`before` 既有测试
  `critical_full_returns_err_and_event_is_preserved` + 本次新增
  `critical_full_does_not_touch_gap_or_durable`）。
- **落盘失败**：消费者 `fail_persist` 记下第一条错误、事件进 `unpersisted`；此后
  `write_critical` 一律 `Err(PersistFailed{detail, events:[新事件]})` 直接退还新事件；
  `shutdown` 返回 `Err(PersistFailed{events: 全部未落盘事件})`。**gap_counter 全程不动**，
  已成功落盘的内容不回滚（`critical_persist_failure_is_fail_closed` +
  新增 `critical_persist_failure_freezes_durable_and_keeps_gap_zero`）。
- normal 通道的丢弃+计数+`Σ(gap span.count) == gap_counter` 不变量不变。

### 3. `poll_anchor` 能证明什么、不能证明什么

- 能：锚开启时 `anchor_seen()` 是「已过 append+fsync 的 critical 条数」的运行时下界
  （`submit_critical_for_anchor` 只在 `append_line` 成功后调用）。
- 不能：默认锚关闭（`endpoint=None`）时它恒 0；不能定位具体 seq；不能证明 fsync 强度；
  `poll_anchor` 本身只触发时间窗封口判定，不触发落盘。
- **结论：poll_anchor 不是持久确认的替代品，本修法不依赖它。**

---

## 二、修法选择与落地（优先做法，未走登记差距的退路）

选了任务书的**优先做法**：让 shutdown 确认点的强度真达到该路径声称的「落盘确认」，
同时把 `Ok` 的含义与实际强度在文档上对齐。`Ok = 已入队` 的移交语义**保持不变**
（既有测试 `critical_full_returns_err_and_event_is_preserved` 等钉死了「容量内 try_send
立即 Ok、不等待落盘」；改成同步等 fsync 属于 API 语义变更，超出本任务授权，见边界①）。

改动（`crates/af-trace/src/`，4 文件 +299/−13）：

1. **`trace.rs append_line`**：critical 分支 `sync_data`(fdatasync) → **`sync_all`(fsync)**，
   数据与 inode 元数据全部落盘。
2. **`trace.rs sync_dir`（新增函数）+ open 路径接入**：`open_log`×2（含 `trace.db` 建库）
   之后对目录本身 fsync 一次，把新建文件的目录项钉到介质；失败即 `TraceError::Open`
   启动停止（fail-closed）。
3. **可检读数（新增 pub API）**：`Trace::critical_durable()`（本次进程 critical 已完成
   append+fsync 的条数，含 gap span 行；只增，失败后冻结）、`Trace::critical_durable_ms()`
   （最近一次落盘成功的注入钟时刻，0=尚未落盘）。不变量：
   `durable ≤ 本次 Ok 条数`，干净 shutdown 后 `== ShutdownReport::critical_persisted`。
4. **文档对齐四处**：`write_critical` doc 明示「**Ok = 已入队，不代表已写盘/已 fsync**，
   持久确认点是 shutdown Ok」；`lib.rs` 通道头注同步；`spawn_consumer`/`anchor.rs`
   的「sync_data」措辞改「sync_all」；`store.rs CRITICAL_DURABILITY="sync"` 值不动
   （fsync ⊃ fdatasync，档位声明仍真，旧库可重开），注释说明实际强度。
5. **`RESULT.md:12`「`Ok(seq)` 即保证落盘」是历史 PoC 文档的不实陈述**——历史记录不改写，
   以本 README 与现行 doc 为准。

---

## 三、测试（新增 5 个，既有断言一字未动）

| 测试 | 覆盖 |
|---|---|
| `poc_probe_ok_means_enqueued_not_persisted` | Ok 真实含义（改前/改后都成立的不变量） |
| `ok_means_enqueued_durable_reads_lag_until_shutdown` | 落地后语义：hold 期 durable==0/时刻==0；排空后 durable==3、时刻走注入钟；shutdown 后 durable==critical_persisted |
| `critical_persist_failure_freezes_durable_and_keeps_gap_zero` | 落盘失败：durable 冻结、事件从 PersistFailed 退还、gap 恒 0、文件只留已落盘首条 |
| `critical_full_does_not_touch_gap_or_durable` | 队列满：CriticalFull 退还、gap==0、durable≤Ok 数、被拒事件从未落盘 |
| `gap_counter_only_increases_across_normal_failures` | 缺口只增未删：溢出+写盘失败混合下采样单调不减，四方对账（counter/reported/文件Σ/落盘+缺口=发送数） |

---

## 四、改前改后读数

| 读数 | 改前 | 改后 |
|---|---|---|
| `critical.jsonl` 的 sync syscall | **fdatasync ×9792**，fsync ×0 | **fsync ×10013**，fdatasync ×0 |
| 目录本身 fsync | 2 次（均为 SQLite 自身） | 4 次（SQLite 2 + af-trace `sync_dir` 2，对应两次 open） |
| Ok 后文件行数（hold 住消费者） | 0（`before/probe-before.txt`） | 0（`after/probe-after.txt`） |
| shutdown Ok 后 | 3 行 == critical_persisted | 3 行 == critical_persisted == durable |
| durable 读数 | 不存在该 API | `critical_durable()`/`critical_durable_ms()` 可检 |

strace 命令（before/after 各一次）：
`strace -f -yy -e trace=fsync,fdatasync,sync_file_range,syncfs -o <out> cargo test -p af-trace --test flood -- --nocapture`
（`-yy` 把 fd 解码成路径；flood 场景含 critical 检查点 + gap span 大量落盘。）

## 五、门读数（全过）

| 门 | 命令 | exit | 读数 |
|---|---|---|---|
| test | `cargo test --workspace --all-features` | 0 | 152 套全 ok，**0 failed**，共 1519 passed |
| test（默认） | `cargo test --workspace` | 0 | 152 套全 ok，0 failed |
| clippy | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | 无警告 |
| clippy（默认） | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 无警告 |
| clippy_gate | `cargo clippy --workspace --all-targets --all-features --message-format=json` → error 级 **0 条** → `python3 -I ci/clippy_gate.py <jsonl>` | 0 | `CLIPPY GATE: PASS`（基线 0 签名，未动） |

`ci/clippy-baseline.txt` 零改动（`git diff HEAD -- ci/` 为空）。crates 集合与根 Cargo.toml 未动。

## 六、如实登记的边界

1. **`Ok = 已入队` 没有改成「Ok = 已持久」**。既有测试（容量内 try_send 立即 Ok、
   500ms 慢消费者下瞬时返回）钉死了移交语义且本任务不许改既有断言；把 write_critical
   变成同步阻塞等 fsync 是 API 破坏性变更，**需人类裁决**。本任务的收敛方式：
   强度升级到 fsync+目录 fsync，Ok 语义明示，确认点固定在 shutdown，运行中给
   durable 读数。
2. **掉电/介质级崩溃一致性无法在 CI 实证**（不能拔盘）；以 strace 证明 syscall 形态
   （fdatasync→fsync、目录 fsync 出现）为最强可检证据。
3. **锚定侧车残余缺口**：`critical.anchors.jsonl`/`anchor-queue.jsonl`/`anchor-status.json`
   由锚线程运行中新建，其目录项不在 open 路径的 `sync_dir` 覆盖内（锚定是「不阻塞落盘」
   的附加层，回执可由 anchor-queue 重试语义对账）。未扩大战线，登记在此。
4. **`trace.db` 持久档位未动**（SQLite WAL + `synchronous=NORMAL`，自身另有重开对账
   fail-closed）；不在本风险范围。
5. **性能**：fdatasync→fsync 每条多刷 inode 元数据；critical 轨本就逐条 fsync，增量
   未单独基准（非本任务范围）。
6. 调用方（`af-harness/src/run.rs:818,832`）把 `write_critical` 的 `Err` 传播为 harness
   错误（fail-closed 正确），未发现把 `Ok` 误当持久确认再去做不可逆动作的调用点。

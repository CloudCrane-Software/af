# REPORT — poc-kernelrun：内核件『裸进程』部署形态实测

任务 id：`kernelrun`  日期：2026-10-19
工程：`af/poc/poc-kernelrun/`（独立 cargo workspace，path 依赖引入 `crates/*` 全部 11 个 crate，AF 根 Cargo.toml 未动）

---

## ① 结论

**裸进程部署叙事成立（带明确边界）**：11 个内核 crate 以 path 依赖一次链接进一个普通 bin，**单进程、零端口、零容器、零服务发现**地跑完全部 11 件事并逐项断言通过；冷启动（fork+exec → 11 件全部初始化完成）10/10 成功，内部耗时中位 191.8 ms、外部墙钟中位 203.5 ms、RSS ≈ 19.5 MB。SQLite 文件树形态实证：默认形态 = 每持久件一个 `.db`（运行中带 `-wal`/`-shm`，干净退出收敛回单文件）；共库形态 = 多件多连接同文件可行（跨连接认领验证通过）；重启后 epoch/意图/任务三处状态原样留存。

**缺口（详见④）**：叙事里"内核 = 一组进程 + SQLite 文件树"的**文件树目前只覆盖 2/11 件**——只有 gateway/station 落 SQLite，meter/release-gate/sampling/stop/trace 全是内存态（重启即丢）；凭证根密钥无持久形态（每次运行随机生成，重启即换根公钥）；首启/重入两条初始化路径未定义（本次以断言失败实测到该缺口）。

---

## ② 步骤

1. **读前置材料**：`crates/*/RESULT.md` 11 份全读；勘察各 crate 公开 API（lib.rs / 具体模块签名）。
2. **建独立工程** `af/poc/poc-kernelrun/`：
   - `Cargo.toml`：空 `[workspace]` 表（自成 workspace 根，防向上挂进 AF workspace；AF 根 members = `["crates/*"]` 本就不含 poc/*，事后复核未变）；11 个 crate 全部 path 依赖；bin 自身仅加 serde_json/chrono/chrono-tz；
   - 复制 `af/Cargo.lock` 作为初始锁文件，使依赖版本与 11 份 RESULT.md 验证过的组合一致。
3. **写 bin `src/main.rs`**，四个子命令：
   - `run <data_dir>`：单进程依次初始化 11 件核心结构并逐项断言（任何断言失败 = panic = 非零退出码）；
   - `layout-default <data_dir>`：默认形态（gateway.db + station.db 各自独立），持连接 6 s 供外部清点运行中文件；
   - `layout-shared <data_dir>`：共库形态（三条独立连接指向同一 shared.db：网关 + station A + station B），并做跨连接交接。
4. **构建**：`source ~/.cargo/env && cargo build -j4`（在 poc-kernelrun/ 内，独立 target 目录）。
5. **冒烟**：`run` 一次全新数据目录。
6. **布局实验**：两个 layout 子命令各跑一次，运行中/退出后各清点一次文件；用 python3+sqlite3 读 shared.db 表清单、读 run-smoke 的两库状态。
7. **冷启动画像**：脚本 `cold/bench.sh` 循环 10 次，每次全新数据目录，外部 `date +%s%N` 计墙钟、`/usr/bin/time -f "%M"` 记 RSS、内部读 bin 输出的 `COLD_START_MS`。
8. **热重入实验**：对已有数据目录重跑 `run`，观察重启语义（预期外发现）。
9. **收尾**：`ps`/`ss` 确认无残留进程、无监听端口（本任务从未开端口，trace 后台线程随进程退出）。

---

## ③ 证据

### 3.1 构建（真实执行）

```
$ cd af/poc/poc-kernelrun && source ~/.cargo/env && cargo build -j4
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 13s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
```

冷编译 1m13s（-j4，独立 target 目录全量）；警告仅 cedar 传递依赖的已知 future-incompat 提示，本工程代码零警告。`binary_bytes=187380992`（debug 未裁剪）。

### 3.2 主场景 11 步（冒烟轮，全新数据目录，`./target/debug/poc-kernelrun run run-smoke`，exit=0）

```
STEP 1/11 [credential 签发一对] ✓ OK (24ms) root(test,git@line-a,budget=1000c)+child(test,≤500c); allow(test)=true, deny(git)=true
STEP 2/11 [graph 校验一条] ✓ OK (0ms) g-7 三节点 DAG 零违例; 回边负例报 cycle(1 条违例)
STEP 3/11 [workorder 一跃迁] ✓ OK (0ms) Created→Released; 负例缺失清单=[LineVersionNotBound, BudgetVoucherNotIssued]
STEP 4/11 [gateway 记账一条] ✓ OK (79ms) gateway.db 意图 1 条 executed, 外呼恰好 1 次, epoch=1
STEP 5/11 [sampling 喂三件] ✓ OK (0ms) 队列 4 件(金标注入 1), 全检拦截缺陷金标 0, 升迁引擎 Hold@Agent
STEP 6/11 [release-gate 放行一件] ✓ OK (0ms) 放行 1 件(10min); WipCap 负例拒绝; complete 后 WIP=0
STEP 7/11 [meter 扣一次] ✓ OK (0ms) Min(周1000,日200) 扣 80 → available=120; 超扣 150 被拒
STEP 8/11 [calendar 算一单] ✓ OK (0ms) 周五16:00+4h→周一11:00; 截单点回算=周五16:00
STEP 9/11 [trace 收一条] ✓ OK (0ms) critical seq=0 已受理(Ok=保证落盘) + normal 1 条
STEP 10/11 [stop 喂两事件] ✓ OK (0ms) 两事件均 Continue; 指纹(search,E_TIMEOUT) 连击=2/3
STEP 11/11 [station 认领一任务] ✓ OK (55ms) task-1 被 worker-1 认领(租约60s); worker-2=None
COLD_START_MS=160.805
TRACE_RECONCILE critical_persisted=1 normal_persisted=1 gap=0
STEPS_OK=11
exit=0
```

逐步断言内容（`src/main.rs`，均为双向：正例 + 负例）：
1. credential：`issue_root`+`attenuate` 签发一对 → serialize/`deserialize_verify`(根公钥) 往返 → `evaluate` 子凭证 test@line-a 放行、git 被衰减 check 封死且 `token_carries_capability=true`（append-only 事实仍在）。stderr 的拒绝原因行来自 `af-credential/src/credential.rs:171` 的 crate 自带日志。
2. graph：三节点 DAG（含 human gate 节点）JSON → serde 解析 → `validate` 零违例；回边负例报 `Rule::Cycle` 恰 1 条。
3. workorder：空单 release 被拒且缺失清单含 `LineVersionNotBound`+`BudgetVoucherNotIssued` → 绑产线+挂签名凭证 → `Transition::Release` 成功 → graph_version 不变、履历 1 条。
4. gateway：open gateway.db → `resume()` epoch=1 → declare `Created` → 窗口内 execute 被拒（`WindowStillOpen`）→ 窗口关后 execute `RanFresh` 外呼**恰好 1 次**（闭包计数器）→ 快照 `Executed` → 重放返回原回执 T-1（`is_replay`）。
5. sampling：注入器(seed=42,p=0.5) 喂 3 件真实件（1 件缺陷）→ 队列 ≥3 → CSP-1(f=1.0 全检) 全数处理 → 金标台账四契约断言（零告警/零误拦/缺陷金标全拦/台账==注入数）→ 升迁引擎 `from_recipe_json` 收一条 RunOutcome → `Hold@Agent`。
6. release-gate：`try_admit` 放行 1 件(10min) → 10+25>30 负例 `WipCap` → `complete` 释容 WIP=0 → 释容后同件可入。
7. meter：`Min(周窗1000, 日窗200)` `try_consume(80)` → `available=120` → 超扣 150 报 `Insufficient{available=120}`。
8. calendar：`weekday_8x5_shanghai` 周五 2026-10-23 16:00 + 4 工时 = 周一 2026-10-26 11:00（`sla_deadline`）→ `latest_order_at` 反向截单回到周五 16:00（正反互逆）。
9. trace：`write_critical` 返回 `Ok(seq=0)`（契约=保证落盘）+ `write_normal` 1 条 → shutdown 后对账：critical_persisted=1、gap=0。
10. stop：同指纹（search,E_TIMEOUT）连喂 2 事件 → 均 `Continue`（N=3 未达）→ `current_streak()==2`。
11. station：new_task → `claim("worker-1","reviewer",60s)` 拿到 `Claimed`+租约 → `claim("worker-2")`=`None` → `count_by_state(Claimed)==1`。

### 3.3 ② SQLite 文件布局

**默认形态**（`layout-default` 子命令：gateway.db + station.db 各自独立，连接保持打开时外部 `ls`）：

```
=== 持连接期间（运行中）layout-default/ ===
-rw-r--r-- 4096  gateway.db
-rw-r--r-- 32768 gateway.db-shm
-rw-r--r-- 45352 gateway.db-wal
-rw-r--r-- 4096  station.db
-rw-r--r-- 32768 station.db-shm
-rw-r--r-- 41232 station.db-wal
=== 进程退出后 layout-default/ ===
-rw-r--r-- 20480 gateway.db
-rw-r--r-- 20480 station.db
```

清单：**运行中每持久件 3 个文件（.db / -wal / -shm，两件均开 WAL）；干净退出后 WAL checkpoint 并删除，收敛为每件 1 个 .db（20 KB 量级）**。

**共库形态**（`layout-shared`：网关 + station A + station B 三条独立连接指向同一 `shared.db`，同进程）：

```
=== 持连接期间 layout-shared/ ===
shared.db (4096) + shared.db-shm (32768) + shared.db-wal (86552)
CROSS-CONNECTION-OK claimed_by_b=t-shared      ← A 入账的任务由 B 的连接认领成功
=== 退出后 ===
shared.db (36864)
=== shared.db 内表清单（python3 sqlite3 实读）===
effect_intent 1
gateway_meta  1
task          1
```

结论：**同一 SQLite 文件可同时承载多件（表名互不冲突），同进程多连接（=多 worker 形态）跨连接交接正确**。

**重启后的状态留存**（对 run-smoke 热重入触发 panic 后实读两库）：

```
gateway_meta: [('epoch', '2')]                     ← 首启 resume→1，重入 resume→2，跨进程持久递增
effect_intent: [('wo-1','publish.notify',1,'executed',1,1)]   ← 回执留存
task: [('task-1', 'claimed', 'worker-1')]          ← 账本留存
```

### 3.4 ③ 冷启动画像（10 次，每次全新数据目录）

命令：`cold/bench.sh`（循环内 `/usr/bin/time -f "RSS_KB=%M"` 包裹，`date +%s%N` 计墙钟）：

```
run=1 steps_ok=11 STEPS_OK=11 internal_ms=235.869 wall_us=243821 rss_kb=19468
run=2 steps_ok=11 STEPS_OK=11 internal_ms=216.867 wall_us=226895 rss_kb=19468
run=3 steps_ok=11 STEPS_OK=11 internal_ms=215.565 wall_us=225153 rss_kb=19600
run=4 steps_ok=11 STEPS_OK=11 internal_ms=191.583 wall_us=205828 rss_kb=19592
run=5 steps_ok=11 STEPS_OK=11 internal_ms=187.583 wall_us=197544 rss_kb=19536
run=6 steps_ok=11 STEPS_OK=11 internal_ms=160.773 wall_us=168347 rss_kb=19504
run=7 steps_ok=11 STEPS_OK=11 internal_ms=192.064 wall_us=201214 rss_kb=19460
run=8 steps_ok=11 STEPS_OK=11 internal_ms=217.233 wall_us=226704 rss_kb=19552
run=9 steps_ok=11 STEPS_OK=11 internal_ms=190.429 wall_us=200156 rss_kb=19564
run=10 steps_ok=11 STEPS_OK=11 internal_ms=166.817 wall_us=175260 rss_kb=19452

统计（python3 实算）：
internal: n=10 min=160.8 max=235.9 median=191.8 mean=197.5 stdev=23.7 ms
wall:     n=10 min=168.3 max=243.8 median=203.5 mean=207.1 stdev=23.9 ms
rss:      n=10 min=19452 max=19600 median=19520 mean=19519.6 stdev=56.5 KB
```

耗时构成（detail 轮逐步计时）：大头是 **credential 24–46ms（biscuit KeyPair 生成）+ gateway 79–99ms（含 30ms 撤回窗口 sleep + SQLite 建库/DDL/首写 ≈ 50–70ms）+ station 55–84ms（SQLite 首开+首写）**；其余 8 件（graph/workorder/sampling/release-gate/meter/calendar/trace/stop）单步均 <1ms（纯内存计算）。±24ms 波动主要来自两次 SQLite 首开。`internal_ms` 口径 = main 第一行 → 11 步完成；`wall` = fork+exec → 进程退出。

### 3.5 热重入实验（预期外发现）

对既有 run-smoke 数据目录重跑 `run`：

```
STEP 1-3 ✓ OK（纯内存件全部无感通过）
thread 'main' panicked at src/main.rs:213:9: 首次声明应为 Created
exit=101
```

gateway 幂等层行为**正确**（同键同载荷 → `AlreadyDeclared`，不产生第二行），但初始化脚本按"首启"形状写断言，重入即炸——见④-3。

### 3.6 环境与纪律核查

- `ps aux | grep -c '[p]oc-kernelrun'` = 0（无残留进程）；`ss -tlnp` 无任何监听（本任务未开端口）。
- AF 根 `Cargo.toml` 事后复核：`members = ["crates/*"]` 原样未动；11 个 AF crate 源码零修改；所有产物（target/、data 目录、脚本、本报告）均在 `af/poc/poc-kernelrun/` 内。
- 失败如实记录：第一版 10 次循环脚本有 shell 重定向笔误（`2>/dev.time`），产生 10 条无效数据（`steps_ok=0`），**作废重跑**，上表 10 次为修正脚本后的真实数据。

---

## ④ 对方案的反馈 / 决策输入

### 成立的部分（裸进程叙事的实证面）

1. **整体进程化零阻碍**：11 crate path 依赖一次链接成单 bin，全部 pub API 可从外部驱动；无需 tokio 运行时、无服务发现、无端口。全程只有一次 stderr 日志（credential 拒绝原因，`af-credential/src/credential.rs:171`）——进程化后可观测性目前只有这一条 + trace 内存 sink。
2. **SQLite 文件树形态可行且可预期**：默认 2 文件、运行中 6 文件、退出收敛 2 文件；共库多件（3 连接 1 文件）表级共存 + 跨连接交接正确。**"内核 = 一组进程 + 一组 SQLite 文件"在单机上的最低版本成立**。
3. **冷启动成本量级**：~200ms / 19.5MB RSS 拿下全部 11 件——对"进程可随工单随时拉起"的编排叙事是绿灯；耗时大头是两次 SQLite 首开与一次 biscuit 密钥生成，不是业务逻辑。
4. **状态可恢复性已实证一处闭环**：重启后 epoch（fencing）单调持久、已执行意图与已认领任务原样可读——gateway/station 的"文件即状态"承诺兑现。

### 缺什么（发现维度清单，按对裸进程叙事的伤害排序）

1. **持久化只覆盖 2/11 件**：meter/release-gate/sampling/stop/trace 全是内存态，重启即丢（各自 RESULT.md 自认"无持久化"）；workorder/graph 是纯值对象，状态在哪一级（文件？DB？）未定义。"SQLite 文件树"要成为内核事实，需要给这 5 件定持久化方案（或明确声明它们是无状态纯函数件、状态由调用方持有）。
2. **凭证根密钥无持久形态**：`KeyPair::new()` 每次随机——裸进程重启 = 换根公钥 = 存量凭证全部验签失败。密钥的来源（文件/环境变量/启动参数）与轮换在部署形态里完全未定义（af-credential RESULT 遗留亦列了密钥分发未验证）。
3. **首启/重入两条路径未定义**（3.5 实测）：幂等层正确但返回值语义不同（`Created` vs `AlreadyDeclared`）。内核需要显式的 bootstrap（首启：建库/发根凭证/绑产线）与 replay（重启：resume+续租+重放）分叉，而不是一份脚本原样重跑。fencing epoch 的"每次 open 都 resume+1"语义在多进程下谁来调用也要定。
4. **跨件装配层不存在**：本次 bin 里"wo-1 → gateway.workorder_id → station.payload_ref → graph_version g-7"是我手工对齐的字符串。11 件的键（工单 id、图版本、凭证 namespace、计量供给名、指纹）之间没有统一的登记/校验层，进程化编排时这正是出错点（af-credential RESULT ④-6 的词表一致性测试是同一问题的另一个面）。
5. **trace 落盘是内存 Vec**：`TraceSinks.critical/normal` 是 `Arc<Mutex<Vec<String>>>`，进程死则丢。裸进程形态下 trace 文件应是文件树一部分（其 RESULT 自认"换真实文件只改 spawn_consumer 一处"）；且与 credential 的 stderr 日志未整合。
6. **时钟语义分散**：release-gate 收 `MinuteOfDay(u32)`、meter 收 `f64 秒`（模拟钟）、station/gateway 用本机 `SystemTime`、calendar 用 chrono 时区钟。单机裸进程可行，但"一组进程"下时钟源要不要统一注入（af-workorder RESULT 遗留的 `dyn Clock`）需要拍板。
7. **分发形态未做**：debug bin 187MB 未裁剪（opt/strip/split-debuginfo）；无任何监督/拉起机制（systemd unit？裸 nohup？）——这是部署形态决策，本次未验证。
8. **SQLite 并发上界待压**：station RESULT 自述"写 QPS 上百需重估"；本次共库实验是同进程多连接，**真·两个 OS 进程共库未测**（诚实记录：没跑）。

---

## ⑤ 遗留问题

- 真·多进程共库（两个 OS 进程各开连接 + 并发写竞争）与断网环境（`sudo unshare -n`）下的行为未测——本任务书未要求，未跑。
- 冷启动画像为 debug 构建数字；release 构建未测（预期显著小于 200ms，但那是推测非实测）。
- 热重入脚本的正确写法（首启/重入分叉：`Created`→bootstrap，`AlreadyDeclared`→replay 后继续）未实现，只以失败形态定位了缺口。
- 步骤 4 的 30ms 撤回窗口 sleep 计入 `internal_ms`（约占总冷启动 15%）；若要纯初始化口径可把窗口等待挪出计时，本次如实保留。
- 第一版循环脚本的 10 条无效数据已作废；`cold/bench.sh` 为修正后脚本，保留在工程内可复跑。

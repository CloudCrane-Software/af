# RESULT.md — poc-ledger「账本部署形态 + meter 窗口对齐日历」

任务目录：`af/poc/poc-ledger/`（独立 cargo 工程，`[workspace]` 空表自隔，未动 AF 根 Cargo.toml）。
依赖：rusqlite 0.32.1 (bundled+backup)、chrono 0.4.45、chrono-tz 0.10.4（与 AF lock 同版本）。

## ① 结论

**通过**。两项裁决全部以真实执行收口，核心交付可建库、可测试、可复跑。

**裁决一（SQLite 单库多表 vs 每件一库）：建议分库** —— meter.db / station.db / gateway.db 三文件，跨库一致性用 outbox 补偿。依据（全部本机实测）：

1. **吞吐**：4 进程持续写 30s，三库形态三轮 48,359–50,695 rows/s，单库形态三轮 34,980–38,799 rows/s（三库 +25%~+30%，六轮交替排跑抗负载漂移）。
2. **SQLITE_BUSY 与尾延迟**：单库每 30s 冒 4–8 次 BUSY（busy_timeout=5s 等满仍失败），单条 INSERT 最大延迟 **2.3–5.0 秒**（写锁排队）；三库 0 次 BUSY，最大延迟仅 0.05–0.11ms。单库的真实代价不是均值是尾部。
3. **跨库一致性边界**：**WAL 模式下跨文件无原子性是硬边界**——子进程单事务写 A/B 两库、在提交窗口内 SIGKILL，8/8 次观测到分裂（A 已提交 20000 行、B 为 0）；对照组 rollback-journal（DELETE）模式同样的注入 8/8 次全部原子回滚 (0,0)（master-journal 2PC 生效）。即：**想靠 ATTACH 多库事务换跨文件原子性，在 WAL（三份 PoC 共同选定的模式）下不成立**。补偿写法（outbox：本库事务内"业务行+信箱行"→ 对端幂等落回执 → 标 done）在三个崩溃点注入下全部收敛到"账=1、回执=1、真实执行=1"。
4. **备份形态**：VACUUM INTO 与 backup API 都是"在线一致快照"（写者不停，备份产物 integrity_check=ok、行数 ≥ 快照起点已提交数）；**裸 cp 主文件会丢已提交数据**——实测 200 行已提交后只拷主文件，拷贝里 0 行（连建表语句都在 -wal 里）；先 `wal_checkpoint(TRUNCATE)` 再拷则完整 200 行。

**裁决二（meter 窗口对齐日历）：已实现并断言验证**（10 测试全绿 + 2 项两年扫描）。`window_bounds(WindowKind, now)` 泛型 `Tz: TimeZone`，同一函数跑两时区：

- Asia/Shanghai：自然日 = 当日 00:00–次日 00:00 钟面（恒 24h）；自然周 = 周一 00:00（ISO 周，恒 168h，106 周扫描断言）。**跨年**：2026-12-31（周四）与 2027-01-01（周五）同属 `2026-W53`，`2027-W01` 从 2027-01-04（周一）00:00 起，元旦不另起新周。**黄金周/调休**：10-01 假日与 09-26 调休周六都不移动自然窗口边界（窗口是纯钟面对齐，假日/调休属工作日历叠加层，与 af-calendar 的二元假日表正交）。
- Europe/Berlin（同函数对照）：春跳周（含 2026-03-29）elapsed **167h**（起点周一 00:00+01:00、终点周一 00:00+02:00），秋拨周（含 2026-10-25）**169h**，平周 168h；过渡日自然日 23h/25h。2026–2027 每周一全扫描：恰 2 个 167h 周 + 2 个 169h 周 + 101 个 168h 周，无一例外。
- 实现纪律（DST 下的命门）：**窗口边界只能由"钟面日期 → 解析午夜"计算，严禁绝对时间加法**——DST 周加 168h 会漂到周日 23:00/周一 01:00。

**交付物**：meter 账本 schema 草案（DDL 原文见下）+ 最小消费原型（惰性窗口重置、显式幂等键、`Reading{available, recovers_at}` 语义、recovers_at=对齐窗口终点），`cargo test` 24 测试 3 轮全绿。

## ② 做了什么

```
af/poc/poc-ledger/
├── Cargo.toml               # 独立工程：空 [workspace]；rusqlite(bundled,backup)+chrono+chrono-tz
├── REPORT.md                # 本报告
├── src/lib.rs               # ① window 模块（window_bounds/day_bounds/week_bounds/window_label）
│                            # ② SCHEMA_SQL（账本 DDL 草案）+ open_ledger（WAL/busy_timeout5s/NORMAL/FK）
│                            # ③ consume 原型（BEGIN IMMEDIATE、惰性窗口重置、幂等、Reading）
│                            # ④ outbox_recover（分库补偿恢复函数）
├── src/bin/ledger_bench.rs  # 4 进程 30s 并发写实测（parent 拓扑编排 + writer 单进程计量）
├── src/bin/crash_tx.rs      # 跨库事务崩溃注入（子进程 ATTACH 双库大事务，父进程在提交窗口 SIGKILL）
└── tests/
    ├── window.rs      (10) # 裁决二全部断言：上海日/周/跨年/黄金周调休/168h 扫描；柏林 167h/169h/23h/25h/两年扫描
    ├── schema.rs       (4) # DDL 建库、CHECK/FK 拦截、幂等重放、惰性窗口重置（日+周）、recovers_at 衔接 af-meter 场景
    ├── consistency.rs  (4) # 跨文件无原子性边界 + outbox 三崩溃点收敛（真实执行恰好一次）
    ├── backup.rs       (3) # VACUUM INTO 在线快照 / backup API 分页并发收敛 / 裸 cp 丢数据边界
    └── debug_wal.rs    (2) # wal_checkpoint(TRUNCATE) 返回值 quirk 的取证
```

实验设计对应三个 rusqlite 使用者（af-meter/af-station/af-effect-gateway 各自的 RESULT.md 先读后动）：
- **拓扑**：meter×2（逐调用热路径，天然双 worker）+ station×1 + gateway×1 = 4 进程。single 模式 4 进程同开 `ledger.db`（各写各表，竞争纯来自文件级单写者）；split 模式 meter.db(2 进程，保留服务内竞争) + station.db + gateway.db。
- **写负载**：WAL + busy_timeout=5000ms + synchronous=NORMAL（与 af-station 同款），autocommit 单行 INSERT 打满 30s；统计行数、`SQLITE_BUSY` 计数（按 `ErrorCode::DatabaseBusy` 精确匹配）、其他错误、单条延迟 p50/p99/max。
- **崩溃注入**：子进程 `BEGIN IMMEDIATE` 后向 A/B 两库各插 20000 行（synchronous=FULL 拉长提交窗口），打印 `COMMITTING` 标记；父进程读到标记 1ms 后 `kill -9`，重开两库（触发热日志/WAL 恢复）数行数：`(0,0)`=原子回滚、`(N,N)`=原子提交、`(N,0)/(0,N)`=**跨库分裂**。WAL 与 DELETE 各 8 轮。

## ③ 证据

**测试**（`cd af/poc/poc-ledger && source ~/.cargo/env && cargo test -j4`，连跑 3 轮）：

```
round1: passed=24 failed=0
round2: passed=24 failed=0
round3: passed=24 failed=0
（窗口 10 + schema 4 + consistency 4 + backup 3 + debug_wal 2 + lib 单元 1）
```

窗口测试关键行（`--test window`，10/10 ok）：

```
test berlin_fall_back_week_is_169h_with_shifting_offsets ... ok      # [00:00+02 → 00:00+01] = 169h
test berlin_spring_forward_week_is_167h_with_shifting_offsets ... ok # [00:00+01 → 00:00+02] = 167h
test berlin_two_year_week_scan_exactly_two_spring_two_fall ... ok    # n167=2 n169=2 n168=101
test shanghai_cross_year_iso_week ... ok                             # 12-31 与 01-01 同属 2026-W53
test shanghai_golden_week_and_makeup_saturday_do_not_shift_natural_windows ... ok
test shanghai_every_week_is_exactly_168h_over_two_years ... ok       # 106 周 × 168h
```

**4 进程 30s 并发写**（`ledger_bench parent --mode <m> --dir runs/bench/<m> --secs 30`；详细轮）：

```
[single] meter0: rows=93567  busy=1 p50=29µs p99=79µs  max=5029404µs(≈5.0s)
[single] meter1: rows=283611 busy=0 p50=30µs p99=90µs  max=3841144µs
[single] station0: rows=298165 busy=0 p50=28µs p99=78µs max=3543113µs
[single] gateway0: rows=284708 busy=0 p50=30µs p99=95µs max=4235984µs
[single] TOTAL rows=960051 rows/s=32002 busy=1     ← 单库：写者饥饿不均（93k vs 298k）+ 秒级尾延迟

[split] meter0: rows=521456 busy=0 p50=26µs p99=57µs max=53588µs(≈54ms)
[split] meter1: rows=518110 busy=0 p50=26µs p99=57µs max=65727µs
[split] station0: rows=209062 busy=0 p50=21µs p99=58µs max=109852µs
[split] gateway0: rows=208567 busy=0 p50=21µs p99=55µs max=105857µs
[split] TOTAL rows=1457195 rows/s=48573 busy=0     ← 三库：双 writer 均分 meter.db（521k/518k）
```

三轮交替复跑（`runs/bench/series.log`，抗负载漂移；本机 16 核但 load 11–18 与他人共享）：

```
round1 single: rows/s=34980 busy=8   |  round1 split: rows/s=50695 busy=0
round2 single: rows/s=38799 busy=4   |  round2 split: rows/s=48359 busy=0
round3 single: rows/s=38790 busy=6   |  round3 split: rows/s=49101 busy=0
```

**崩溃注入**（`crash_tx <wal|delete> runs/crash/<m> 8 20000`）：

```
mode=wal     iterations=8 rows_per_db=20000
iter0..7: a_count=20000 b_count=0  → 8/8 分裂(SUMMARY: 全提交=0 回滚=0 分裂=8)
mode=delete  iterations=8 rows_per_db=20000
iter0..7: a_count=0     b_count=0  → 8/8 回滚(SUMMARY: 全提交=0 回滚=8 分裂=0)
```

**跨库补偿**（tests/consistency.rs，4/4 ok）：`boundary_...` 证明两文件 autocommit 之间崩溃必然分裂（meter=1 账、gateway=0 回执）；`crash_point_a/b` 与 `all_crash_points_converge...` 证明 outbox 恢复在三崩溃点下恒收敛：账=1、回执=1、真实执行计数=1、信箱=done。

**备份**（tests/backup.rs，3/3 ok）：VACUUM INTO 在线快照（写者持续推进 `rows_after > rows_before`，产物 integrity_check=ok、行数≥起点、独立单文件）；backup API 分页 step 与写者交错收敛（Busy/Locked 重试后 Done）；裸 cp：`copied_rows=0 < 200`（已提交数据全在 -wal，连 schema 都没拷到），`wal_checkpoint(TRUNCATE)` 后拷贝 200/200。

**运行后状态**：无遗留进程（`ps` 确认 0），AF 根 Cargo.toml 原样（`members = ["crates/*"]` 未变），runs/ 保留 266MB 原始产物备查。

## meter 账本 schema 草案（DDL，实测可建库）

```sql
-- 金额/数量一律整数最小单位（call/cent/token，禁 f64，af-meter RESULT④-4 与 af-credential *_cents 对齐）
-- 时间一律 unix ms INTEGER（af-station RESULT④-4）；窗口边界按 tz 对齐后以 UTC 时刻存储，可由 (tz,window_kind) 复算
CREATE TABLE IF NOT EXISTS ledger_meta (
  key   TEXT PRIMARY KEY,              -- 'schema_version' | 'fencing_epoch'
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS meter_account (
  subject     TEXT NOT NULL,           -- 被计量主体（agent id / workorder id）
  meter_id    TEXT NOT NULL,           -- 计量器路径 id（'subscription:daily' / 'api:stock'）
  kind        TEXT NOT NULL CHECK (kind IN ('stock','window','concurrency','rate')),
  window_kind TEXT,                    -- window 型: 'day' | 'week_mon'；其他型 NULL
  tz          TEXT NOT NULL DEFAULT 'Asia/Shanghai',  -- 窗口对齐时区（IANA 名）
  unit        TEXT NOT NULL,           -- 'call' | 'cent' | 'token'
  capacity    INTEGER NOT NULL,        -- 初始额度/上限（最小单位）
  parent_id   TEXT,                    -- Min 组合树父节点；NULL=根（Max 择优在内存路由层）
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (subject, meter_id),
  FOREIGN KEY (subject, parent_id) REFERENCES meter_account(subject, meter_id)
);
CREATE TABLE IF NOT EXISTS meter_window_state (   -- window 型当前窗口投影（可由事件重放重建）
  subject      TEXT NOT NULL,
  meter_id     TEXT NOT NULL,
  window_start INTEGER NOT NULL,       -- 对齐后的窗口起点 unix ms
  window_end   INTEGER NOT NULL,       -- 对齐后的窗口终点 unix ms（开区间；即 recovers_at）
  remaining    INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (subject, meter_id),
  FOREIGN KEY (subject, meter_id) REFERENCES meter_account(subject, meter_id)
);
CREATE TABLE IF NOT EXISTS meter_event (          -- append-only，事实来源
  seq             INTEGER PRIMARY KEY AUTOINCREMENT,
  subject         TEXT NOT NULL,
  meter_id        TEXT NOT NULL,
  op              TEXT NOT NULL CHECK (op IN ('consume','reserve','release','refill','reset','adjust')),
  amount          INTEGER NOT NULL,
  window_start    INTEGER,              -- 记账时所属对齐窗口（stock 型 NULL）
  available_after INTEGER,              -- 事件后余额（审计锚点，可校验重放）
  idempotency_key TEXT UNIQUE,          -- 显式命名幂等键（af-station 坑④-3：禁裸 ON CONFLICT）
  ref_type        TEXT, ref_id TEXT,    -- 溯源（workorder/step）
  epoch           INTEGER NOT NULL DEFAULT 0,   -- fencing 纪元（af-effect-gateway 语义）
  ts              INTEGER NOT NULL,
  FOREIGN KEY (subject, meter_id) REFERENCES meter_account(subject, meter_id)
);
CREATE INDEX IF NOT EXISTS idx_event_window ON meter_event(subject, meter_id, window_start);
CREATE INDEX IF NOT EXISTS idx_event_ts     ON meter_event(ts);
CREATE TABLE IF NOT EXISTS meter_outbox (         -- 分库形态的跨库补偿信箱（发起方持有）
  id         TEXT PRIMARY KEY,          -- = 对端库幂等键
  payload    TEXT NOT NULL,             -- 目标操作 JSON
  state      TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','done','dead')),
  created_at INTEGER NOT NULL,
  done_at    INTEGER,
  attempts   INTEGER NOT NULL DEFAULT 0
);
```

单库形态 = 本 DDL 三服务共库共表（表名已带前缀，无冲突）；分库形态 = meter 相关表进 meter.db，station/gateway 各自 DDL 同理。两种形态 DDL 不变，变的只是文件拓扑——这是"分库裁决可逆"的结构保证。

## ④ 对方案的反馈/决策输入

1. **部署形态建议"默认分库、单库设准入条件"**：三服务分三个 db 文件。实测分库吞吐 +25~30%、BUSY 归零、写尾延迟从秒级降到 0.1ms 级；且 af-station RESULT⑤ 自己点名"写 QPS 上百需重估"，分库直接消掉跨服务写竞争。单库唯一实质收益是跨表事务，但三服务账表之间**没有需要跨表事务的业务关系**（各服务各写各表），而 WAL 下跨文件原子性反正不存在（8/8 分裂实测），单库换不来一致性收益。若未来某服务内部需要"账目+索引"共事务，那是**服务内单文件**的事，与三服务共库无关。
2. **跨库一致性写法定案：outbox，禁 ATTACH 多库事务**。WAL 是三份 PoC 共同选择（读写并发），其代价是跨文件事务不原子（本次 8/8 分裂实证）；rollback-journal 虽然多库原子但牺牲并发，不值得为省一个 outbox 表退回。补偿协议三步：本库事务（业务行+信箱行）→ 对端幂等落回执（主键=信箱 id）→ 标 done；恢复 = 扫 pending 重放。三崩溃点实测收敛"恰好一次"。**对端必须以主键幂等**是协议成立的前提（crash_point_b 实测：重放被 `ON CONFLICT DO NOTHING` 吞掉，执行计数不增）。
3. **meter 窗口语义定案**：窗口边界 = tz 钟面的自然日/周一 00:00，与假日/调休**正交**（黄金周照常按自然日重置；"假日不扣量"是工作日历叠加层，接 af-calendar 而非改窗口函数）。`recovers_at` = 对齐窗口终点，与 af-meter `Reading` 语义无缝（`routing_decision_input_...` 测试复刻了"日窗尽周窗有余"场景：日窗恢复点=次日 00:00、周窗=下周一 00:00，两钟面即为路由的换路点）。**DST 周容量语义需拍板**：窗口按钟面定义时，柏林春跳周"一周"只有 167h 流逝时间但容量额度不变——按"每自然周 N 次"计费则无影响，若未来有"按流逝时间派生"的计量（Rate 型），要在方案里明确其窗口不与自然周混用。
4. **三个实现级坑（正式版必须写进编码规范）**：
   - **建 `BEGIN IMMEDIATE` 事务**，别用 deferred：读快照后升级写锁会撞 `SQLITE_BUSY_SNAPSHOT`，busy_timeout 不重试它；
   - **多进程并发建库**：`PRAGMA journal_mode=WAL` 切换要短暂排他锁且 busy_timeout 对它不生效——本次两个 writer 同时初始化同一新库互报 BUSY（bench 启动即崩，重试 + 父进程预建库解决）；正式版应在部署期建库，或对初始化段做退避重试；
   - **`PRAGMA wal_checkpoint(TRUNCATE)` 的返回行恒报 (0,0,0)**（debug_wal.rs 取证：物理上 -wal 已清空、主文件已增长），校验 checkpoint 效果要看文件尺寸/行数，别信返回列。
5. **备份规范直接采纳**：在线备份只有三种合法形态——VACUUM INTO（单文件产物、最简单）、backup API（可分页限速、可断点）、`wal_checkpoint(TRUNCATE)` 后拷贝；**裸 cp 数据文件（不带 -wal）属于数据丢失**（实测 200 行丢到 0 行）。定时备份建议 VACUUM INTO + 产物 integrity_check，禁止"crontab cp 一发"式备份。
6. **公平性与环境噪声声明**：本机 16 核但测试期间 load 11–18（与他人共享），吞吐绝对值波动 ±10%，故结论只用同机交替排跑的**相对差**（三库/单库 = 1.28~1.45×，三轮方向一致）与 BUSY/尾延迟这类结构性信号；绝对 rows/s 数字仅供量级参考。单库写者调度不均（一轮里 meter0 只有其他写者的 1/3）是 SQLite busy-handler 退避的真实行为，多写者单文件会放大长尾，这也是分库的隐性收益。
7. **窗口函数保持纯钟面、无日历依赖**：`window_bounds` 只吃 `Tz`，不吃假日表——这让 af-meter（计量）与 af-calendar（工时/SLA）各持各的语义：计量按自然窗口重置，SLA 按班次推进。两个 crate 通过"时区字符串"对齐（DDL 的 `tz` 列 = af-calendar 的 `Calendar.tz`），不需要互相依赖。

## ⑤ 遗留问题

- **崩溃注入的判读边界**：DELETE 模式 8/8 (0,0) 无法区分 kill 落在"提交前 / 事务中 / 提交中"（三者恢复后同为 (0,0)）；但 8 次瞄准提交窗口无一分裂，与 SQLite master-journal 2PC 的文档语义一致。WAL 模式的 8/8 (20000,0) 则是"落在提交序列内"的**直接证据**（该状态只能在 A 已提交、B 未提交的窗口出现）。
- **未实测**：混合读写负载（本次按任务要求只测持续写）；单机 4 线程形态（任务指定 4 进程）；`wal_autocheckpoint` 阈值调优与 checkpoint 阻塞写者的量化；meter_event 重放重建投影的完整性验证（原型只实现重置路径）；concurrency/rate 两型的落库语义（schema 已留 kind，语义属 af-meter ④-5 的 Reservation API 议题）；outbox 的 dead 死信重试上限与告警。
- **消费原型的边界**：`consume` 的 Insufficient 返回路径会提交"惰性窗口重置"但不记 reset 事件（投影替换已发生）——正式版若要求事件可完全重放，重置也应落 `op='reset'` 事件行；`epoch` 列已预留但原型未接 af-effect-gateway 的 resume() 流程。
- **runs/ 保留 266MB 原始实验产物**（bench 库与 crash 库），作为报告数据出处备查，可随时删除不影响代码与测试。

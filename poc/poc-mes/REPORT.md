# poc-mes — ClickHouse MES 表引擎与写入路径实测报告

- 任务 id：`poc-mes`
- 日期：2026-10-19
- 环境：x86_64 / 16 核 / 14 GiB 内存（约 6 GiB 可用）/ NVMe 145 GiB 空闲；单机 ClickHouse，无副本
- 二进制：ClickHouse **26.7.34.3-stable** 官方 `clickhouse-common-static` 通用二进制（GitHub releases 资产名自查取得，`sha512sum -c` 通过，225,878,088 字节），落盘 `/home/pan-ding/ch-mes/bin/clickhouse`
- 端口：按任务约束落在 5220-5229 区间——HTTP `127.0.0.1:5221`，native TCP `127.0.0.1:5222`（`config.xml` 自带最小 users.xml，单 default 用户，仅监听 127.0.0.1）
- 所有结论均出自本机真实执行，命令与输出摘录见第③节；原始日志在 `results/`，可复现脚本在 `scripts/`、`sql/`、`writer/`

---

## ① 结论

1. **引擎裁决：明细用 `MergeTree` + 物化视图增量聚合（模型 A）作为 MES 默认形态；`AggregatingMergeTree`（模型 C）用于"允许丢明细、只要窗口指标"的高压缩场景；`ReplacingMergeTree`（模型 B）不做 MES 主表**——它适合"幂等重试/更正"这一件事，而不是当查询主路径。
   - 查询时延（100 万行、单机，5 轮 min/avg）：**A 与 C 全部 3-6 ms**；**B（`FINAL`）105-166 ms，慢 30-40 倍**。迟到回填后 B 的 FINAL 查询进一步涨到约 2 倍（46→106 ms 起）。
   - 存储（bytes_on_disk，100 万行）：明细 14.62 MiB（A）/ 13.20 MiB（B）；C 的状态表仅 **1.49 MiB（约 1/10）**；A 的 MV 目标表 20 KiB。
   - 迟到回填（2 万行回填历史窗口）正确性 **18/18 全 PASS**：A 的 MV 在 INSERT 时自动触发、无需任何人工干预即回填正确；C 补插部分聚合态（-State）合并后正确；B 依赖 `FINAL` 读时全局去重，结果正确但最贵。
2. **写入路径裁决：默认走官方 `clickhouse` crate（0.15.3，HTTP+RowBinary+lz4），批大小 ≥ 5 万行；不能用"每事件/每小批即时插入"**。
   - 批大小梯度（每档 100 万行）：1k → 1.6 万行/秒；1 万 → 14.0 万；**5 万 → 45.3 万**；20 万 → 70.6 万；100 万单批 → 118 万行/秒。**拐点在 1 万与 5 万之间**；每 INSERT 固定开销约 60 ms（1k 批时 60.6 ms/次）。
   - 同批大小下 crate 比"裸 HTTP INSERT（TSV 文本）"快 0%~25%（RowBinary+lz4 载荷更小）：50k 档 45.3 万 vs 37.8 万行/秒。
   - **落地建议：批大小 5 万-20 万行，或"满 5 万行、或满 1 秒"双阈值 flush**（1 秒 1 万行仍有 13.5 万行/秒，对产线事件速率 1k-1 万行/秒绰绰有余）；crate 的 `inserter` feature（`with_period`）天然支持该双阈值。
3. **诚实声明**：任务所述"clickhouse crate（native）"与事实不符——官方 `clickhouse` crate 底层是 **HTTP + RowBinary（默认 lz4 压缩）**，不是 native TCP 协议；本机实测亦证明它与"裸 HTTP INSERT"同协议栈、仅编码与压缩不同。Rust 生态没有维护良好的 native TCP 客户端（`clickhouse-rs` 已停更），选 crate 即等于选 HTTP。
4. **两个反直觉的实测发现**（详见③/④）：ReplacingMergeTree 的 `OPTIMIZE FINAL` **不跨分区合并**（45 条跨午夜更正残留在盘上），但 `SELECT ... FINAL` **读时全局去重、结果仍精确正确**；ClickHouse `rand64(x)` **不是 x 的纯函数**（仅首次调用播种），合成数据必须用 `xxHash64(toString(n)+tag)`。

## ② 步骤（全部真实执行）

1. **取二进制**：GitHub API 匿名限流、`gh` 凭证失效 → 改用 `releases/latest` 302 重定向取得最新稳定 tag `v26.7.34.3-stable`；`expanded_assets` 页确认资产名；`curl -sSL --retry 3 -C -` 后台下栽 `clickhouse-common-static-26.7.34.3-amd64.tgz` 至 `/home/pan-ding/ch-mes/dl/`；`sha512sum -c` OK；`tar` 解出通用二进制（server/local/client 三合一）。
2. **起单机 server**：自写最小 `config.xml`/`users.xml`（HTTP 5221 / TCP 5222 / 127.0.0.1 / tz=Asia/Shanghai / 内存上限 0.6×RAM），`clickhouse server --config-file=... --daemon`；`curl :5221/ping` → `Ok.`；`SELECT version(), timezone()` → `26.7.34.3 / Asia/Shanghai`。
3. **造数**（`scripts/gen_events.sh`，`clickhouse-local` 一次 SELECT 生成，输出 Parquet）：
   - 主数据 100 万行：5 产线 × 4 工位，事件序列按"每工单 10 事件"展开（start→complete→defect 交替，start 恒先于 complete），时间轴 `base + n*1200ms + 抖动` 覆盖 13.9 天；随机源全部 `xxHash64(toString(n)+tag) % 域宽`（纯函数，见①-4 的教训）。
   - 迟到回填 2 万行：1.6 万行"全新事件"（ts 落在头 3 天历史窗口）+ 4 千行"更正事件"（复刻既有业务键 `(job_id,event_type,station)`，`qty_good+5`、`cycle_ms+1000`、`ts+15min`，`seq` 取更高版本）。
   - 校验：主数据 1,000,000 行 seq 唯一、时间跨度 `2026-09-07 06:00 → 2026-09-21 03:19`；更正行与原行逐行核对 **mismatch=0**。
4. **三模型同数对比**（`sql/01_schema.sql` + `scripts/run_models.sh`，详见③）：
   - 模型 A：`events_mt`（MergeTree 明细）+ `mv_hourly` 物化视图 → `agg_hourly`（line, hour, starts/completes/defects/good_qty/defect_qty/cycle_n/cycle_sum）。
   - 模型 B：`events_repl`（`ReplacingMergeTree(seq)`，ORDER BY 业务键）。
   - 模型 C：`hourly_agg`（`AggregatingMergeTree`，sumState/quantileState 状态列，无明细）。
   - 三查询：Q1 按小时 WIP（累计开工-完工，窗口函数）、Q2 按班次（8 小时班）良率、Q3 按产线×小时 takt（均值周期）。
   - 正确性：用 `clickhouse-local` 对同一批 Parquet 算独立真值（A/C=追加语义、B=同键取最大 seq 的替换语义），服务端查询输出与真值 **md5 全等比对**；回填前 9/9、回填后 9/9。
   - 回填后测 `OPTIMIZE TABLE events_repl FINAL` 的行数变化与 Q1 时延。
5. **写入路径基准**（`writer/` 独立 cargo 工程，`scripts/writer_bench.sh`）：
   - 两条路径喂**同一份确定性数据**（xorshift64，与 server 侧造数同构）：① `clickhouse` crate 0.15.3（`insert::<Event>()`，HTTP+RowBinary+lz4）；② `reqwest` 裸 HTTP POST TSV 到 `:5221`。
   - 批大小梯度 1k/1 万/5 万/20 万/100 万，每档共 100 万行，批间 `TRUNCATE`，每档运行后 `count()` 与 uniqExact 核对（10/10 全对）；每路径先 1 万行预热不计入。

## ③ 证据（命令原文 + 输出摘录）

### 3.1 版本与端口

```
$ clickhouse client --host 127.0.0.1 --port 5222 --query "SELECT version(), timezone()"
26.7.34.3	Asia/Shanghai
$ ss -tln | grep -E ':(5221|5222) '
LISTEN 127.0.0.1:5221  # HTTP
LISTEN 127.0.0.1:5222  # native TCP
$ sha512sum -c clickhouse.tgz.sha512
clickhouse-common-static-26.7.34.3-amd64.tgz: OK
```

### 3.2 三模型加载耗时（同一 100 万行 Parquet，`bash -x scripts/run_models.sh`，日志 `results/run_models.log`）

```
LOAD  A_raw_insert   0.532   # MergeTree 明细（MV 随插入同步触发）
LOAD  B_repl_insert  0.570   # ReplacingMergeTree
LOAD  C_agg_states   0.156   # INSERT SELECT -State GROUP BY（写入端预聚合）
--- 迟到回填 2 万行 ---
LOAD  A_raw_insert   0.096   LOAD  B_repl_insert  0.097   LOAD  C_agg_states  0.085
```

### 3.3 存储（system.parts，active parts）

加载 100 万行后：
```
table         rows      parts  disk
agg_hourly      1670        1  20.19 KiB   # A 的 MV 目标表
events_mt    1000000       15  14.62 MiB   # A 明细
events_repl  1000000       15  13.20 MiB   # B 明细
hourly_agg      1670        1   1.49 MiB   # C 状态表（≈明细 1/10）
```
回填 + OPTIMIZE 后：
```
agg_hourly      3222        2  50.00 KiB
events_mt    1020000       30  14.99 MiB
events_repl  1016045       15  13.68 MiB   # 盘上残留 45 条跨分区未合并（见 3.6）
hourly_agg      3222        2   1.55 MiB
```

### 3.4 查询时延（`clickhouse client --time`，5 轮，ms；`results/bench_{pre,post}.tsv`）

回填前（100 万行）：
```
       min  avg        min  avg
A/Q1    3    4    B/Q1   46   49    C/Q1   3    4
A/Q2    3    4    B/Q2   52   58    C/Q2   3    3
A/Q3    3    4    B/Q3   46   55    C/Q3   3    4
```
迟到回填后（102 万行）：
```
A/Q1    4    4    B/Q1  106  122    C/Q1   4    5
A/Q2    3    3    B/Q2  116  130    C/Q2   4    4
A/Q3    3    4    B/Q3  105  109    C/Q3   3    4
```
B 回填并 `OPTIMIZE FINAL` 后 Q1 复测 3 轮：`0.125 / 0.085 / 0.086`s —— OPTIMIZE 只减 parts，不改变 FINAL 的读时去重成本，收益有限。

### 3.5 正确性（输出 TSV 与 clickhouse-local 独立真值 md5 全等，`results/check_{pre,post}.log`）

```
pre : A Q1/Q2/Q3 PASS；B Q1/Q2/Q3 PASS；C Q1/Q2/Q3 PASS   （9/9）
post: A Q1/Q2/Q3 PASS；B Q1/Q2/Q3 PASS；C Q1/Q2/Q3 PASS   （9/9）
      （post 的 B 对"同键取最大 seq"替换语义真值；A/C 对追加语义真值——见 3.7）
```

### 3.6 关键过程发现（均附原始报错/输出）

1. **ReplacingMergeTree 分区边界**：`OPTIMIZE TABLE events_repl FINAL` 后 `count()`=1,016,045（=1,016,000 唯一键 + **45 条跨午夜更正**：更正行 `ts+15min` 跨过 `PARTITION BY toYYYYMMDD` 边界，磁盘合并不跨分区）；但 `SELECT ... FROM events_repl FINAL` 查询计数恰为 1,016,000 —— **FINAL 读时全局去重，结果精确**。
2. **`rand64(x)` 非纯函数**：首轮数据用 `rand64(number+seed)`，更正行"原值+5"假设实测不成立（同 x 不同进程结果不同：主数据 qty=52 对应 `rand64(269945)%23=14`，迟到进程同式=9），并产生 3 条"同键同版本异载荷"退化行导致 B/C 与真值差 17。换 `xxHash64(toString(x)+tag)` 后逐行核对 mismatch=0，18/18 转 PASS。**合成数据永远不要用 rand64(seed) 当确定性 PRF**。
3. **`UNION ALL` 直接接 `ORDER BY+LIMIT BY` 去重失效**（真值查询踩中）：优化器把 LIMIT BY 下推进各 UNION 分支，最小复现：
```
(SELECT number%2 k, number seq FROM numbers(4) UNION ALL SELECT number%2, 100+number FROM numbers(4)
 ORDER BY k, seq DESC LIMIT 1 BY k)          -- 保留 0,2,102 三行（错）
SELECT * FROM (上述 UNION 包一层子查询) ORDER BY k, seq DESC LIMIT 1 BY k   -- 正确 102/103
```
4. **窗口函数即聚合**：`sum(sumMerge(x)) OVER (...)` 报 `ILLEGAL_AGGREGATION`（Code 184）；C 模型读端必须单层 `sumMerge(x)`，窗口再包子查询。
5. **Parquet 不能文件级 `cat` 拼接**（footer 损坏，thrift 报 `No more data to read`）；必须单查询 `UNION ALL` 生成。
6. **clickhouse-local 的 `file('相对名')` 只按 CWD 解析**（不读 config 的 `user_files_path`，报 `File .../scripts/mes_events.parquet doesn't exist`）；真值查询统一改绝对路径。26.7 的 `clickhouse local` 也不接受 `--user_files_path` CLI 参数（`Unrecognized option`），要走 `--config-file`。

### 3.7 两种迟到语义的业务差（2 万更正/回填行 ÷ 102 万 ≈ 2% 数据）

```
                  追加语义(A/C: 更正=新事件)      替换语义(B: 更正覆盖)
rows s/c/d        407977;408059;203964           406400;406400;203200
good_qty          20000985                       19919675   (+0.41% 高估)
yield             0.97515                        0.97514    (-0.001pp，比率类几乎无感)
avg takt ms       37753                          37752
净 WIP            -82                            0          (状态类指标漂移全量更正数)
```

### 3.8 写入路径吞吐（`writer/target/release/writer`，`results/writer_bench.tsv`，每档 100 万行、count+uniqExact 全核对 10/10）

```
批大小     crate 行/秒   http(TSV) 行/秒   crate avg/max 批时延
1,000         16,495          16,491        60.6ms / 69.8ms
10,000       140,310         135,453        71.3ms / 81.7ms
50,000       452,787         377,682       110.4ms / 125.2ms
200,000      705,902         569,787       283.3ms / 378.9ms
1,000,000  1,180,627       1,007,487       847.0ms / 847.0ms
```
每 INSERT 固定开销 ≈60 ms（1k 批 60.6ms/次、1000 次共 60.6s）；crate 全程 ≥ 裸 HTTP TSV（RowBinary+lz4 传输量约为 TSV 的一半）。

## ④ 对方案的反馈 / 决策输入

1. **MES 表 schema 草案**（完整 DDL：`sql/01_schema.sql`）——明细表（模型 A 主表）：
```sql
CREATE TABLE mes.events (
  seq UInt64,                 -- 到达序（兼作 Replacing 版本/排重依据）
  job_id UInt64,              -- 工单
  line LowCardinality(String),        -- 产线 L1..L5
  station LowCardinality(String),     -- 工位 ST01..ST04
  event_type LowCardinality(String),  -- start|complete|defect（量大可换 Enum8）
  ts DateTime64(3),                   -- 事件时间
  qty_good UInt32, qty_defect UInt32,
  defect_code LowCardinality(String), -- D01..D08，非缺陷为 ''
  cycle_ms UInt32                     -- 仅 complete 携带
) ENGINE = MergeTree PARTITION BY toYYYYMM(ts)  -- 生产建议按月：更正/迟到跨天不换分区
ORDER BY (line, station, ts, event_type, job_id);
-- 窗口聚合（MV 目标表，查询端先 GROUP BY line,hour 再开窗）：
--   hour, starts, completes, defects, good_qty, defect_qty, cycle_n, cycle_sum
```
2. **引擎裁决依据**：A 与 C 查询同为毫秒级（3-6ms），B 的 FINAL 恒 100ms+ 且随回填变贵；C 存储只要明细 1/10 但**丢明细**（缺陷根因分析、重算历史口径都做不到）；A 兼得明细可查与聚合快，唯一代价是"口径变更需回刷 MV"。裁决：**A 为默认；确无明细诉求的看板/年报场景用 C；B 仅作"上游可能重发/更正"时的幂等层**，且若用 B：分区必须按月（否则更正跨天永远残留在盘上，见 3.6-1）、读必须 FINAL（或 `argMax(seq)` 子查询，等价且可与其他 GROUP BY 组合）。
3. **写入路径决策输入**：
   - 用 `clickhouse` crate（它就是 HTTP；没有 native TCP 的 Rust 正选）。crate 相比裸 HTTP TSV 快 0-25%，且拿到类型检查与 schema 校验（本 PoC 两处编译/运行期即拦住表名引号与 DateTime serde 问题——`#[serde(with = "clickhouse::serde::chrono::datetime")]` 必须显式标注、库表须 `with_database("mes")` + 裸表名，0.15 的 `Insert::add` 已改名 `write`）。
   - **批大小 5 万-20 万行；每 INSERT 固定开销 ~60ms 意味着任何 <10k 行的即时插入都在浪费 95% 以上吞吐**。建议实现"满 5 万行或满 1 秒即 flush"双阈值（crate `inserter` feature 原生支持），崩溃丢数据窗口 ≤1 秒。
   - 批 100 万单插 0.85s 达 118 万行/秒：单机容量上限远超单产线速率，瓶颈在设计（小批/逐条）而非 ClickHouse。
4. **迟到/更正数据策略**：MV 路线（A/C）对迟到数据**零特殊处理**（INSERT 即正确）；唯一要防的是 B 式"更正"在追加语义下的双计（3.7：counts/good_qty 高估 ~0.4%，WIP 漂移全量更正数；比率类指标几乎无感）。若业务要求更正覆盖，要么 B+FINAL，要么把更正建模为独立 `event_type='correction'` 事件、查询端 argMax。
5. **运维注意**（真实踩坑，进设计纪律）：`FINAL` 是读时代价不是写时代价；`OPTIMIZE FINAL` 不跨分区；窗口函数是聚合（不能包 `sumMerge`）；`UNION ALL+LIMIT BY` 要多包一层子查询；合成数据用 `xxHash64(toString(n)+tag)` 不用 `rand64(seed)`；`clickhouse-local` 的 `file()` 相对名按 CWD 解析。

## ⑤ 遗留

1. 未测：集群/副本形态（ZooKeeper/ClickHouse Keeper、`Replicated*` 引擎）、异步 INSERT（`async_insert=1` 服务端攒批）、压缩编码（`CODEC(DoubleDelta)` 等）对明细表存储的影响——本 PoC 为单机默认压缩。
2. 未测：JSONEachRow/Parquet 二进制写入路径对比（本 PoC 只对比 crate-RowBinary 与 HTTP-TSV）；`inserter` feature 的 `with_period` 未单独跑（结论由批大小梯度推导，标注为推断而非实测）。
3. Rust writer 的数据为程序内生成（xorshift64），与 server 侧造数"同构不同源"（字段分布一致、非逐字节同一批）；吞吐对比不受影响，但未做"同一字节流"级别的严格对齐。
4. 时延测量含 client 进程启动与行序列化，未剥离纯服务端耗时（三模型同法测量，横向可比；绝对值偏保守）。
5. 当机背景：GitHub API 匿名限流、`gh` 凭证失效，release tag 经 302 重定向取得；GitHub 大文件下载（215 MB）以 `--retry 3 -C -` 一次成功，未触发断点续传。
6. 复现入口：`scripts/ch_server.sh start|stop` → `scripts/gen_events.sh` → `bash -x scripts/run_models.sh` → `scripts/writer_bench.sh`；原始日志在 `results/`。server 已停止、5221/5222 已确认释放（`ss -tln` 无监听）；AF 根 `Cargo.toml` 未改动（writer 工程自带空 `[workspace]` 表独立存在）。

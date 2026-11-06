# RESULT — poc-otel：af-trace → OpenTelemetry 出口实测

日期：2026-10-19　任务目录：`af/poc/poc-otel/`

## ① 结论

**通过**（带一处环境性偏差，如实声明）。OTLP 全链路（Rust SDK → collector → file exporter）在本机实测打通并完成三组实验：

1. **协议对比**：gRPC 与 HTTP(protobuf) 裸导出器往返时延同量级（batch≥64 时摊销 **14–17 µs/span**，batch=1 单次往返 p50：gRPC 160 µs / HTTP 112 µs）；全链路洪峰两者都精确零丢失（20000/20000），e2e 吞吐 55–61k spans/s。**本机场景协议不是瓶颈，选哪个都不吃亏；Langfuse 直连只收 HTTP，协议选择应跟随后端。**
2. **批量参数**：稳态（50/s）下端到端时延 ≈ scheduled_delay 线性放大（100ms→p50 143ms，5000ms→p50 2586ms，p95 4848ms）；洪峰下吞吐由导出器串行能力决定（51–68k spans/s），delay/批量大小对吞吐不敏感，**batch=2048 因单批 2MB 反而最慢**。默认值（512/5s）适合吞吐、不适合观测新鲜度；建议 500ms–1s。
3. **collector 挂掉**：**不阻塞业务线程**（burst 4000 条 6–9ms 发完，队列吸收）、**有界缓存**（max_queue_size）、**无持久缓存**——导出失败重试 3 次（100ms 起、1.6s 封顶）后整批丢弃；10s 停机窗口后恢复，4000 条 burst 全部丢失、500 条 baseline 完好。collector 持续下线时 `shutdown()` 阻塞 5s 返回 `Err(Timeout(5s))`。**OTel 出口是有损组件：af-trace 的 critical 语义不能托付给它。**

偏差声明：GitHub release 下载不可达（`github.com` 资产域名连接超时，多次重试失败），collector 改用 **ocb v0.163.0 + goproxy.cn 从上游源码构建**（组件：otlpreceiver/batchprocessor/fileexporter/debugexporter v0.163.0），功能等价于 otelcol-contrib 的相关子集，非官方发布二进制。

另获两个对方案关键的实测事实：**(a) OTel Rust SDK 的 `on_end` 无返回值、BSP 队列满静默丢弃——fail-closed 无法建在 SDK 层**（源码证据见③）；**(b) file exporter 启动即截断目标文件（17B→0B 实证）——collector 重启会抹掉已落盘数据**。

## ② 步骤

```
af/poc/poc-otel/
├── bin/                    # （空）GitHub release 下载失败，二进制改放 collector/build/
├── collector/
│   ├── builder-config.yaml # ocb 配置：otlpreceiver + batchprocessor + fileexporter + debugexporter
│   ├── otelcol-config.yaml # 运行配置模板（__OUT__ 占位）：4317(gRPC)/4318(HTTP) → file, flush 100ms
│   └── build/otelcol       # 50MB 自编译 collector（v0.163.0）
├── otel-probe/             # 独立 cargo 工程（含空 [workspace]，opentelemetry 0.33.1）
│   └── src/main.rs         # 子命令：raw / pipe / down
├── run/                    # 实验驱动脚本 + 每 run 的 collector 配置/日志/结果
│   ├── run-experiments.sh  # E1/E2/E3 全矩阵
│   ├── run-e2.sh           # E2 修正版（每 run 独立 collector）
│   └── experiments.log     # 原始运行日志
└── out/                    # 每 run 的结果 JSON 与 collector span 落盘文件（证据留存）
```

执行序列：

1. **collector**：`curl -L` 下载 otelcol-contrib v0.163.1 GitHub release → 连接超时失败（`curl: (28) Connection timed out`）→ 改 `go install go.opentelemetry.io/collector/cmd/builder@v0.163.0`（`GOPROXY=https://goproxy.cn,direct`，Go 临时目录指到 `/home/pan-ding/gotmp`，/tmp tmpfs 配额不足）→ `builder --config=builder-config.yaml` 生成源码 → `go build -o otelcol .` → `./otelcol components` 确认组件 → 起服监听 4317/4318（`ss` 确认）。
2. **otel-probe**：`cargo new`（自动挂进 AF workspace members，已在根 `Cargo.toml` 还原为 `["crates/*"]`）；依赖 `opentelemetry/opentelemetry-sdk/opentelemetry-otlp 0.33.1`（API 以本地 registry 源码核对：`SpanExporter::builder().with_tonic()/.with_http()`、`BatchConfigBuilder`、HTTP 编程式 endpoint 不自动追加 `/v1/traces`、默认重试 `RetryPolicy::recommended()`=3 次、超时默认 10s）；`cargo build -j4 --release`。HTTP 导出在 tokio worker 上调用 `reqwest::blocking` 会自旋卡死，raw 子命令改为独立 std::thread + `futures-executor::block_on` 驱动（pipe 模式走 BSP 专用线程，无此问题）。
3. **冒烟**：gRPC 2000 spans 全到达（file 4 行 JSON，每行一个导出批次；`af.level` 三级与 `af.gap_counter` 属性均在 OTLP JSON 中可见）。
4. **E1/E2/E3**：`run/run-experiments.sh` + 修正版 `run-e2.sh`（E2 每 run 独立 collector，避免共享文件与 collector 截断互相污染）。
5. **Langfuse 调研**：官方文档 `langfuse.com/integrations/native/opentelemetry` 抓取核对（见④）。

## ③ 证据

### 环境

```
$ ./otelcol components   # 自编译
receivers:  [otlp]  go.opentelemetry.io/collector/receiver/otlpreceiver v0.163.0
processors: [batch] go.opentelemetry.io/collector/processor/batchprocessor v0.163.0
exporters:  [file]  github.com/open-telemetry/opentelemetry-collector-contrib/exporter/fileexporter v0.163.0
            [debug] go.opentelemetry.io/collector/exporter/debugexporter v0.163.0
$ ss -tlnp | grep -E '4317|4318'
LISTEN 127.0.0.1:4317 users:(("otelcol",pid=...,fd=7))
LISTEN 127.0.0.1:4318 users:(("otelcol",pid=...,fd=8))
```

### E1：协议对比（payload=1024B/条）

裸导出器往返（`otel-probe raw`，逐次 await，测单次 RPC 往返；grpc=4317，http=4318 `/v1/traces`）：

```
transport batch  spans  err thru(spans/s)  call µs: p50    p95    avg
grpc      1      2000   0   4283             160     318    231
grpc      64     40000  0   58824            929     1818   1047   (≈14.6 µs/span)
grpc      512    40000  0   59172            7316    14317  8177   (≈16.0 µs/span)
grpc      2048   40000  0   57307            34245   48529  33372  (≈16.3 µs/span)
http      1      2000   0   6601             112     232    149
http      64     40000  0   58055            892     1888   1055   (≈16.5 µs/span)
http      512    40000  0   64725            7552    9837   7446   (≈16.1 µs/span)
http      2048   40000  0   62402            30276   40690  30669  (≈15.0 µs/span)
```

全链路（`otel-probe pipe`，20000 spans，BSP 512/200ms/queue65536，collector file 计数独立核验）：

```
pipe grpc: arrived=20000/20000 send=43ms shutdown=320ms e2e_thru=55096/s  | collector_file_spans: 20000
pipe http: arrived=20000/20000 send=42ms shutdown=285ms e2e_thru=60976/s  | collector_file_spans: 20000
```

### E2：批量间隔/大小（gRPC；E2a=20000 spans 最快速率，E2b=1000 spans @50/s 稳态测时延）

```
E2a 吞吐（洪峰）                        E2b 端到端时延（稳态 50/s）
delay  batch  arrived   wall   thru     delay   arrived  p50    p95    max (ms)
100ms  512    20000/20000 296ms 67568/s   100ms   1000/1000 143    190    203
500ms  512    20000/20000 316ms 63291/s   500ms   1000/1000 298    547    605
1000ms 512    20000/20000 305ms 65574/s   1000ms  1000/1000 596    1047   1099
5000ms 512    20000/20000 301ms 66445/s   5000ms  1000/1000 2586   4848   5094
500ms  64     20000/20000 331ms 60423/s   http 500ms 1000/1000 338  564    590
500ms  2048   20000/20000 390ms 51282/s
（12 个 run 全部零丢失；时延≈delay 线性，下限≈100ms 为 file exporter flush_interval+采集粒度）
```

### E3：collector 挂掉（baseline 500 在线 → 杀 collector → burst 4000 → outage 10s → 重启 → shutdown；BSP 512/200ms/queue8192）

```
E3 recover grpc:  after_baseline=500 just_after_burst=500 burst_send_ms=6
                  shutdown_ms=0(ok) final_arrived=500 lost=4000
E3 recover http:  同上（burst_send_ms=9，final 500/lost 4000）
E3 shutdown_down grpc: shutdown_ms=5000 shutdown_err="Operation failed: Shutdown errors: [Timeout(5s)]" final=500 lost=4000
E3 shutdown_down http: 同上
```

解释链：burst 被 BSP 队列（8192）吸收→调度 delay 200ms 触发导出→连接拒绝→重试 3 次（`RetryPolicy::recommended`：100ms 起/1.6s 封顶）→整批弃→10s 窗口内 4000 条燃尽→恢复后无可恢复。**无磁盘/跨进程缓存。**

### 关键行为实证（非推测）

```
# file exporter 启动截断（重启丢已落盘数据）
$ echo "PREEXISTING LINE" > out/trunc-test.json   # 17B
$ (otelcol --config run/trunc.yaml &) ; sleep 2
size_before_collector=17 size_after_collector=0

# OTel SDK 无 fail-closed（源码，~/.cargo/registry/.../opentelemetry_sdk-0.33.1）
src/trace/span_processor.rs:202  fn on_end(&self, span: SpanData);          # 无返回值，错误不回传
src/trace/span_processor.rs:818  let result = self.span_sender.try_send(span);
src/trace/span_processor.rs:862  Err(TrySendError::Full(_)) => ...          # 队列满：丢弃+内部计数

# 重试默认值（opentelemetry-otlp-0.33.1/src/retry_policy.rs:36）
recommended(): max_retries=3, initial=100ms, max_delay=1600ms
# 导出超时默认（exporter/mod.rs:49）
OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT = 10s
```

### Langfuse OTLP 兼容性（文档级调研，未真连；来源见④）

| 项 | 事实 |
|---|---|
| 端点 | `POST {base}/api/public/otel`（自建域名同路径）；signal 分离配置时 `/api/public/otel/v1/traces` |
| 最低版本 | 自部署需 **Langfuse ≥ v3.22.0**（OTLP 端点引入版本） |
| 协议 | **OTLP/HTTP**：`HTTP/JSON` 与 `HTTP/protobuf` 均可；**gRPC 不支持**（文档明示，另有 issue #11366 佐证） |
| 认证 | HTTP Basic：`Authorization: Basic base64(公钥pk-lf-…:私钥sk-lf-…)`；SDK 侧经 `OTEL_EXPORTER_OTLP_HEADERS`，collector 侧经 otlphttp exporter `headers` |
| 必须 header | `x-langfuse-ingestion-version: 4`：缺省时 v4 数据模型实时摄入不生效（延迟可达 10 分钟） |
| 语义约定 | `langfuse.observation.level` ∈ `DEBUG\|DEFAULT\|WARNING\|ERROR`（缺省从 `span.status.code` 推断，默认 DEFAULT）；`langfuse.observation.type` ∈ `span\|generation\|event\|embedding\|agent\|tool\|chain\|retriever\|guardrail\|evaluator`（未识别值不报错，fallback 最终落到 `span`）；`langfuse.trace.metadata.*`/`langfuse.observation.metadata.*` 进结构化 metadata，其余属性进 `metadata.attributes` catch-all；resource 属性进 `metadata.resourceAttributes`；缺失时间戳会被补齐（零时长），建议每 span 都带 start/end |
| 限制 | 不收 OTLP metrics/logs 信号（只 traces）；span 过滤有风险（需 root span 才能正确重建 trace） |

## ④ 对方案的反馈/决策输入

### 1. 架构定位：af-trace 必须保留在 OTel 上游，OTel 只做出口

实测+源码双重确认 OTel SDK 是**有损**组件：BSP 队列满静默丢弃且错误不回传（`on_end` 无返回值）；exporter 重试 3 次后弃批；collector 挂掉 10s 即实测丢 4000/4000。af-trace 的 critical fail-closed / gap 可验证不变量**不能托付给 OTel 层**。建议拓扑：

```
业务 → af-trace(critical fail-closed / normal 有损计数) → [adapter 消费 af-trace 已保证的事件] → OTel SDK(BSP) → collector → Langfuse
                                                          ↑ critical 事件从 af-trace sink 尾随读取(tail)，
                                                            保证只有已被 af-trace 保证的事件才进 OTel
```

这样 OTel 链路任何丢失只降级"可见性"，不违反 af-trace 的零丢失/计数一致不变量；gap 计数即使 OTel 全丢，af-trace 本地 sink 仍有对账事实。

### 2. 三级事件 + gap_counter 的映射建议

**主载体：每条 af-trace 事件 = 一个 OTel span**（name=`af.trace.event`，kind=INTERNAL），属性 `af.seq/af.level/af.kind/af.ts_ms/payload`。三种候选映射取舍：

| | span（推荐） | span-event | log record |
|---|---|---|---|
| 与 af-trace 现状 | gap 本就是内部"critical span"，外化零转换 | 需引入长命 carrier span | 需换信号管线 |
| 时延 | BSP 直批导出，E2 实测≈delay | 受 carrier span 生命周期拖累，critical 可见性变差 | 独立 log 管线，同级 |
| 截断风险 | 无（span 是顶级单位） | **有**：SDK 默认每 span 128 events，超限静默丢——洪峰时 gap 事件本身会被截断，等于用可丢机制报告丢失 | 无 |
| Langfuse 兼容 | 直接收（traces 信号） | 收（作为 observation type=EVENT） | **不收**（需 collector 桥接） |
| 语义 | "事件不是有时长的操作"，轻微语义溢价；后端时长图表无意义 | 分组自然但上限致命 | 严重级原生，但脱离 trace 生态 |

**gap 表达（保住"缺了多少"的可验证性）——四条硬建议**：

1. gap 永远是**显式记录**，绝不能只体现为"数据缺席"或只进 metric：span 属性 `af.gap.count`（本条覆盖丢弃数）、`af.gap.first_seq`/`af.gap.last_seq`（闭区间）、`af.gap.total_dropped`（累计）——即把 af-trace 的 `GapInfo` 原样外化。
2. 保持不变量并写进消费端验收：`Σ(af.gap.count) == gap_counter == 实际丢弃数`，seq 区间支持逐条对账（af-trace RESULT.md 的三不变量在 OTel 侧可机械复检）。
3. **配套一条 metric**（monotonic Counter `af.trace.gap.dropped`，按 level 分维）：告警/面板可直接 sum；但 metric 无 seq 区间，与 span 互补、不替代。注意 Langfuse 不收 OTLP metrics，这条 metric 只在有 collector 的部署里可用（或由 collector 周期性把累计值转成 summary span）。
4. **禁用 span-event 承载 gap**（截断风险，见上表）；洪峰时 af-trace 已验证的"合并 gap"（count>1）在 OTel 侧继续沿用，一条 gap span 带区间即可，天然压缩导出量。

**level → 后端语义字段对照**（Langfuse）：
- critical → `langfuse.observation.level=ERROR`（或 span `status.code=Error` 自动推断）；log 侧对应 SeverityNumber ERROR/FATAL。
- degraded → `WARNING`（或 DEFAULT，按业务口径）；log INFO/WARN。
- gap → `WARNING`；critical 通道也满时（合并 gap）建议升 `ERROR`。

### 3. 参数与部署决策输入

- **协议**：后端定协议。直连 Langfuse ⇒ 必须 OTLP/HTTP（`HTTP/protobuf` 与 `HTTP/JSON` 均可，首选 protobuf）；要 gRPC ⇒ 中间放 collector（官方文档的标准拓扑即如此）。本机两者性能等价（14–17µs/span），无性能理由偏好任一。
- **BSP 参数**：观测新鲜度优先 ⇒ `scheduled_delay=500ms–1s`（E2：p50≈delay，5s 会把 p95 拖到 4.8s）；洪峰吞吐对 delay/批量不敏感；**queue 取"洪峰速率 × 最长可容忍后端故障时长"**（本实验 4000 条/10s 即全弃；queue 只是秒级缓冲，不是存储）。
- **critical 复用同一 OTel 管线没问题**（af-trace 已在上游保证），但**不要**为 critical 单独用 SimpleSpanProcessor"同步导出"——它会把每次导出时延（~0.15–33ms）直接加在业务线程上，且错误同样不回传（`on_end` 无返回值），买不到 fail-closed。
- **collector 侧两个坑**（实测）：① file exporter 启动即截断文件——collector 重启=落盘数据清零，生产必须用持久化后端 exporter 或外部落盘方案，不能依赖 file exporter 的跨重启连续性；② collector 本身也是进程，E3 证明它挂掉期间数据不可恢复——collector 的高可用（或业务侧对 OTel 出口的降级预期）要进最终方案。
- **Rust 依赖坑**（接入时直接避开）：`opentelemetry-otlp` 0.33 的 HTTP 导出用 `reqwest-blocking-client` feature 时，**不能在 tokio worker/async 上下文直接调用**（本实验卡死实录），BSP 专用线程或独立 std::thread 驱动才安全；HTTP 编程式 endpoint 不会自动追加 `/v1/traces`，必须写全路径；gRPC 的 tonic 路径无此问题。

### 4. Langfuse 接入清单（文档级，供后续真连时直接用）

```
exporter 配置（collector otlphttp 或 SDK 直连）：
  endpoint: https://<langfuse-host>/api/public/otel        # SDK http 模式需补全为 .../otel/v1/traces
  headers:
    Authorization: Basic <base64(pk-lf-…:sk-lf-…)>
    x-langfuse-ingestion-version: "4"
  协议: OTLP/HTTP（protobuf 或 JSON）；gRPC 不可用
```

## ⑤ 遗留问题

1. **非官方发布二进制**：GitHub release 下载不可达，collector 为 ocb 自编译（v0.163.0，组件子集）。`bin/` 目录留空并在此声明；若后续网络可达，建议换官方 `otelcol-contrib_0.163.x_linux_amd64.tar.gz` 复测一遍（预期行为一致：OTLP/file 语义稳定）。
2. **E3 只测了"连接拒绝"型故障**（进程被杀，端口立即不可达）。"连接挂起"型（如网络分区、对端假死）行为不同：导出会等到 10s 超时而非快失败，队列燃尽更快，未实测。
3. **端到端时延含 ±(5ms 采集轮询 + ≤100ms file flush)** 的量化误差；绝对值在小数档（<100ms）不可作精读，趋势（≈delay 线性）可靠。
4. E2 首轮驱动脚本曾共用一个 collector 实例且 `--tail-file` 与 `--out` 同路径（已修正为每 run 独立 collector），`run/experiments.log` 中 E2a 的 `collector_file_spans: 0` 行是该 bug 的残留痕迹；以 `run/e2.log` 与 `out/e2*.json`（probe 内 tail 独立计数 + 全部零丢失）为准。
5. Langfuse 为文档级调研（官方文档明示事实），未真连验证 header/映射行为；`langfuse.observation.type` 的完整 fallback 链以官方文档为准，接入时应以小批量真实 span 验证一次。
6. 洪峰实验单进程单线程发送；多写者（多 af-trace 实例共享出口）下的队列竞争与公平性未测。
7. 环境 noise：bash 会话的 `pwd` 写 /tmp 偶发 `磁盘配额超时`（harness 每命令写 cwd 文件到 tmpfs），与本次实验数据无关；Go 构建已用 `TMPDIR=/home/pan-ding/gotmp` 规避。

---
实验原始数据：`run/experiments.log`、`run/e2.log`、`out/*.json`（每 run 结果 + collector 落盘文件）；probe 源码：`otel-probe/src/main.rs`；驱动脚本：`run/run-experiments.sh`、`run/run-e2.sh`。

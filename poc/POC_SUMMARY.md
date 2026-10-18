# POC_SUMMARY — 15 个 PoC 结果汇总

生成时间：2026-10-19。素材：`af/BOOTSTRAP.md` + `af/crates/*/RESULT.md`（11 份）+ `af/poc/*/RESULT.md`（4 份）。
核对结果：crates/ 下 11 个目录、poc/ 下 4 个目录，**15/15 均有 RESULT.md，无"缺报告"**。

---

## 一、状态表

| # | id | 名称 | 状态 | 关键发现（均出自各自 RESULT.md，测试为真实执行） |
|---|---|---|---|---|
| 1 | af-calendar | 通用日历与 SLA 计时 | ✅ 通过 | `cargo test -j4 -p af-calendar` 18/18 绿 + `--example sla_demo` 跑通。周五 16:00+4h→下周一 11:00、黄金周跨越、UTC 消费者拿回 UTC 全部验证。截单点=逆向派生值（非一等字段）好用。坑：交付点在班次外时正反互逆非恒等（需"≤due+最晚性"语义）；req=0 在正/逆向语义相反；假日表必须"假日+调休"二元（day_overrides 统一表达）；搜索视界必须显式（HorizonExceeded 防死循环）。遗留：夜班跨午夜、DST 未实测 |
| 2 | af-credential | 凭证签发与权限求交 | ✅ 通过 | `cargo test -j4 -p af-credential` 8/8 绿。biscuit-auth 6.0.0 + cedar-policy 4.14.0 **纯 Rust、全树 0 个 openssl/ring**，冷编译 61.8s。衰减 append-only：事实删不掉但行为封死（git 被拒且事实仍在）。结论：biscuit 6 无 schema、谓词是运行时字符串 DSL（词表必须收进内部常量，业务层只见类型化 API）；**预算真相源放 af-meter 账本，凭证只带 per-call cap**；biscuit 6 无 Float→预算统一 `*_cents: i64`（需与 af-meter 对齐） |
| 3 | af-effect-gateway | 副作用网关与幂等 | ✅ 通过 | `cargo test -j4 -p af-effect-gateway` 6/6 绿，连跑 10 轮无抖动。崩溃恢复重放（外呼恰好 1 次）、同键异载荷 PayloadConflict、窗口内撤回、旧纪元四操作全拒。rusqlite(bundled)。坑：撤回窗口是"发送闸"非"宽限期"（窗口未关必须拒发）；SQLite `RETURNING` 返回存储类型 TEXT，rusqlite 按 i64 取会炸；载荷指纹依赖 serde_json 默认按 key 排序（勿开 preserve_order） |
| 4 | af-graph | 图模板 schema 与编译期校验器 | ✅ 通过 | `cargo test -j4 -p af-graph` 16/16 绿 + `afg validate` CLI 8 fixtures（3 PASS、5 个非法各恰报 1 条对应规则违例）。关键裁定：**循环体子图豁免 DAG、map 体不豁免**（有原则，建议写进设计纪律）；kind 与 human/loop/map 构造正交，扩展零 schema 改动；应"先引用完整性、再判 DAG"否则报错条数随顺序抖动。遗留：工件名级引用完整性、Violation 无源位置、事件数估算器未做 |
| 5 | af-meter | 可组合计量器 | ✅ 通过 | `cargo test -j4 -p af-meter` 10 单元+5 集成全绿；"日窗耗尽但周窗有余→转 API、日窗重置切回订阅"时间线逐行验证。Min=约束合取（各扣一份）/Max=择优（只扣选中）**不对称必须写死进接口文档**；`Reading{available, recovers_at}` 两字段即够路由"用/等/换"。建议：钱从 f64 换整数最小单位；Concurrency（占坑）与数量计量（扣量）拆成两个操作；无持久化、单消费者假设 |
| 6 | af-release-gate | 投放闸与渐放（CONWIP） | ✅ 通过 | `cargo test -j4 -p af-release-gate` 14 单元+1 全天模拟全绿（262 件：max_wip=400=上限、551 次 WipCap 拒绝、零清零 tick、夜间比例 0.317≈0.3、canary 晋升）。结论：下限应是**预警线事件化**（NeedRefill/RefillSatisfied 边沿对）而非准入线；夜间限流必须与逐小时排班产能配套，否则结构性断粮；被限流件**不要原地重试**（烧信用），需 defer 语义；渐放要独立泊位通道否则被积压淹没 |
| 7 | af-sampling | 抽样注入与升迁引擎 | ✅ 通过 | `cargo test -j4 -p af-sampling` 23/23 绿（22 单元+1 端到端）。CSP-1 三不变式 1000 件逐件核验；金标告警数==放行缺陷金标数（25==25，全检对照 0）；端到端轨迹 Demoted/Promoted/Recovered/Hold 四类动作全部真实发生。结论：f_min 做成"构造校验+setter 钳制"结构性不变式；升迁要分 current/attained 双水位（否则连净回升成绕过判据的后门）；喂引擎的一致性率必须用**批次增量**（累计口径会卡死在 Agent，端到端初版真实踩中） |
| 8 | af-station | 人工任务账本 | ✅ 通过 | `cargo test -j4 -p af-station` 8/8 绿，含两线程抢任务恰好一赢、4 线程×40 任务零重复认领、租约过期接管；并发 3 用例 10 轮稳定。rusqlite bundled（libsqlite3-sys 0.30.1，C 编译产物确认）。坑：心跳必须带 `lease_until > now` 守卫（否则过期 worker 复活已判死租约）；`ON CONFLICT(idempotency_key)` 必须显式指定（裸 DO NOTHING 会对所有 UNIQUE 生效）；lease_until 用 unix 毫秒 INTEGER。WAL+busy_timeout 5s 下 SQLite 单文件扛账本并发足够 |
| 9 | af-stop | 失败指纹止损 | ✅ 通过 | `cargo test -j4 -p af-stop` 11 单元+1 doctest 全绿（零外部依赖 std only）。三规则+优先级钉死：check 首败 Stop > 指纹连击 Stop > 累计阈值 Escalate；不同工具交替错误不误触。`feed(event)->Decision` 单向拉取式好用且引擎不做停机锁存。坑："连续"的打断条件（成功/不同指纹/任何其他事件）必须显式约定；阈值语义="第 M 个当拍返回"；无 serde 边界（跨进程需补 derive） |
| 10 | af-trace | 分级背压与缺口计数 | ✅ 通过 | `cargo test -j4 -p af-trace` 4 单元+10 万事件洪峰集成全绿，5 连跑稳定。洪峰 99.1% 丢弃下：critical 全部落盘零丢失、`gap_counter==gap_reported==实际丢弃数` 精确一致，"缺了多少"成为落盘可验证事实。坑：gap span 挤占 critical 容量（挤掉 86 次真实 critical 写）→需预留子容量/独立 lane；逐条 gap 放大写入（9.9 万丢弃→1.8 万 span）→按时间/计数窗批量；`Ok(seq)⇒必在 sink` 不变量+事件退还机制建议保留 |
| 11 | af-workorder | 订单门控与决策工件 | ✅ 通过 | `cargo test -j4 -p af-workorder` 34 单元+3 集成全绿 + gate_demo 打印门控拒绝原因。门控错误=**类型化缺失清单** `PreconditionsNotMet{missing: Vec<Missing>}`；非法转换与门控不满足分两种错误（先查状态机边再查门控）；graph_version 不可变+变更单 spawn 后继单（supersedes）+原单封存。占位诚实声明：FNV-1a64 hash（须换 SHA-256/BLAKE3）、ed25519 占位、deadline 字符串比较（应接 af-calendar 的 DateTime<Utc>）。建议补 `check_close_readiness()` dry-run 只读接口 |
| 12 | poc-cas | 内容寻址存储（zot+crane+cosign） | ✅ 通过 | 真实执行：zot v2.1.22 起服 200；crane 推 v1/v2（真实 alpine layer），digest 不同且均拉回；cosign sign-blob/verify-blob `Verified OK`，篡改 1 字节 exit 1；GC 闭环。关键实测：**zot 是 OCI-only 写入**（docker manifest PUT→415）；crane push tarball 只认 docker-save→"pull 成 tar 再 push"路线走不通，用 `crane append --oci-empty-base`；**GC 可控触发=停服后 `zot verify-feature retention`**（无在线 REST API）；**cosign v3 离线验证必须 `--insecure-ignore-tlog`**（bundle 必填且 verify 会发起在线 TUF 刷新）；zot 写入校验只查 blob digest+size 存在性 |
| 13 | poc-cue | CUE 产线定义与配方 | ✅ 通过 | cue v0.17.1 三项验证全过：closed schema（含嵌套层）拒未知字段；L4 覆盖 L3（default 标记法）；十条配方规则+5 类反例全拒。**总回答：CUE 够用且是最优选**，但是"规则参数与参数间约束的 schema 语言，不是规则引擎"——数据相关谓词（时间窗/事件流）做不了，需另配 CEL（cel-interpreter 0.10.0，Rust 内嵌）。坑：v0.17 closed definition 不能 `&` 扩展须用 embedding；"L4 盖 L3"必须靠 `*30 | int & >0` default 标记（裸具体值=冲突报错非覆盖）；引用完整性可用 `or()` 析取静态校验（须内联）；运行时求值 7-17ms/次（进程路线低频可行）；Rust 无原生求值器（cue/cue-rs crate 是无关的乐谱库！） |
| 14 | poc-dspy | DSPy IR 结构验证 | ✅ 通过 | 36/36 断言通过（dspy 3.4.1 / Python 3.14.4，全程未调 LLM）。命题"四元组=schema 化数据+可枚举改写+JSON 存取"成立：7 个改写 API 全部不可变、返回新类型、可链式；dump_state→load_state 往返字节级等价。**边界建议：IR source of truth 与校验器放 Rust**——DSPy `delete` 缺失字段静默 no-op、存档不含字段名（仅 prefix/description，位置恢复）、load_state 按位置 zip 错位不报错。改写语义是纯字典操作，Rust 复刻无难点 |
| 15 | poc-temporal | Temporal 持久执行引擎 | ✅ 通过 | 全验收点真实执行：dev server 文件库 kill -9 →重启→workflow 同 run_id 自动续跑完成；signal+update 两种审批门；activity 执行 2 次扣款恰好 1 次（at-least-once+业务幂等键=exactly-once 实证）；CAN 后同 workflow id 查询仍路由最新 run、旧 run 历史可审计；**Rust SDK（temporalio-sdk 1.1.0）smoke 全往返 result=84，编译 1m42s**。Replay 纪律是真实成本：5 个坑中 4 个是"代码 vs 事件历史"（类型提示进历史→在飞 run 中毒只能 terminate） |

环境（BOOTSTRAP.md，本机实测）：rustc/cargo 1.99.1、temporal CLI 1.9.1（Server 1.32.0）、zot v2.1.22、cosign v3.1.3、cue v0.17.1、crane 0.22.1、python 3.14.4、temporalio 1.34.0、dspy 3.4.1；11 crate 空壳 `cargo build -j4` 全过；两个 venv 建成（cp314 wheel 齐全无兼容失败）。

---

## 二、发现

### 2.1 语言选型证据

- **Rust 主体成立，全部关键库纯 Rust 无 C/openssl 拖累**：af-credential 实测 biscuit-auth 6.0.0 + cedar-policy 4.14.0 全依赖树 `grep -c 'openssl|ring|rustls|native-tls'` = 0（签名走 curve25519-dalek），冷编译仅 61.8s（-j4）；rusqlite bundled（libsqlite3-sys 0.30.1）C 编译正常（af-station/af-effect-gateway 两处实证）；chrono/chrono-tz、serde 全家桶均顺畅。11 个 crate 合计约 170 个测试全绿，Rust 侧开发效率在本机无障碍。
- **Python 侧定位为"执行/优化引擎"，不做 source of truth**：poc-dspy 硬证据——DSPy 存档不含字段名、delete 静默 no-op、load_state 按位置恢复错位不报错；结论是 Rust 持 IR（含字段名）下发 JSON，Python 薄适配层执行，回传后 Rust 校验。dspy 3.4.1 在 Python 3.14.4 下 cp314 wheel 齐全（BOOTSTRAP.md），无兼容坑。
- **Temporal 的 Rust SDK 可用但成本明显高于 Python**：包名是 `temporalio-sdk` 1.1.0（`temporalio` 是空名 404）；**构建硬依赖系统 protoc**（缺它两次 build 失败）；宏展开直接引用 futures/temporalio_common/temporalio_workflow，使用方必须显式声明；worker future 非 Send、官方形态 worker/starter 分进程。Python SDK 装饰器齐全（signal/update/query/wait_condition/CAN），适合平台层与快速验证。
- **CUE 无 Rust 原生求值器**（见 2.3），但"部署期预编译成 JSON、运行时零 CUE 依赖"的主路线使这一短板不阻断 Rust 主体。

### 2.2 Temporal vs DBOS 证据

- poc-temporal ④ 给出直接裁决证据："需要长流程编排+人工审批+外部系统幂等+审计，**Temporal 语义收益 > 纪律成本**；仅是'函数级 crash 恢复'则 **DBOS 更省心**。"
- 支撑 Temporal 的实测：kill -9 server 后同 run_id 自动续跑；审批门（signal/update）与 child fan-out 真实跑通；外部账本 activity 重试 2 次扣款恰好 1 次；CAN 后 workflow id 不变、旧 run 事件历史 1..58 完整可审计。
- 支撑"纪律成本"的实测：5 个坑中 4 个是"代码 vs 事件历史"——activity 返回类型注解必须 `Any`；**schedule 时类型提示会进事件历史，热修改代码后在飞 run 直接中毒、只能 terminate**；改控制流即 replay 非确定性失败。结论：选 Temporal 就必须把"工作流代码当版本管资产"（patched()/versioning）写进最终方案。
- **诚实声明：DBOS 本身未实测**，该对比是从 Temporal 实测成本侧论证的。

### 2.3 CUE 是否够用（poc-cue 总回答）

- **够用且是最优选，但要认清它是什么**：CUE 是"规则参数与参数间约束的 schema 语言，不是规则引擎"。closed schema、默认/覆盖合并（L3/L4）、引用完整性静态校验（`or()` 析取把 map 键集转成析取，悬空引用 vet 期拒绝）三项全部实测通过，引用完整性甚至超出预期。
- **三件必需配套**：① 运行时求值走"部署期 `cue vet` + `cue export` 预编译 JSON"主路线（运行时零 CUE 依赖），或接受进程路线 7-17ms/次（低频配方加载可行，事件级高频不可行）；② 数据相关谓词（"now 在静默窗内""10 分钟内 ≥3 次失败"）CUE 表达不了，用 CEL（cel-interpreter 0.10.0，1M+ 下载）在 Rust 内嵌求值，CUE 只静态看住表达式参数闭合；③ 析取类 schema 报错 DX 差（`8 errors in empty disjunction`），底座需包一层友好报错。
- 版本坑：v0.17 closed definition 不能用 `&` 扩展新字段（报 field not allowed），必须 embedding 风格 `{#Base, ...}`；可覆盖字段必须带 default 标记（`*30 | int & >0`），否则 L4 覆盖=冲突报错——这是 L3/L4 语义的命门，建议定为 lint/评审项。

### 2.4 zot / cosign 实测结果（poc-cas）

- **zot v2.1.22 可用，三条硬结论**：① OCI-only 写入——docker manifest PUT 返回 415，上游 docker 格式镜像必须先转 OCI（存在未测开关 `http.compat`，疑似兼容入口，M1 应优先实测）；② GC 可控触发已闭环——停服后 `zot verify-feature retention <config>`（回收 4 blob、v2 的 4 blob 全保留、重启拉取完整），**无在线 REST API**，要"删 tag 即回收"只能接受停服窗口或短 gcInterval 调度；③ 写入校验只查引用 blob 的 digest+size 存在性，不解析内容（手工把 config/layer 指反也 201）——语义完整性要靠未测的 `zot scrub` 补位。
- **cosign v3.1.3 离线签名/验签成立但有坑**：`sign-blob --bundle` 必填（新 bundle 格式，签名时自动上传 rekor tlog）；`verify-blob --bundle` 即使有 TUF 本地缓存仍会发起在线 TUF 刷新（实测死代理下失败）——**纯离线验证必须 `--insecure-ignore-tlog`**（丢透明账本审计性）或预置 `--trusted-root`（未测）。篡改 1 字节 exit 1 正常。
- **crane 组合拳**：push tarball 只认 docker-save，"pull 成 tar 再 push"在 zot 上走不通；可行路线是 `crane append --oci-empty-base`（本次实证）或 `pull --format=oci` 后自组 blob+manifest。跨 push blob 去重（`existing blob`）与磁盘 blob 名==sha256(内容) 自检通过。
- 网络事实：docker.io 双栈不可达，镜像源用 public.ecr.aws——M1 镜像供给链要预置。

### 2.5 依赖与编译坑（跨报告汇总）

1. **proc-macro-error2 future-incompat 警告**：三份报告独立定位（af-calendar 经 `cargo tree -i proc-macro-error2` 指向 af-credential；af-credential 确认来自 cedar 传递依赖；af-release-gate 确认 thiserror 2 拉入）。当前可用，等上游修，不阻断。
2. **交叉矛盾一处，建议 CI 复验**：af-effect-gateway RESULT 称"整 workspace `cargo build` 因 af-graph `src/validate.rs:358` 缺生命周期 E0106 失败"，而 af-graph 自身 RESULT 称 `cargo build -j4 -p af-graph` 1.22s 通过、0 warning——疑似 af-graph 后修的报告时间差，未在本汇总中重跑构建验证，最终方案评审前应全量 build 一次确认。
3. **根 Cargo.toml 无 `[workspace.dependencies]`**：af-workorder 与 af-release-gate 均警告 11 个 crate 各自声明 serde/thiserror 版本会漂移，建议正式方案统一收进 workspace 依赖表。
4. **Temporal Rust SDK 三坑**：包名 `temporalio-sdk`；硬依赖系统 protoc；需显式声明 futures/temporalio_common/temporalio_workflow 直接依赖。
5. **biscuit 6 API 坑**：无 Float（预算统一 `*_cents: i64`，跨 crate 对齐 af-meter）；builder 只收 `&str`；check 是存在性查询（一次请求恰好一个 request 事实由调用方保证）。
6. **下载/环境坑**（BOOTSTRAP.md）：cargo 不在默认 PATH（每命令 `source ~/.cargo/env`）；构建一律 `-j 4`；GitHub release 大文件慢（zot 215.9MB 约 35 分钟）且偶发 SSL eof 需 `--retry 3`；crane 指定资产名已 404（实际为 `go-containerregistry_Linux_x86_64.tar.gz`）；temporal.download 参数必须 `?platform=linux&arch=amd64`；clippy 未装（minimal profile），af-graph/af-sampling 只做到 rustc 零警告，未跑 lint。
7. af-effect-gateway 新增 sha2 0.10（纪律清单外最小依赖，载荷指纹用）；af-stop 零依赖、自带 xorshift64*（af-sampling 同款选择，避免 rand）。

### 2.6 接口与语义层的横向共识（供最终方案直接采纳）

- **错误要类型化**：af-workorder 的 `PreconditionsNotMet{missing: Vec<Missing>}`、af-meter 的 `MeterError::Insufficient{need, available, recovers_at}`、af-effect-gateway 的 `PayloadConflict`/`StaleEpoch`、af-trace 的 `Err(CriticalFull(原事件))`——四份报告一致证明"结构化错误让编排层可编程下一步"。
- **时间一律交 af-calendar**：af-workorder 的 deadline 现为字符串比较（遗留项明确指向接 Calendar/DateTime<Utc>）；af-meter 窗口建议支持"对齐自然日/周+时区"（同样指向日历）。
- **幂等三板斧已被三处独立验证**：业务幂等键（Temporal 账本 attempts=2 扣款 1 次）、DB 主键/UNIQUE 幂等（af-station、af-effect-gateway）、外呼恰好一次计数器证明（af-effect-gateway）。结论一致：**Temporal at-least-once + 业务幂等 = exactly-once**。
- **"边界放 Rust、执行放外部"模式出现两次**：DSPy IR 校验器放 Rust（poc-dspy）、CUE 谓词参数静态校验在 CUE、数据谓词求值在 Rust 内嵌 CEL（poc-cue）。
- 时间源风险两处点名：af-effect-gateway 用 SystemTime（可回拨，建议单调钟）；af-station 跨机部署依赖各连接时钟同步。

---

## 三、建议

### 3.1 语言选型（维持并收敛）

1. **Rust 为底座唯一主体语言**：全部 11 crate 无一需要换语言；纯 Rust 依赖树（无 openssl）+ bundled SQLite + 秒级增量编译已实测。仅两处例外按既定边界走：DSPy 优化执行留 Python 薄适配层（IR/校验在 Rust）；Temporal worker 宿主短期用 Python SDK（Rust SDK 留作后续深验证项，见 3.4）。
2. **配方体系 = CUE（结构/参数/覆盖，部署期预编译 JSON）+ CEL（数据谓词，Rust 内嵌）**：不要试图让 CUE 当规则引擎，也不要引入 rhai 类图灵完备脚本（设计纪律排除）。
3. **计量数值统一整数最小单位**（af-meter 建议：调用数 u64、金额 i64 分），与 af-credential 的 `*_cents: i64` 词表同一次评审定死。

### 3.2 选型调整与必须写进方案的决策

1. **采纳 Temporal，同时立项"工作流代码版本化"**：裁决证据见 2.2；配套必做——审批门统一"signal 进/update 出回执"二选一约定、跨 run 状态收敛到显式 State dataclass 并与 run 签名同类型（CAN 参数类型不一致会**静默丢字段**，poc-temporal 实证）、外部 activity 一律带业务幂等键。
2. **zot 校验线三件补测（M1 优先）**：`http.compat` 开 docker 兼容实测（能省全部转换管线）；`zot scrub` 补写入校验缺口；cosign 离线策略二选一落地（`--insecure-ignore-tlog` 或预置 `--trusted-root`），并把"上传镜像=OCI manifest 端到端"定为接口。
3. **止损/升迁/闸门的语义直接照抄 PoC 结论**：af-stop 的优先级序（check 首败>指纹连击>阈值）与"连续"定义原文写进规范；af-sampling 的 current/attained 双水位与批次增量口径；af-release-gate 的下限事件化、夜间限流配排班产能、限流件 defer 不重试、渐放独立泊位。
4. **af-graph 两条设计纪律固化**：循环体豁免 DAG/map 体不豁免；先引用完整性再判 DAG。同时把工件名级引用完整性与 Violation 源位置列入正式方案范围（PoC 未做）。

### 3.3 波次顺序建议（按报告间已声明的依赖关系）

- **波次 1（地基，无相互依赖可并行）**：af-trace（观测底座）、af-stop（止损）、af-graph（模板校验）、CUE 预编译管线（vet+export 落 JSON 的 CI 化）。
- **波次 2（账本与凭证）**：af-meter 先行（预算真相源）→ af-credential 对齐 `*_cents` 词表接账本余量注入；af-station（人工任务账本）可并行。
- **波次 3（编排合成）**：af-workorder 注入 af-calendar 的 `Calendar` 并把 `Sla.deadline` 换成 `DateTime<Utc>`；af-credential 的签名能力下沉给 af-workorder 决策工件（换掉 FNV-1a64 占位，改 SHA-256/BLAKE3）；af-effect-gateway 与 af-release-gate 挂到工单状态机上。
- **波次 4（引擎与外围深化）**：Temporal Rust SDK 补 signal/update/CAN 同规模验证；poc-dspy 的 Rust IR schema 定稿（demos 是否入 IR、LM 配置隔离）；zot/cosign 校验线补测（见 3.2-2）。

### 3.4 全量收尾前三件事

1. **一次 workspace 全量 `cargo build -j4` + `cargo test -j4`**：了结 af-graph E0106 的交叉矛盾（2.5-2），并顺势统一 `[workspace.dependencies]`（2.5-3）。
2. **安装 clippy 补静态检查**：minimal profile 未装，af-graph/af-sampling 明确留了欠账。
3. **各 PoC 的"未测"清单勿带入正式方案当已验证**：DBOS 未实测、`cuengine`/Go CUE 库未测、Temporal worker 被杀重投递未测、`workflow.patched()` 未测、zot 周期 GC 长跑未测、DST 日历未实测——均已在各 RESULT.md ⑤ 节如实登记。

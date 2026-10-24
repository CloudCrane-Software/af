# RESULT-消债ABC —— 补丁台账第二阶段 A/B/C 三件主线重构销项

> 2026-10-25 · 消债实现员（动态工作流子代理） · 工作树 af-wt-phase2-execline @ phase2（自 04a125f 起 11 个 commit）
>
> **一句话结论：F-首单三补丁（A af-mes 认证面 / B af-order 执行面+MES 写入 / C HarnessRunner 四步工单）已全部在主线重构为正式设计并按台账消除条件逐条销项——TDD 先红后绿 6 对 RED/GREEN commit，workspace 测试 1089 → 1114（+25）零回归，golden 11/11 零回归，clippy 零新增，七门 7/7 全绿；mingda 真实例三驱零双写验证读侧对账；顺带暴露并修复 af-mes 读路径 FINAL 潜伏缺陷（真实例 ILLEGAL_FINAL，mock 测不出）。主线自身可跑真实订单，phase2-f-patches@9b2321f 实体已全部被主线超越（分支保留作历史档案，未删。**

---

## 0. 销项对照（台账消除条件逐条）

| 条件 | 消除条件（台账原文要点） | 结果 | 证据锚点 |
|---|---|---|---|
| A | ch::build_client_auth 与 WriterConfig::{user,password,with_auth} 转正式 API；mock/集成测试覆盖带认证路径；匿名路径行为逐字节不变的回归测试在；公开文档注释 | ✅ | §1；commit 52de217(RED)/d076da5(GREEN) |
| B① | Submit exec 参数面转正式 OrderSpec/CLI 契约；F-首单/二单旗标向后兼容（证据链可复现）；stub 路径退役或标注冒烟档 | ✅ | §2.1；commit b0b0e39(RED)/8892d3b(GREEN) |
| B② | af-execline driver 增正式 writer 缝（MesWriter trait 或等价）；af-order 文件 stub 直写 CH 逻辑替换为 driver 缝消费；订单面只留声明不做写侧 | ✅ | §2.2；commit 0a78137(RED)/dad6ca6(GREEN) |
| B③ | marker 幂等升级为 CH 读侧 event_id 对账：ship 前按 event_id 查 af_mes.mes_event（mingda 实例）已有则跳过 | ✅（真实例验证） | §2.3、§3；commit c426bdc（真实例暴露的 FINAL 修复） |
| C | HarnessRunner::run 内四步账面工单上移为正式构造件（独立函数/类型，run 只调用）；补真实 harness 集成测试（非 FakeRunner）覆盖订单态门（Created 拒 declare→Released 放行）；吸收方案 §1.1/§2.3「四步不搬」判定已证伪的复核结论写进 RESULT | ✅ | §4；commit b07e64f(RED)/63cf75c(GREEN) |

---

## 1. A —— af-mes 认证面正式化

**设计**：认证经官方 clickhouse crate 0.15.3 的 `with_user`/`with_password`，线格式为 `X-ClickHouse-User` / `X-ClickHouse-Key` **请求头**（headers.rs with_authentication，实测确认不在 URL query）；`None/None` = 匿名 = 旧行为。

- `ch::build_client_auth(endpoint, database, user, password)`（ch.rs）：正式构造，旧 `build_client` 并入其 `None/None` 形态（匿名请求线逐字节不变由测试钉死）。
- `WriterConfig::{user, password, with_auth}`（writer.rs）：公开字段 + builder；**Debug 手写脱敏**——password 渲染为 `"***"`（凭据不进日志/回执）。
- `MesReader::open_with_auth`（reader.rs）：B③ 读侧对账所需认证读（B② 的 sink 同源使用）。
- mock 扩展（mock.rs）：`CapturedRequest{request_line, headers}` 全录 + `requests()` 访问器（凭据线格式断言依据）。

**测试**（tests/http.rs，26/26 绿）：带认证 writer 头注入与值一致 / 匿名零凭据头零 URL 凭据（插入+SELECT 全查）/ 带认证 reader SELECT / WriterConfig Debug 脱敏。

**偏差**：无。

## 2. B —— af-order 执行参数面 + MES writer 缝 + CH 读侧对账

### 2.1 B① exec 参数面 = 正式 CLI 契约（crates/af-order/src/exec.rs）

**设计**：OrderSpec（JSON）保持**身份极简**（order_id/line/budget/note，F1/F2 的 7 行 spec 原样可复用）；执行面 = CLI 旗标声明。`RealExec{task, profile, data_root, host, max_turns, spend_cap_cents}`：

- 三件套 `--task/--profile/--data-root` 同给 = 真实 SwevExecuteUnit（SshBridge+HarnessRunner+冻结判分）；全不给 = **stub 冒烟档**（显式分档保留，不退役——既有测试与冒烟面不变）；半套 = OR1001；附属旗标不得脱离三件套。
- 缺省 = fix2 代际：`anolis` / 250 轮 / $3（`DEFAULT_HOST/DEFAULT_MAX_TURNS/DEFAULT_SPEND_CAP_CENTS` 常量单源）。
- **F1/F2 逐字兼容钉死**（tests/exec_contract.rs）：两条已归档命令逐字解析断言（F1 django-11099×glm-cp×60×$1×auto；F2 requests-1142×step-5 同款）——证据链可复现。
- `swev_unit_spec_for(exec)`：fix2 基底 + 预算硬顶覆写 + 现产判分锚 `a292547f…`（冻结链）。

### 2.2 B② MES 写入上移 = 驱动正式缝（af-execline mes.rs / mes_ch.rs）

**设计**：`MesSink` trait（正式缝）两面分离——

- `append`（记账面）：驱动每次跃迁调用；**文件正本 mes-events.jsonl 必写**（权威记录，F-首单 ship v2 教训保留）。
- `sync`（对账面）：`drive()` 收尾调用；文件实现 no-op（冒烟路径零改动），`MesChSink` 实现 = `retry_outbox 先行 → 读侧对账 → 补差量送批`。

`MesChSink`：`MesChTarget::{Auto, Url}` 目标声明（auto：env `MES_CH_URL` > 0600 `~/.config/mes-ch/url`，bench-mes.py 同约定；URL 可含 `user:pass@host` 拆认证头，回执只含 host:port）；每行独立批（max_rows=1，失败即时显形）；Parked = **EX2003 fail-loud**（新稳定码，EX2xxx 记账面只增不改）。

**订单面只留声明**：af-order 把 `--mes-ch` 字符串原样转交 `MesChTarget::parse` + `MesChSink::open` + `open_with_sink`，**零 CH 写侧代码**；补丁期 af-order 主依赖 af-mes 的必要性消失（正式设计仅 dev-dep 供 mock 集成测试）。

**测试**（tests/mes_sink.rs 8/8 + af-order tests/exec_contract.rs 端到端 2 例）：文件缝形态 / CH 送批与回执零凭据 / 读侧差量 / 全重叠零送 / Parked EX2003 / **park 后重开收敛不双写**（retry_outbox 先行再对账）/ 目标解析（env>文件>报错、凭据段拆分与残缺拒绝）/ 驱动端到端（真引擎子进程：一驱 4 行入 CH、二驱重放零补送零双写）。

### 2.3 B③ marker 幂等 → CH 读侧 event_id 对账

**设计**：幂等真值源 = CH 本身（`SELECT … FROM mes_event WHERE work_order_id=?` 按 event_id 认行），不再依赖本地 `mes-ch-shipped.jsonl` marker（**marker 退役**；F1/F2 订单目录中的旧 marker 成为死文件，重跑走读侧对账——CH 已有该 4 行则 shipped=0，见 §3 真实例）。结构性收益：跨驱补送（重试驱新跃迁）与 park 后重发双写两个补丁期孔洞同时闭合。

**真实例验证**（mingda `http://100.64.0.2:15221`，凭据源=平台约定 0600 文件，零明文零打印）：

1. 独立只读回读 F1/F2（bench-mes 同款连接面）：各恰 4 行，event_id/kind/quantity 与交付报告逐字段一致。
2. 真实验证单 `XZ-B23-verify-20261025`（line_id=verify-xiaozhai 自标识，冒烟档+--mes-ch auto）三驱：
   - 首驱（旧二进制，FINAL 修复前）：订单 Closed，CH 写失败 → **EX2003 fail-loud 显性上报**（exit 1，真实落账纪律实证）；
   - 二驱（修复后重发）：replayed=true，读侧对账 already_in_ch=0 → **补差量 4 行**（rows_delivered=4，outbox=0，auth=true，回执 endpoint=100.64.0.2:15221 零凭据）；
   - 三驱：already_in_ch=4 → **shipped=0**（幂等上限形态）；独立只读回读恰 4 行 verify-xiaozhai——**三驱零双写**。

## 3. 真实例暴露的潜伏缺陷：读路径 FINAL（commit c426bdc）

**红（真实例证据）**：mingda CH 26.7.34.3 实测 `SELECT … FROM af_mes.mes_event FINAL` → HTTP 500 `Code: 181. DB::Exception: Storage MergeTree doesn't support FINAL (ILLEGAL_FINAL)`；`system.tables` 实查引擎=MergeTree（poc-mes 裁决原文即 MergeTree、无版本列，FINAL 无意义）。mock 无 SQL 引擎，af-mes 自身测试从未暴露；F-首单回读走 bench-mes.py（无 FINAL）故未触发。B③ 首次把 `MesReader` 打到真实例即现形。

**绿**：reader.rs `event_select_sql` 去 FINAL；`select_final`（ReplacingMergeTree 读包装器）语义保留不回退 OPTIMIZE；两处测试断言随真实例证据修正（af-mes tests/http.rs、af-execline tests/mes_sink.rs，均注明实测依据）。修复后真实例读回全通（§2.3）。

## 4. C —— HarnessRunner 工单构造件 + 真实 harness 集成测试

**构造件**（agent.rs）：`bench_paper_order(run_id) -> Result<WorkOrder, String>`——旧路 bench_order 语句级对照四步 `bind_line(bench-line/1)` → `attach_budget_voucher(v-{run_id},1,signed)` → `release` → `start`，终态 InProgress；`HarnessRunner::run` 内联四步全部上移，run 只调用（构造失败=结构化 AgentRunFailure，不 panic）。

**真实 harness 集成测试**（tests/agent_paper_order.rs，**非 FakeRunner**——真 Opened/真 effect gateway/真 ToolRegistry/真 RunCredential 签发；模型 HTTP 端点按 af-harness cargo test 惯例走本 crate stub 回放）：

- 门钉（恒绿）：真 gateway 下 Created 工单首个 bash side-effect 被拒——钉住门本身。
- runner（RED→GREEN）：主线原状死于 `effect gateway: 订单态拒绝 declare：order_id=wo-paper-a1 state=Created，允许 [Released, InProgress]`（**F-首单 EX3001 失败原地复现**，运行时红实证）；构造件落地后同链路 Ok（Completed、effect_fresh≥1、model_calls=1）。
- 形状：构造件四步材料与终态逐项断言，对照裸构造=Created。

**吸收方案 §1.1/§2.3 复核结论（判定已被证伪，结论入档）**：「四步不搬」的依据是「af-harness 只查 id 非空（run.rs:221-223）」——该检查只是 `Opened::run` 的入参校验，工具 side-effect 真正的门在 af-effect-gateway 的 declare 订单态校验（`DECLARE_OK=[Released, InProgress]`，lib.rs:170/486）；FakeRunner（P4/P5 验证缝）伪造整条 runner 链、L0 回放（P6）不经过真 gateway，两者结构性无法暴露该门。**"订单四步生命周期不搬"对订单面（driver→af-engine 门控）成立，对 harness 进程内账面工单不成立**——两套四步各司其职，互不替代；本构造件即后者缺口的正式闭合。（正本同步：编排者）

## 5. 偏差与如实登记

| # | 项 | 说明 |
|---|---|---|
| 1 | `--mes-ch` 从 RealExec 上移为 `Submit` 独立字段 | GREEN 归位：记账声明与执行面正交（冒烟档同样可声明 CH 记账）。F1/F2 命令语义不变（旗标与取值逐字兼容），仅 Rust 字段位置调整；测试断言位置随契约归位并注明。 |
| 2 | 补丁期 OR2006 → 正式 EX2003 | 写侧上移 af-execline 后按其码表记账（EX2xxx 记账面，只增不改）；OR2006 仅存在于 patches 分支从未上主线，无重编号违约。 |
| 3 | marker 文件退役（无兼容垫片） | F1/F2 重跑即走读侧对账（CH 已有其 4 行→shipped=0）；旧 marker 成死文件保留原处。 |
| 4 | MES/CH 凭据源 = 0600 文件（非 bao） | bao 未托管 mes-ch 密钥（实查 0 字段）；平台约定即 0600 `~/.config/mes-ch/url`（mes迁移mingda-交付 §users.xml 登记，F-首单同款接线）。全程零明文零打印，回执只含 host:port。 |
| 5 | stub 路径保留为显式冒烟档 | 台账给"退役或标注"二选一；退役将破坏既有测试与冒烟面，取标注分档（exec.rs 模块文档 + cli.rs 路径注释明示非生产路径）。 |
| 6 | golden_test 预存在竞态（观察项，未动） | `g3_golden_tamper_detection`（golden_test.rs:418-419）**原地篡改 golden 文件再恢复**，与并行执行的 byte_exact 读同文件构成竞态——本任务两次全量测试中偶发 2 失败（首字节 `{`→` `，等长，与篡改字节一致），孤立重跑 11/11 过、七门 step3 过。非本次改动引入；不动 golden（纪律），登记待 golden 线处置。 |
| 7 | 真实验证单在生产 MES 表留 4 行 | `XZ-B23-verify-20261025`（line_id=verify-xiaozhai 自标识）经真引擎全生命周期，4 行跃迁为真实记账；删除生产数据风险大于保留，如实登记。 |
| 8 | patches 分支处置 | phase2-f-patches@9b2321f 三实体已被主线全部超越；分支保留作历史档案（不删不改）。台账在挂区三行移入已消除区。 |

## 6. 验证汇总（全部本任务实测）

- **TDD**：6 对 RED/GREEN commit（A、B②③、C、B① 各 RED→GREEN；RED 证据=编译错或运行时失败原文，见各 commit message）。
- **workspace 测试**：1089 → **1114 passed / 0 failed**（+25：af-mes http +4、af-execline mes_sink +8 + agent_paper_order +3、af-order exec_contract +10）。
- **golden**：`cargo test -p af-compiler --test golden_test` = 11/11（零回归；竞态观察项见 §5-6）。
- **clippy**：七门 step2 基线门 PASS（过程逮到 2 处新增已消：unused import 8e192f8、unwrap_or_default a473e46）。
- **fmt**：`cargo fmt --all --check` 全仓通过。
- **七门**：`CARGO_TARGET_DIR=~/afbuild-phase2i bash ci/run.sh` = **7/7 全绿**（ci/out 收编）。
- **真实例**：mingda 三驱零双写（§2.3）+ F1/F2 独立回读一致（§1 前置）。

## 7. Commit 清单（自 04a125f 起）

| commit | 内容 |
|---|---|
| 52de217 | A-RED 认证面测试（with_auth/open_with_auth 缺，编译红）+ mock 捕获请求头 |
| d076da5 | A-GREEN build_client_auth/with_auth/open_with_auth + Debug 脱敏 |
| 0a78137 | B②③-RED MesSink/MesChSink/读侧对账测试（缝 API 缺，编译红） |
| dad6ca6 | B②③-GREEN mes.rs 缝 + mes_ch.rs + driver 接线 + EX2003 |
| b07e64f | C-RED 真 harness 集成测试（runner 死于订单态门，运行时红=F-首单失败复现） |
| 63cf75c | C-GREEN bench_paper_order 构造件上移 + 证伪结论入档 |
| b0b0e39 | B①-RED exec 契约测试（F1/F2 逐字兼容/三件套/映射/mock CH 端到端） |
| 8892d3b | B①-GREEN exec.rs 正式 RealExec + --mes-ch 独立声明 + 驱动缝消费 |
| c426bdc | fix(af-mes) 读路径去 FINAL（真实例 ILLEGAL_FINAL 证据） |
| 8e192f8 | chore 清 unused import |
| a473e46 | fix(af-order) clippy unwrap_or_default 消解（七门 step2 转绿） |
| （本 commit） | RESULT-消债ABC.md + ci/out 七门证据收编 |

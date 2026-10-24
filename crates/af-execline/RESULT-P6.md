# RESULT-P6 —— af-bench 吸收 · 终包 P6（薄壳化 + 影子对拍 L0/L1 + L2 钩子）

> 身份：吸收施工员 P6（P2-F08），2026-10-24。工作树 `af-wt-phase2-p2`（分支
> `phase2-execline-p6`，基于 P5 完成点 1830371——suite/bridge/judge/Swev+Tb
> 单元/驱动接线全套在位），权威施工图 `计划/研究组/af-bench吸收方案-20261024.md`
> §5-P6 与 §4 影子对拍三层。施工形态（ADR-007 先红后绿再重构）：RED `3b92aee`
> （p6_shadow 验收测试 5 红/3 忽略 + shadow.rs `unimplemented!()` 桩 + 冻结件
> 基准 + p1_comparison/p3_judge 对照基准迁冻结件，既有 95 例全绿不受影响）→
> GREEN `707b5b0`（shadow 工具面）+ `e5f03c8`（af-bench 薄壳化）。af-workorder /
> af-engine 零改动（`git diff 1830371..HEAD --stat -- crates/af-workorder
> crates/af-engine` 输出 0 字节，实测）；未写 计划/变更日志.md；未跑真容器/
> 真模型/网络（L2 金丝雀只备入口与 #[ignore] 用例，编排者裁决不在本包执行）；
> 未碰工厂守护进程与 AB 测试线。
>
> **范围（编排者裁定）**：本包完成 ①薄壳化 ②L0 判分回放对拍 ③L1 FakeRunner
> 取证判分链对拍 ④L2 钩子（入口/配置面/#[ignore] 在册）。原方案 §4.1 的
> L1（真容器镜像复原）与 L2（真容器+真模型端到端）不在此执行——真容器/真模型
> 级对拍属后续 F-首单任务包。

## 1. 薄壳化改动清单（af-bench：1255 行 main.rs → 843 行薄壳；swev.rs/tb.rs 删除）

| 改动 | 形态 | 契约面 |
|---|---|---|
| `main.rs` 重写为薄兼容壳 | run_swev/run_tb 循环骨架留壳（方案 §5-P6 原文），单题执行 = `SwevExecuteUnit`/`TbExecuteUnit::execute` 直调（P4 单元直调形态——方案 §5-P6 验收原文"薄壳产出的 results 行与 P4 单元直调逐字段一致"即此形态） | CLI 参数面逐字不变（`Args::parse_from` = 原 `parse()` 逻辑抽出可测入口，行为零变化；六条拒绝路径文案逐字实测一致：未知参数/缺值/缺必填/坏整数/未知套件/未知档案，见 §4 演练记录） |
| **results 行唯一来源 = af-execline 单元** | 壳内 run_swev/run_tb **零行装配**（`grep` 实测：壳内 `"suite"` 装配仅 injec 一处）；18/12/9/7/11/8 键全部由 `record.rs` 装配、经 `FileRecordSink` 落盘（append+每行 flush） | **行字节契约不变**：p6_shadow D7 断言新行与冻结 main.rs:861-890 模板 oracle **Value 全等 + 序列化字节全等**；同源双轨（v1 投影 `records.jsonl` 为新增文件，不触碰既有 `results.jsonl`） |
| `swev.rs`(293)/`tb.rs`(341) 删除 | 载荷已 P1-P5 全量在 af-execline（suite::swev/tb）；删除前 `git show 1830371` 冻结字节级副本入 `tests/fixtures/p6-frozen-bench/`（sha256 provenance 测试钉死） | 迁移源可追溯：冻结件三文件 sha256 在册（cmp_p6_frozen_fixture_provenance） |
| Cargo.toml | 增 `af-execline`（薄壳调库，方向禁反转——af-execline 不依赖 af-bench，无环由 cargo 构建实证）；`sha2` 移除（唯一使用方 swev.rs 已删）；其余依赖保留（injec 线执行面） | 依赖方向 = 方案 §1.2 既定；`cargo deny` 四类 ok |
| D-20 attempt 递增 | 壳层 `next_attempt_index`（= af_execline::suite 同函数）分配 `swev-<task>-<profile>-a<N>`；R8 TB 固定名覆写在壳层（层属调用方，P5 §3-2 口径） | runs/ 布局与 run_dir 字段值域不变 |
| 进度行/汇总行 | `[i/n] task status=… resolved=… tokens(in=,out=) agent=…ms` 取自刚落的 legacy 行（行是唯一权威口径）；失败臂 SETUP_FAILED/RUN_FAILED 文案同旧（TB 的 SETUP_FAILED 附 verifier_note 同旧） | stderr 观感面不变（计量数值豁免——同源同值） |
| injec 线 | 留壳内（方案 §1.1"本期不搬"）；profile/resolve_key/status_label 改用 af_execline 同函数（P1 已钉字节级一致） | 行形状/文案零变化 |
| 订单上下文 | 壳无订单面：order_id=`{instance_id}-a{N}`（main.rs:778 惯例）、line_id=`bench-line`（main.rs:524）、stable_id 同源派生——只进 v1 投影不进 legacy 行；旧路 bench_order 的进程内四步由 HarnessRunner 内账面工单 + P5 driver 引擎 op 序列承接（driver 全生命周期入口保留，见 §3-L2） | legacy 行无订单字段（新行订单字段差集单独核对，§4.2-1 口径） |
| **机制升级一处（方案明文）** | 单元装载期判分指纹门 fail-loud（§3.2-2：judge 脚本 sha256 ≠ a292547f 锚即 EX3002 拒跑）——af-bench 原无此门 | 拒跑发生在起容器/调模型之前；旧路无此防线属吸收方案的既定加固 |

## 2. 对拍结果（L0/L1 实跑记录；L2 钩子在册）

### L0 判分回放（零容器零模型，真 venv 真脚本；判据 = 新旧两路逐位一致 100%）

- 证据：bench-obs4-out 战役 `astropy__astropy-12907` a1–a4（**4 份独立 run 证据**，
  各异 eval.log/agent.diff；task_json 由 `/tmp/swev-s1` 补齐）。
- 两路：旧 = `shadow::old_judge_call`（af-bench swev.rs:205-210 逐字复制，源文本
  与冻结件双向钉死）vs 新 = `judge::invoke`（locator 参数化）；同 venv 同脚本同 argv。
- **结果 4/4 全一致**：resolved=Some(true)、log_parsed=Some(true)、judge stdout
  163 字节逐位相等（`cargo test --test p6_shadow l0_replay_four_historical_evidences
  -- --ignored` 实跑 PASS，8.5s）。
- 交叉信息（仅报告不判死，§4.1 判据是两路互检）：历史 results.jsonl 四行 reward
  均 true——复判定性与历史终值一致；eval_sha256 四值各异（2c972ea1…/ae7b680d…/
  b0e01753…/7b427f46…）与在盘证据一一对应。
- 原始产物：`工具状态备份/shadow-20261024/l0-replay-4evidences.log` +
  `l0-replay-summary.jsonl`。

### L1 FakeRunner 取证-判分链（零真容器/零模型/零网络）

两变体（假 docker = PATH 首段注入 shim 按命令形状回放 fixture 证据 + argv 留痕；
新旧两路经同一 shim——旧路冻结 swev 模块的 tb_ssh 与新路 SshBridge 在 host=local
下是同一 `bash -c` 管道）：

**(a) 假判分变体（everywhere 可跑，`l1_fake_runner_chain_vs_frozen_bench_six_dimensions`）**
——10 维断言全过：D1 agent.diff 字节（旧 capture_diff vs 新单元落盘，逐字节相等）
/D2 docker 可见命令族（旧路 rm/run/capture 命令在新路口志原样再现，容器名+6g/512
护栏一致）/D3 eval.sh = task.eval_script 逐字节/D4 eval.log = 回放 fixture 字节
/D5 resolved 消费（fake judge stdout→行 reward）/D6 verifier_note 精确拼装
/D7 **新行 vs 旧路冻结模板 oracle：18 键 Value 全等 + 序列化字节全等**/D8 run_id
公式 `{task}-a1` 与 run_dir 命名/D9 eval_sha256 独立重算对账/D10 gate 记账
（旧路冻结 diff_touches_tests 同 diff 同值）。其中 D5/D6 的旧侧等价依据 = 判分
调用形式逐字钉死（调用形式同 ⇒ 同 argv ⇒ 同输出；L0 实跑已证）。

**(b) 真判分变体（#[ignore] 真跑 PASS，4.1s；`l1_real_judge_old_vs_new_full_verify_chain`）**
——旧路 `swev::verify` 全链（冻结件：capture→eval 执行→judge）vs 新路单元全链
（真 locator 指纹门 + SshBridge + FakeRunner），同题同证据，**6 维全等**：resolved
（且随日志翻转：eval.log→false / eval-ok.log→true，证明确实出自 judge 非旁路）/
verifier_note 拼装/eval_sha256/agent.diff 字节/eval.log 字节/eval.sh 字节。

### L2 端到端金丝雀（钩子在册，不在本包执行）

入口/配置面/用例三件齐（§3）；冻结旧路二进制在位指纹 `7a87876c0a5541c0` 本任务
sha256sum 复测一致。

### shadow-diff 三维度（§4.2）

- 维度一 行字段（`diff_legacy_rows`/`diff_results_files`）：确定性 10 字段逐位判死
  /计量 5 字段记录不判死/键集差（新行订单字段）单独列——`shadow_diff_row_dimensions`
  电池覆盖；
- 维度二 判分一致性：L0/L1 承载（见上）；
- 维度三 MES 映射表（`mes_mapping_table` + `check_mes_against_row` +
  `legacy_family_from_row`）：started↔start、done↔complete、reward→verdict、
  tokens_*/turns/wall_ms→usage 八对映射在册；真引擎子进程驱动（FakeRunner+假
  docker+假判分）四类事件对行检查通过（start/complete 各恰 1、work_order_id
  收敛、quantity=1、行内映射落点齐备）——`l1_mes_mapping_dimension_via_driver`。

## 3. L2 钩子清单（真执行入口与配置面；执行属 F-首单任务包）

| 钩子 | 落位 | 说明 |
|---|---|---|
| 配置面 `L2Canary::from_env` | `src/shadow.rs` | 旧路二进制（缺省 `/tmp/threetest-bin/af-bench-g3-frozen`，`AF_SHADOW_OLD_BIN` 覆盖）/新路二进制（缺省 `CARGO_TARGET_DIR/debug/af-bench` 薄壳，`AF_SHADOW_NEW_BIN`）/题库（`AF_SHADOW_DATA_ROOT` 缺省 `/tmp/swev-s1`）/题集（缺省 7 题：确认集快标 + glm 档）/档案/宿主（缺省 local——R3 分宿主防互杀）/双输出根 |
| 两路命令行 | `old_command_line`/`new_command_line` | CLI 参数面逐字一致（薄壳化契约的可执行形态）；`l2_canary_hooks_in_place` 断言 |
| §4.3 前置清单 | `preflight_checklist` | 双锚口径（a292547f 现产 + 0338566c 历史）/旧路指纹核销（`OLD_BIN_SHA256_PREFIX`）/R3 错峰分宿主/窗口避开 :17 cron 与在飞扩量批/运营者批预算/硬软门判据 |
| #[ignore] 用例 | `tests/p6_shadow.rs::l2_canary_end_to_end_golden_path` | 两路真执行 → 软门 `diff_results_files` 确定性字段全过 → 硬门新路三件套旧 judge 复判 = 行内 reward；缺席即显式跳过。运行：`AF_SHADOW_NEW_BIN=<薄壳> cargo test -p af-execline --test p6_shadow l2_canary_end_to_end_golden_path -- --ignored`（真容器+真模型，运营者批预算后） |
| 旧路判分调用 | `shadow::old_judge_call` | L0/L2 硬门复用的旧调用形式（冻结件源文本钉死） |

## 4. 验证记录（本任务实跑，CARGO_TARGET_DIR=~/afbuild-phase2b，未占 af/target）

| 项 | 命令 | 结果 |
|---|---|---|
| 验收命令①（任务书原文） | `cd /home/pan-ding/workspace/af-wt-phase2-p2 && CARGO_TARGET_DIR=~/afbuild-phase2b cargo test -p af-execline` | **全绿**（exit 0）：lib 18 + lifecycle 8 + p1_comparison 9 + p1_suite 15 + p2_bridge 12（1 ignored）+ p3_judge 4（1 ignored）+ p3_verify 7 + p4_unit 10 + p5_tb 8 + p5_wiring 4 + **p6_shadow 8（P6 新增，3 ignored）**+ doc 0 = **103 passed / 0 failed / 5 ignored** |
| 验收命令②（任务书原文） | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo test -p af-bench` | **全绿**：3 passed / 0 failed（D-20 等价迁移 1 + 参数面回归 2） |
| L0 对拍（≥4 证据） | `cargo test -p af-execline --test p6_shadow l0_replay_four_historical_evidences -- --ignored --nocapture` | **PASS**：4/4 证据 resolved/log_parsed/stdout 逐位一致（见 §2） |
| L1 对拍（≥6 维） | 同上 `l1_real_judge_old_vs_new_full_verify_chain -- --ignored`；`l1_fake_runner_chain_vs_frozen_bench_six_dimensions`（常规跑） | **双 PASS**：真判分 6 维全等（判定随日志翻转）；假判分 10 维全等 |
| 薄壳 CLI 面演练 | `~/afbuild-phase2b/debug/af-bench` 六条拒绝路径 + 全参数集（单测 `args_parse_*`） | 拒绝文案逐字同旧（未知参数/缺值/缺必填/坏整数/未知套件/未知档案，rc=1）；缺省值面（case-file/limit/offset/out/trial/tasks/host=anolis）一致 |
| clippy | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo clippy -p af-execline -p af-bench --all-targets` | 两 crate **0 警告**；仅第三方 proc-macro-error2 v2.0.1 future-incompat 提示（P1 已登记遗留，RESULT-P1.md §3-5） |
| fmt | `cargo fmt --check --all` | PASS（零 diff） |
| cargo deny | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo deny --offline check` | advisories / bans / licenses / sources 四类 ok（含 af-bench→af-execline 新边） |
| 只读约束 | `git diff 1830371..HEAD --stat -- crates/af-workorder crates/af-engine` | 输出 0 字节（零改动） |
| 冻结旧路指纹 | `sha256sum /tmp/threetest-bin/af-bench-g3-frozen` | 前 16 位 `7a87876c0a5541c0`（与 OLD_BIN_SHA256_PREFIX/盘点 §② 一致） |
| git 序列 | `git log --oneline 1830371..HEAD` | 3b92aee（RED）→ 707b5b0（GREEN shadow）→ e5f03c8（GREEN 薄壳）→ 本文件提交，红→绿成立 |
| 资源纪律 | `df -h /`（开工/收尾） | 开工 22G 可用 → 收尾见 RESULT 附记（>15G 线）；构建全走 ~/afbuild-phase2b；零真 docker/真模型/网络调用/工厂守护进程/AB 测试线（假 docker=PATH 首段注入 shim，非真容器） |

原始产物归档：`工具状态备份/shadow-20261024/`（l0-replay-4evidences.log /
l0-replay-summary.jsonl / l1-real-judge-chain.log / l1-fake-runner-chain.log）。

## 5. 与方案的偏差（如实登记）

1. **L1/L2 的真容器级执行移出本包**（编排者裁决）：原方案 §4.1-L1（真容器镜像
   复原 ≈5 题）与 §4.2 口径中的容器级对拍不在 P6 执行；本包 L1 以"假 docker 回放
   历史证据字节 + 真判分脚本"承载取证-判分链对拍（零真容器），L2 只备钩子。
   真容器/真模型对拍属 F-首单任务包。
2. **薄壳采用单元直调形态，不经 driver 全生命周期**。方案 §5-P6 验收原文即
   "薄壳产出的 results 行与 P4 单元直调逐字段一致"——壳以 P4 直调形态保
   `--out` 共享布局（results.jsonl 跨单续写 + runs/ D-20 递增）与旧路逐字节
   契约；driver 全生命周期（引擎 8-op 门控）需要每单独立账本目录（EX2002 身份
   检查），与批形态共享布局冲突，两形态在 F-首单任务包合流。P5 的驱动接线与
   幂等语义不回退（p5_wiring 4 例全绿）。
3. **对照测试基准迁冻结件（明示等价迁移）**：p1_comparison/p2_bridge/p3_judge
   的活源对拍与源文本比对基准，自薄壳化起从 af-bench 活源迁至
   `tests/fixtures/p6-frozen-bench/`（1830371 字节级冻结，sha256 钉死）。语义
   不变（同字节基准）；af-bench 活源已不含载荷，对照面向"迁移时点的旧路"。
4. **af-bench 单元装载新增判分指纹门**（§3.2-2 方案明文的机制升级，非偏差，
   列此明示）：judge 脚本漂移时薄壳拒跑（EX3002），旧 af-bench 无此防线。
5. **薄壳进度行计量字段读自刚落的行**：旧路从内存 report 直取，新路读行
   （同源同值；wall 类字段本就属计时面）。SETUP_FAILED/RUN_FAILED 的区分依据
   = 单元错误文案前缀（行本身已正确，仅 stderr 观感面）。
6. **af-bench Cargo 依赖删 sha2**（唯一使用方 swev.rs 已删）；thiserror 原本
   即未引用，保留未动（最小改动纪律）。
7. **L0 历史行 reward 交叉一致属观测**（4/4 true，与复判一致），判据仍按 §4.1
   只认新旧两路互检——历史行可能出自他代判分脚本，不进断言。
8. clippy 零新增；第三方 proc-macro-error2 提示为 P1 已登记遗留。

## 6. 交接后续任务包的边界

- L2 金丝雀执行（真容器+真模型双路 + 硬软门判据 + 归因）——运营者批预算/窗口后
  按 §3 清单跑 `l2_canary_end_to_end_golden_path`。
- 原方案 §4.1-L1 真容器镜像复原对拍（如仍需要）与 L2 一并归 F-首单。
- 薄壳与 driver 全生命周期的合流（批形态 vs 单单账本布局）——契约 §1.2/落地
  布局定稿时独立裁决。
- R8（TB 目录覆写是否对齐 D-20）、TB/失败行 v1 投影口径——P5 后独立决定，
  未混入本包。

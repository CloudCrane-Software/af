# RESULT-P5 —— af-bench 吸收 · 任务包 P5（订单生命周期接线 + TB 单元）

> 身份：吸收施工员 P5（P2-F07），2026-10-24。工作树 `af-wt-phase2-p2`（分支
> `phase2-execline-p5`，基于 P4 完成点 a88c316——已含 suite/HostBridge/judge/
> SwevExecuteUnit(FakeRunner)），权威施工图 `计划/研究组/af-bench吸收方案-20261024.md`
> §5-P5（依赖序 P1→(P2∥P3)→P4→**P5**→P6）。施工形态：ADR-007 先红后绿再
> 重构——RED `0278c5d`（验收测试 + `unimplemented!()` 桩：p5_wiring 0/4 红 +
> p5_tb 0/7 红，既有 83 例全绿不受影响）→ GREEN `11f78b2`（实现转绿 94/0/2）
> → 重构 `c4dfcdf`（两单元同构段抽共用件，行为不变）→ 测试补强（TB bash
> 无护栏差异钉死，p5_tb 第 8 例）。af-bench / af-workorder /
> af-engine 零改动（`git diff a88c316..HEAD --stat -- crates/af-bench
> crates/af-workorder crates/af-engine` 输出 0 字节，实测）；未写
> 计划/变更日志.md；未碰 P6 内容（薄壳化/shadow-diff/L0L1L2 对拍/MES 映射表/
> factory-watch 判据属 P6）。

## 1. 做了什么（源 af-bench 行号 → 落位）

af-bench 源 = 本工作树 `crates/af-bench/src/`（开工 `diff` 实测与主仓
swev.rs/tb.rs/main.rs 三文件逐字节一致，收尾复测仍一致，rc=0）。

| 搬入物/接线 | af-bench 源 | af-execline 落位 | 迁移形态 |
|---|---|---|---|
| **RecordSink 文件实现** `FileRecordSink` | main.rs:642-648（批级 OpenOptions append）+ 644-648/668-670/891-895/1086-1090（writeln+flush 逐行纪律） | `src/record.rs:61` | 双文件 sink：legacy→`{{out}}/results.jsonl`、v1→`{{out}}/records.jsonl`；单元化后随驱动逐单打开，append 语义跨单续写同文件 |
| **attempt 目录驱动层分配** `allocate_attempt_dir` + `AttemptLayout` | swev：main.rs:698/760-766（`next_attempt_index` D-20 递增不覆写）；TB：main.rs:1000-1002（固定名 `remove_dir_all` 覆写） | `src/driver.rs:77,87` | 套件差异显式为 enum 两臂；**R8 不对称如实保留**（迁移不修，P5 后独立决定，方案 §6-R8）；建目录仍归单元（create_dir_all） |
| **driver.execute 接线** | 方案 §5-P5（P4 时以 `out_dir/runs` 占位 + `PendingRecordSink` fail-loud） | `src/driver.rs` execute() | attempt 由驱动层分配 + `FileRecordSink::open(&out_dir)` 传入 ctx；**重放路径不触**（单元不重跑、行不重落，测试实证）；准备失败按单元失败入账（ok=false、幂等键不落、EX3001 可重试） |
| DriverConfig + `profile`/`attempt_layout` | 方案 §2.2/§5-P5 | `src/driver.rs:65-67` | attempt 命名需要 profile（见 §3-1 双轨偏差）；layout 承载套件差异 |
| **TbEnv::verify**（TB 判分容器链） | tb.rs:224-292 原样 | `src/suite/tb.rs:266` | `ssh()` 换 `bridge.run()`，命令串与超时逐字节不变（清卷→提 /app→建 `afbench-tbver-<task>`→跑 test.sh→读 reward.txt）；判分三件套契约不适用 TB（判分是容器链非 swev-judge 子进程） |
| `TB_SYSTEM_PROMPT` | main.rs:975 原样（一个字节不动） | `src/suite/tb.rs:105` | TB system 一行角色（完成协议随 harness 提交式语义） |
| **TbExecuteUnit**（run_tb 结构重组） | main.rs:916-1109 分相 | `src/suite/tb_unit.rs:87,125` | setup（装载+容器）→run（AgentRunner 缝）→verify（容器链）→行落盘→cleanup（RAII）；语句级对照清单见模块头注释 |
| `TbUnitSpec`（静态声明） | main.rs:975/981-991/1021-1027 现值 | `src/suite/tb_unit.rs:47` | fix2 缺省构造器：40 步/TASK_COMPLETE/无工具 3 轮/$1（100/1/100）；**无采样覆写**（main.rs:628-631"只作用于 swe-v 线；TB 不动"）；无判分锚（TB 无 swev-judge） |
| TB 12 键 legacy 行 / 7 键 setup_failed / 8 键 run_failed | main.rs:1068-1085 / 938-946 / 1037-1046 原样装配 | `src/record.rs:295,312,335,348` | `TbExecutionFacts` 同源；note 截断 300（`clip300` 共用）；**TB 无 v1 投影**（§3-3） |
| ContainerGuard 定居 suite::tb | §2.4 cleanup 必达（Drop → tb.rs:294-296 `docker rm -f`） | `src/suite/tb.rs:343` | P4 私有实现移居 tb 模块，swev/tb 两单元共用（GREEN 一并完成，非事后重构） |
| 单元侧行落盘共用件（重构） | main.rs:668-670/947-949 与 891-895/1086-1090 两对同构段 | `src/record.rs:114,128` + `src/agent.rs:82` | `append_legacy_row`/`fail_with_legacy_row`/`llm_cents_policy`（Policy 组装 main.rs:746-758↔980-991 同构）——c4dfcdf，行为不变 |

**bench_order 替换的验收形态（方案 §5-P5 第一目标）**：四步生命周期
（bind_line→attach_voucher→release→start）自 PoC 起即由 driver 的引擎 op 序列
承载（`driver.rs` engine_step，真 af-engine 子进程）；P1-P4 期间从未有进程内
WorkOrder 生命周期调用进执行链（P4 RESULT §3-8 已登记 HarnessRunner 内
`WorkOrder::new` 仅是 harness 账面工单、af-harness 只查 id 非空）。**P5 补上的
是"对真实单元成立"的集成证明**：p5_wiring 全生命周期测试以 SwevExecuteUnit
（FakeRunner/FakeBridge）经驱动走完 8 op，ledger 序号连续、MES 四类各一、
voucher/line 出自 DriverConfig（driver.rs:43-45 即 cfg.line/cfg.voucher）、
同 order_id 二次 drive → replayed=true 且单元不重跑（runner 计数仍 1）。

**判分三件套契约与判分独立性语义不变**：P3 的 judge.rs/swev::verify 与 P4 的
装载期指纹门零改动（p3_judge 4 例/p3_verify 7 例/p4_unit 10 例全绿即证）；
TB 判分走容器链（af-bench 原样），不经 swev-judge，不触碰判分独立性边界。

**未搬（按方案范围纪律）**：af-bench 薄壳化与 CLI 参数面（P6）；shadow-diff
对拍工具与 L0/L1/L2 执行（P6，L2 需运营者批预算）；MES 映射表进对拍报告
（P6，R6）；TB run 目录覆写是否对齐 swev D-20（P5 后独立决定，R8——本包
以 `AttemptLayout::TbOverwrite` 如实保留并加回归测试钉死）。

## 2. 测试清单（P5 新增 12 例：p5_wiring 4 + p5_tb 8；全套 95 passed / 0 failed / 2 ignored）

方案 §5-P5 验收草案逐项落位：

| 测试 | 覆盖（草案条目 → 断言面） |
|---|---|
| `full_lifecycle_with_swev_unit_ledger_mes_rows_and_trio` | **集成（真引擎子进程 + FakeRunner + FakeBridge + 假判分 venv + 假 docker）**：全生命周期 8 op（create_order/bind_line/attach_voucher/release/start/attach_artifact/complete/close）入账序号从 1 连续、close 成功唯一；MES 四类恰各一、work_order_id 收敛 stable_id；**results 行落盘**（out/results.jsonl 恰一行 18 键、run_dir=驱动层分配的 `runs/swev-<task>-<profile>-a1`、eval_sha256 独立重算对账）+ **v1 行落盘**（out/records.jsonl、plan_id=order_id/node_id=line_id）+ **三件套落盘**（eval.log/eval.sh/agent.diff 在驱动分配的 attempt 目录）；resolved=true→合格终态工件→close 放行（对齐 lifecycle.rs:1-11 断言风格） |
| `second_drive_replays_real_unit_without_rerun_or_duplicate_rows` | **同 order_id 二次 drive**（driver.rs:207-215 幂等语义对真实单元成立）：replayed=true、终态不变、**单元不重跑**（同一 FakeRunner 实例计数仍 1）、容器不重起（docker run 仍恰 1 次）、close 成功记录唯一、replay_skip 在账、序号仍连续、results/records 行不重落（仍各 1 行）、MES 仍四类各一 |
| `attempt_layout_asymmetry_regression_r8` | **TB 目录覆写不对称回归注记**（main.rs:1000-1002 vs 594-610）：swev 臂——既有 `swev-T-P-a1` 哨兵目录在，分配得 `a2` 且 a1 哨兵原样（D-20 绝不覆写）；空根得 a1。TB 臂——固定名 `tb-T-P`，分配即 `remove_dir_all`（哨兵消失、目录本体移除）。任何"顺手统一"的改动在此红灯 |
| `file_record_sink_append_flush_and_reopen_append` | RecordSink 文件实现：writeln+flush 逐行完整（`{"a":1}\n`）；重开同目录 append 续写不截断（main.rs:644-648 纪律） |
| `tb_full_flow_row_launch_shape_and_verify_chain` | **TB 单元全流程**：12 键行字段级对照（reward=true/status=completed/verifier_note=verify run_log）+ AgentLaunch 形状 TB 原样差异（run_id 无 -a 后缀、user_message=裸 prompt、TB_SYSTEM_PROMPT、**无采样覆写**（temperature/top_p/max_tokens 三 None，glm-cp 基底）、40 步、$1 预算 100/1/100）+ **verify 容器链九步逐串断言**（build tbenv→rm→run -d(2g/256)→[runner]→清卷(`-v {vol}:/vol`+python:3.12-slim)→mkdir+docker cp→build `afbench-tbver-<task>`(tests 目录)→run test.sh(挂 app/logs)→cat reward.txt→guard rm，超时 1800/30/60/120/300/1800/1200/30/60 逐个对）+ vol 名 `{task}-{container}`（tb.rs:227）+ infra 语义（reward=1→合格终态工件；evidence 空——TB 无本地三件套） |
| `tb_verify_missing_reward_is_infra_row` | **TB verify 无 reward.txt → infra 行**（tb.rs:284-288）：reward=null、verifier_note 前缀"无 reward.txt；验证器输出:"、执行 Ok 非 Err、工件不合格 |
| `tb_verify_reward_zero_is_failure_not_infra` | reward=0 → Some(false) 真失败（占分母），verdict/工件资格/行三面断言 |
| `tb_setup_failure_row` | setup 失败（docker run -d 失败臂）→ 7 键 setup_failed 行（main.rs:938-946 字段级，note=TbEnv::setup 失败文案）+ EX3001 + runner 零调用 + run 目录未建 |
| `tb_task_load_failure_is_setup_failed_row` | 任务装载失败（题库缺 instruction.md）归 setup_failed 行（af-bench 原为批级 `?` 中止无行；口径同 P4 RESULT §3-5） |
| `tb_run_failure_row_and_container_cleanup` | runner 失败 → 8 键 run_failed 行（main.rs:1037-1046 字段级含 http_retries=4）+ EX3001 + **cleanup 已调**（guard Drop 断言：末次桥调用=`docker rm -f`）+ verify 链未启动（无 test.sh 调用） |
| `tb_bash_tool_has_no_guards` | **TB bash 工具无护栏**（main.rs:963-970 原样差异）：反考古命令（`git fsck --lost-found`，swev 线会命中 ARCHAEOLOGY_PATTERNS）不注入 `[harness] Searching…`；同命令第 3 次无 `exact command` 教学包装；普通命令原样透传（exit 行在末尾）——迁移不"顺手补齐" swev 线护栏（p4 的两个护栏测试只属 swev） |
| `tb_row_key_sets_and_note_truncation` | TB 三行纯函数键集（12/7/8）+ note 400→300 截断 + reward None→null |

注入手法（零模型、零凭证、零真容器、零 ssh、零网络——资源纪律）：FakeRunner
（AgentRunner 缝）+ FakeBridge（HostBridge 缝，TB 命令形状智能分派）+ 假
docker（PATH 首段注入，p3/p4 手法）+ 假判分 venv（p3 手法）；引擎不打桩
（`EngineClient::locate` 真 af-engine 子进程）。

## 3. 与方案的偏差（如实登记）

1. **DriverConfig 增 `profile`/`attempt_layout` 两字段（方案 §2.2 表列之外）**。
   attempt 目录命名需要 profile（D-20 命名 `swev-<task>-<profile>-a<N>`、TB
   `tb-<task>-<profile>`），方案把分配放驱动层（§2.2 attempt_dir 行、§5-P5）
   而 profile 归单元 spec——**双轨**：组合根（P6 壳）从同一 CLI `--profile`
   构造两者保同源。"换档=换 spec"判据不被破坏（单元行为仍由 spec 驱动，
   驱动侧 profile 只进目录命名）。
2. **TB 覆写（remove_dir_all）落在驱动层 `allocate_attempt_dir`，不在单元**。
   af-bench main.rs:1000-1002 的覆写在 run_tb 任务循环体（=调用方/驱动层
   层次），单元只 create_dir_all（main.rs:1002 后半）。直调单元不经驱动即
   不覆写——与 af-bench 层次划分一致；回归由 p5_wiring R8 测试钉死。
3. **TB 无 v1 投影**（`record: None`，records.jsonl 不写 TB 行）。方案 §1.4
   映射表面向 swe-v 18 键行（gate/eval_sha256/scaffold 等 TB 行没有的字段）；
   验收草案 TB 部分只断言 legacy 行。TB v1 化留 P6 对拍口径独立决定（与 P4
   §3-4 失败行 v1 化同族）。
4. **TB `evidence`/`outputs` 为空**。TB 判分产物在远端卷（`/data/bench/vol/…`），
   本地无三件套——不虚列（对照 swev"只记在盘文件"同款纪律）。
5. **infra（reward=None）不是 Err**。TB verify 无 reward.txt → 行照落
   （reward null）、ExecOutput Ok + verdict None + 工件不合格；若全生命周期
   驱动，订单在 close 门被引擎拒（NoQualifiedFinalArtifact）——"infra 不占
   分母"的订单面自然投影。af-bench 中 infra 仅计数器+行，无订单概念。
6. **RED→GREEN 期间测试修正 3 处（测试自身缺陷）**：`contains`/`ends_with`/
   `starts_with` 的字面 `{TB_TASK}`/`{TB_CONTAINER}` 未格式化（assert 消息
   才是格式串）→ `format!`。RED 桩期全部短路在 `unimplemented!()` 未显形。
7. **全真路径（真容器/真模型/真判分 venv）不进单测**：集成测试 = 方案 §5-P5
   草案明示的"真引擎子进程 + FakeRunner + FakeBridge"；真容器/真模型验证属
   P6 L2 金丝雀（需运营者批预算）。无新增 `#[ignore]` 冒烟（P2 既有 docker
   冒烟 1 例、P3 既有真判分 fixture 1 例维持原样在册）。
8. **TB bash 工具无护栏（原样差异保留）**：反考古/重复命令拦截是 swe-v 线
   （main.rs:710-734），TB 注册处（main.rs:963-970）没有——迁移不"顺手补齐"；
   由 `tb_bash_tool_has_no_guards` 反向断言钉死（GREEN 后测试补强，行为未变）。
9. **驱动准备失败（FileRecordSink::open 失败）按单元失败入账**（ok=false、
   幂等键不落、EX3001 可重试）——与 §2.4 供给面失败可重试语义一致；方案
   未明文此臂，按既有失败语义类推。
10. **clippy 零新增**；第三方 proc-macro-error2 v2.0.1 future-incompat 提示
    为 P1 已登记遗留（RESULT-P1.md §3-5），非本包新增。

## 4. 验证记录（本任务实跑，CARGO_TARGET_DIR=~/afbuild-phase2b，未占 af/target）

| 项 | 命令 | 结果 |
|---|---|---|
| 验收命令（任务书原文） | `cd /home/pan-ding/workspace/af-wt-phase2-p2 && CARGO_TARGET_DIR=~/afbuild-phase2b cargo test -p af-execline` | **全绿**（exit 0）：lib 18 + lifecycle 8 + p1_comparison 9 + p1_suite 15 + p2_bridge 12（1 ignored）+ p3_judge 4（1 ignored）+ p3_verify 7 + p4_unit 10 + **p5_tb 8（P5 新增）+ p5_wiring 4（P5 新增）**+ doc 0 = **95 passed / 0 failed / 2 ignored** |
| RED 基线 | 同验收命令（RED 提交 0278c5d 时点） | p5_wiring 0/4 红 + p5_tb 0/7 红（全 `unimplemented!()` panic）/既有 lib 18+lifecycle 8+p1_comparison 9+p1_suite 15+p2_bridge 12（1 ignore）+p3_judge 4（1 ignore）+p3_verify 7+p4_unit 10 全绿不受影响（83 passed/2 ignored） |
| clippy | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo clippy -p af-execline --all-targets` | exit 0，af-execline 自身 **0 警告**；仅第三方 proc-macro-error2 future-incompat 提示（§3-10） |
| fmt | `cargo fmt --check --all` | PASS（零 diff，exit 0） |
| cargo deny | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo deny --offline check` | advisories / bans / licenses / sources 四类 ok（exit 0） |
| 只读约束 | `git diff a88c316..HEAD --stat -- crates/af-bench crates/af-workorder crates/af-engine` | 输出 0 字节（零改动） |
| 迁移源一致性 | `diff` 主仓 af/crates/af-bench/src/{swev,tb,main}.rs vs 本工作树 | 逐字节一致（diff -q 静默，rc=0，开工与收尾两次实测） |
| git 序列 | `git log --oneline a88c316..HEAD` | 0278c5d（RED）→ 11f78b2（GREEN）→ c4dfcdf（重构）→ 测试补强提交 → 本文件提交，红→绿→重构成立 |
| 资源纪律 | `df -h /`（开工/收尾） | 开工 26G 可用 → 收尾 23G 可用（>15G 线）；构建全走 ~/afbuild-phase2b；未碰其他工作树与主仓 af/target；零 docker 守护进程/LLM 网络调用/工厂守护进程/AB 测试线（假 docker=PATH 首段注入的 shell 脚本，非真容器） |

## 5. 交接 P6 的既定边界

- 薄壳化（af-bench CLI 内部改调 af-execline，参数面不变）+ shadow-diff 三维度
  工具 + L0/L1/L2 对拍执行与报告（§5-P6）；L2 前按 §4.3 复核双锚口径
  （a292547f 现产 + 0338566c 历史）与并发防撞（R3 错峰/分宿主）。
- DriverConfig 与单元 spec 的 profile 双轨同源责任在 P6 壳（§3-1）。
- TB v1 投影/失败行 v1 化的对拍口径独立决定（§3-3，连同 P4 §3-4）。
- R8（TB 覆写是否对齐 D-20）P5 后独立决定——如改，走正常任务包+变更日志。

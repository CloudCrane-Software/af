# RESULT-P4 —— af-bench 吸收 · 任务包 P4（SwevExecuteUnit，FakeRunner 下全流程）

> 身份：吸收施工员 P4（P4-F06），2026-10-24。工作树 `af-wt-phase2-p2`（分支
> `phase2-execline-p4`，基于 P3 完成点 4cba6cb——已含 suite/HostBridge/judge
> 模块），权威施工图 `计划/研究组/af-bench吸收方案-20261024.md` §5-P4
> （§2.2 ExecContext/ExecOutput 扩展、§2.3 AgentRunner 缝、§2.4 失败路径与
> 清理、§1.4 双轨行）。施工形态：ADR-007 先红后绿再重构——RED `b8c4e44`
> （验收测试 + agent/record/swev_unit `unimplemented!()` 桩，p4_unit 9 红/1 过
> + 既有 69 例全绿不受影响）→ GREEN `941c40c`（实现转绿）→ 重构 `6df0f6b`
> （gate 计算抽纯函数 `gate_of` + full_flow 测试断言分组抽 helper，行为不变）。
> af-bench / af-workorder / af-engine 零改动
> （`git diff 4cba6cb..HEAD --stat -- crates/af-bench crates/af-workorder
> crates/af-engine` 输出为空，实测）；未写 计划/变更日志.md；未碰 P5-P6 内容
> （driver 引擎 op 接线/attempt 驱动层分配/RecordSink 文件实现/TB 单元属 P5，
> 薄壳化/影子对拍属 P6）。

## 1. 做了什么（源 af-bench main.rs:行号 → 落位）

af-bench 源 = 本工作树 `crates/af-bench/src/`（开工 `diff` 实测与主仓
swev.rs/tb.rs/main.rs 三文件逐字节一致，收尾复测仍一致）。

| 搬入物 | af-bench 源 | af-execline 落位 | 迁移形态 |
|---|---|---|---|
| `AgentRunner` trait（`AgentLaunch`/`AgentRunOutcome`/`AgentRunFailure`/`BashFn`） | 方案 §2.3 设计（真实现材料 ← main.rs:736-798） | `src/agent.rs:72` | 模型执行缝：单测注入 FakeRunner 返回固定 RunReport——零模型调用、零凭证消耗；Err 侧结构化保 http_retries（§3-2） |
| `HarnessRunner`（真实现） | main.rs:681-687（root）、703-710（bash 注册胶水）、768-773（key 紧邻现取 + 401 重取器）、776-798（Launch 组装）、800/819（http_retries 双路径累计） | `src/agent.rs:84,105` | 语句级对照；root 逐题签发（= main.rs 任务循环内每题一签同粒度，§3-8）；`RootCredential` 未导出故签发入 run() 局部 |
| `SwevUnitSpec`（静态声明） | §2.2 表：profile 名（main.rs:163-327 表=P1 已迁）、采样覆写 0/1000/32000（main.rs:634-636）、两段提示词（main.rs:543, 548-566）、Policy 250 步/TASK_COMPLETE/无工具 3 轮（main.rs:746-758）、单题 $3（main.rs:786-793）、scaffold v3/reasoning off（main.rs:871-872）、判分器定位与指纹锚（§3.2） | `src/suite/swev_unit.rs:49` | fix2 代际缺省构造器 `fix2(profile, judge)`（swev_unit.rs:84）；判分锚默认 = `JudgeLocator::production()` 现产值 `a292547f…` |
| `SwevExecuteUnit::new`（装载期指纹门） | 方案 §3.2-2（P3 在 verify 入口设门时预留"双重防线"） | `src/suite/swev_unit.rs:126` | 装载即 `check_fingerprint()`（EX3002 fail-loud）+ profile 可解析检查——拒在起容器/调模型之前 |
| `SwevExecuteUnit::execute`（分相执行） | main.rs:625-914（run_swev 单题执行体结构重组） | `src/suite/swev_unit.rs:148` | setup（任务装载+容器）→ run（AgentRunner 缝）→ gate（记账非门控）→ verify（P3 判分链）→ 行落盘 → cleanup（RAII）；逐段行号对照见模块头注释（§1.3 清单） |
| bash 护栏 `bash_guard_wrap` | main.rs:710-734 原样 | `src/suite/swev_unit.rs:401` | 反考古命中注入护栏前缀（模板逐字）+ 最近 6 条窗口重复 ≥3 拦截（`{repeats+1} times` 计数口径同源）；纯函数 + 闭包两用 |
| bash 工具闭包 | main.rs:700-735 | `src/suite/swev_unit.rs:387` | 闭包整体 `'static + Send + Sync`（af-harness ToolRegistry 钉死，run.rs:53-55）：Arc<TbEnv> + Arc 桥 + Mutex 窗口；`TbEnv` 补 `Clone` 派生（行为无变化） |
| `ContainerGuard`（RAII cleanup） | §2.4"cleanup 必达"（显式 cleanup ← main.rs:820/854；Drop 目标 = tb.rs:294-296 `docker rm -f`） | `src/suite/swev_unit.rs:449` | 机制升级点（方案明文）：任何早退路径 Drop → `TbEnv::cleanup(bridge)` |
| gate 计算 `gate_of` | main.rs:806-813 原样 | `src/suite/swev_unit.rs:424` | 空/纯空白 → no_patch；`diff_touches_tests` 命中 → test_edit；其余 ok。只记账不入重试（审计 D5） |
| `ExecutionFacts` + 18 键 legacy 行 | main.rs:861-890 原样装配 | `src/record.rs:57,85` | 两轨同源（§1.4 防漂移）；note 截断 300（main.rs:889 口径，`clip300`） |
| `af.plan.record.v1` 投影 | 契约 §1.2 行形状 + 方案 §1.4 映射表 | `src/record.rs:121` | 14 键：plan_id/node_id（order_id/line_id）、item_id=task_id、verdict=reward、evidence{run_dir, evidence_digest=eval_sha256}、usage{turns, unit_calls=model_calls, http_retries, quota=tokens 三分项}、wall_ms、note |
| setup_failed 行 / run_failed 行 | main.rs:656-667 / 822-834 原样 | `src/record.rs:160,176` | 9 键/11 键，字段齐备含 null（D-20）；只落 legacy 轨（§3-4） |
| `RecordSink` trait + `PendingRecordSink` | append+flush 语义 ← main.rs:644-648, 668-670, 891-895 | `src/record.rs:33,42` | 双流（legacy results.jsonl / v1 records.jsonl）缝；文件实现与驱动层接线 = P5（方案 §5-P5），占位 sink 误用即 Err（fail-loud，不静默丢行） |
| ExecContext / ExecOutput 扩展 | 方案 §2.2 | `src/exec.rs:25,52` | ctx +`task_id`/`host_alias`/`data_root`/`attempt_dir`/`record_sink`/`trial`（task_id 补全见 §3-3）；output +`verdict`/`record`/`evidence` |
| DriverConfig 透传 + 占位接线 | §2.2/P5 边界 | `src/driver.rs:48-58,229-243` | +host_alias/data_root/task_id/trial 四字段；attempt_dir 以 `out_dir/runs` 占位 + `PendingRecordSink`——P5 落位真实分配与文件 sink（§3-6） |
| 依赖 af-credential/af-meter/af-stop | 方案 §1.2-3（af-harness 依赖集补齐） | `Cargo.toml` | DAG 无环：cargo deny bans 实测 ok；workspace 无 crate 依赖 af-execline（方向单向） |

**判分链零改动**：P3 的 `judge.rs`/`suite::swev::verify` 原样复用（判分独立性
边界不动——子进程 argv 契约、指纹门、三件套字节语义全部由 P3 测试钉死）；
本包只加装载期一道指纹门（双重防线，脚本 sha256 微秒级、双算无害）。

**未搬（按方案范围纪律）**：bench_order 四步生命周期（bind_line→
attach_voucher→release→start，方案 §1.1"不搬，替换"——P5 由 driver 引擎 op
承接）；`RecordSink` 文件实现与 attempt 驱动层分配（P5）；`TbExecuteUnit`/
TB verify 容器链（P5）；af-bench 薄壳化与影子对拍 L0/L1/L2（P6）。

## 2. 测试清单（p4_unit 10 例 + crate 内 4 例：14 跑全绿）

方案 §5-P4 验收测试草案逐项落位：

| 测试 | 覆盖（草案条目 → 断言面） |
|---|---|
| `full_flow_trio_rows_and_launch_shape` | **FakeRunner+FakeBridge 单题全流程**：三件套落盘（agent.diff=capture 文本 / eval.sh=task.eval_script 逐字节 / eval.log=容器输出）+ **18 键 legacy 行字段级断言**（含 eval_sha256 独立重算对账）+ **v1 行映射断言**（14 键、两轨 wall_ms 同源）+ AgentLaunch 形状（run_id `-a1`、user_message=prompt+模板、system 常量、采样覆写 0/1000/32000、policy 250/TASK_COMPLETE/3、bash 工具 workdir /testbed、预算 300/1/300）+ ExecOutput 扩展字段（verdict/record=v1 同源/evidence=三件套路径/合格终态工件）+ 桥调用序列（rm→run(6g/512)→capture×2(gate/verify)→cleanup rm） |
| `gate_no_patch_on_empty_diff` | **gate 分支①**：空 diff → `gate: "no_patch"`（judge 照跑、行照落） |
| `gate_test_edit_on_test_file_diff` | **gate 分支②**：diff 碰 tests/test_sampler.py → `gate: "test_edit"`；verdict 仍出自 judge（记账不是门控，§2.4） |
| `runner_failure_row_and_container_cleanup` | **runner 失败 → run_failed 行**（11 键字段级：实际目录/http_retries=5/失败文案）+ **容器 cleanup 已调**（RAII Drop 断言：末次桥调用 = `docker rm -f`）+ 判分链未启动（三件套零文件）+ Err=EX3001（订单侧不停终态） |
| `setup_failure_row_with_nulls` | **setup 失败 → setup_failed 行**（9 键、run_dir/eval_sha256 均 null）+ runner 零调用 + run 目录未建 + Err=EX3001 |
| `bash_tool_guards_archaeology_and_repeats` | **bash 工具行为**（main.rs:710-734）：反考古（git fsck）注入 `[harness] Searching for an upstream fix…` 前缀；同命令第 3 次 `[harness] You have issued this exact command 3 times.` 拦截、原输出保留；普通命令带 `exit=0` 无护栏；桥收到的 docker exec 构造（cd /testbed 前缀 + `__AF_RC` 标记 + 180s）逐串断言 |
| `unit_load_fingerprint_gate_fail_loud` | **装载期指纹门**：改字节脚本副本 → EX3002、桥零调用、runner 零调用（拒跑行为证据）；锚匹配装载成功 |
| `sink_write_failure_fails_loud` | 行写失败 → EX3001 上抛、sink 文案透传（不静默丢行）；判分链已完成（eval.log 在盘——先产证据后落行） |
| `failure_rows_key_sets_and_note_truncation` | 失败行键集（9/11）+ legacy 18 键 + v1 14 键；note 400 字符截断 300（main.rs:889 口径）；reward None → null |
| `pending_driver_sink_fails_loud` | 驱动占位 sink 双流误用即 Err（文案指向 P5 接线） |
| crate 内 `bash_guard_wrap_semantics` / `bash_guard_window_slides_after_six` | 护栏纯函数：透传/注入/第 3 次拦截计数；6 条窗口滑出后同名命令重新计数不误拦 |
| crate 内 `attempt_from_dir_follows_next_attempt_naming` / `gate_of_three_branches` | 目录名后缀解析（约定/回退）；gate 三分支（含纯空白与 tox.ini 命中） |

**注入手法**（零模型、零凭证、零真容器、零网络——资源纪律）：FakeRunner
（AgentRunner 缝，捕获 launch 全形状）+ FakeBridge（HostBridge 缝，按命令形状
智能分派：rm/run/capture/docker exec，setup 失败臂可注入）+ 假 docker（PATH
首段注入，verify 的 eval 执行链真子进程，p3_verify 手法）+ 假判分 venv
（p3_judge 手法）。本包测试即 Layer 1 对拍的"金种子"（方案 §5-P4 明文）。

## 3. 与方案的偏差（如实登记）

1. **两缝持有形态：`Arc<dyn … + Send + Sync>` 而非方案 §2.3 字面的 `&dyn`**。
   af-harness ToolRegistry 的工具函数体钉死 `'static + Send + Sync`
   （run.rs:53-55），bash 闭包必须整体可注册——桥与环境的 `&` 引用带不进
   注册闭包，单元改持 Arc（HostBridge 带 Send+Sync 界），`TbEnv` 补 `Clone`
   派生（行为无变化）。缝的可注入性不变（FakeBridge/FakeRunner 注入即为证）。
2. **AgentRunner Err 侧是结构化 `AgentRunFailure` 而非 §2.3 草图的 `String`**：
   run_failed 行的 http_retries 在失败路径也要取自 `opened.http_retries()`
   （main.rs:819/833），裸 String 会丢字段；草图签名语义保持（单入口
   `run(launch) -> Result<报告, 失败>`）。
3. **ExecContext 增表列之外的 `task_id` 字段**。§2.2 表列 5 字段未含题目标识，
   但同节判据明言"换题=换 ctx"——题目标识必须进 ctx（单元从
   `<data_root>/<task_id>.json` 装载）。按其自身判据补全，非另起设计。
4. **失败行（setup_failed/run_failed）只落 legacy 轨，无 v1 投影**。§1.4 映射
   表面向完整执行事实（ExecutionFacts），失败行不是 ExecutionFacts；验收草案
   也只要求 v1 断言于全流程。失败行 v1 化（如 status 直映 + verdict null）留
   作 P6 对拍口径的独立决定。
5. **任务装载/key 解析/Opened::open 失败从"批级 ? 中止无行"改为"失败行 +
   EX3001"**。af-bench 中 load 失败（main.rs:640）与 key/open 失败
   （main.rs:768/773 的 `?`）使整个 run_swev 中止、不落行；单元化后无法中止
   "批"（批循环在 P6 壳），按 §2.4"供给失败行由单元落、订单侧不停终态"分类
   落行（task 装载/key/open 属供给面，分别归 setup_failed/run_failed）。
6. **驱动层 ExecContext 接线为 P5**：driver.execute 以 `out_dir/runs` 占位
   attempt_dir + `PendingRecordSink`（误用 fail-loud）保类型完整；DriverConfig
   增 host_alias/data_root/task_id/trial 透传。驱动面现役单元（Stub）不消费
   这些字段；真实单元经驱动执行前必须先完成 P5 接线（方案 §5-P5 原文边界）。
7. **attempt 序号从 run 目录名后缀解析**（`attempt_from_dir`）：§2.2 把
   attempt 目录分配放 ctx（驱动层 = P5），而 run_id 需要序号（main.rs:778
   `-a{attempt}` 后缀）——按 next_attempt_index 的命名约定
   （`swev-<task>-<profile>-a<N>`）从目录名解析；非约定命名回退 1（fix2
   口径 attempt 恒 1，行内 attempts 字段本就不随目录序号变）。
8. **`issue_root` 移入 `HarnessRunner::run` 逐题签发**：`RootCredential` 未从
   af-credential 导出（无法作字段类型命名）；af-bench 本就每题一签
   （main.rs:681-686 在任务循环体内，issue_root 每次 new KeyPair、文档明言
   不跨重启保根）——粒度一致，非行为变化。harness 侧账面工单 =
   `WorkOrder::new(run_id)`（af-harness 只查 id 非空，run.rs:221-223）；
   bench_order 四步生命周期不搬（方案 §1.1）。
9. **gate 取证与 verify 各调一次 capture_diff**：af-bench 本就两次
   （main.rs:801 gate 用一次 + swev.rs:152 verify 内一次），迁移保持原样
   （行为等价；多一次桥调用成本与原版相同，未"顺手优化"——迁移纪律）。
10. **run_failed 行 scaffold 硬编码 "v3"**（main.rs:829 原样），而 18 键行
    scaffold 走 spec 字段——轻微不对称如实保留：af-bench 两处都内联硬编码，
    spec 化只做了完整行。
11. **RED→GREEN 期间测试修正 2 处**（测试自身缺陷）：bash 护栏测试的桥调用
    计数误写 4（实际 5 次：echo once/git fsck/dup×3）；crate 内窗口滑出测试
    初版手工构造 recent 状态错误（未模拟 wrap 内裁剪）。均已修，如实记。
12. **HarnessRunner 全真路径（真 key 解析/真 Opened/真模型调用）不进单测**：
    FakeRunner 注入即方案 §2.3 设计意图（单测零模型零凭证）；全真路径的
    行为等价由语句级对照清单 + P6 L2 端到端金丝雀承接。无新增 `#[ignore]`
    冒烟（真 local docker 冒烟维持 P2 既有 1 例）。
13. **clippy 零新增**；第三方 proc-macro-error2 v2.0.1 future-incompat 提示
    为 P1 已登记遗留（RESULT-P1.md §3-5），非本包新增。

## 4. 验证记录（本任务实跑，CARGO_TARGET_DIR=~/afbuild-phase2b，未占 af/target）

| 项 | 命令 | 结果 |
|---|---|---|
| 验收命令（任务书原文） | `cd /home/pan-ding/workspace/af-wt-phase2-p2 && CARGO_TARGET_DIR=~/afbuild-phase2b cargo test -p af-execline` | **全绿**（exit 0）：lib 18（P4 新增 4）+ lifecycle 8 + p1_comparison 9 + p1_suite 15 + p2_bridge 12（1 ignored）+ p3_judge 4（1 ignored）+ p3_verify 7 + p4_unit 10（P4 新增）+ doc 0 = **83 passed / 0 failed / 2 ignored** |
| RED 基线 | 同验收命令（RED 提交 b8c4e44 时点） | p4_unit 9 红/1 过（过的 pending_driver_sink 是桩期即真代码的占位 sink）/既有 lib 14+lifecycle 8+p1_comparison 9+p1_suite 15+p2_bridge 12（1 ignore）+p3_judge 4（1 ignore）+p3_verify 7 全绿不受影响 |
| clippy | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo clippy -p af-execline --all-targets` | exit 0，af-execline 自身 **0 警告**；仅第三方 proc-macro-error2 future-incompat 提示（§3-13） |
| fmt | `cargo fmt --check --all` | PASS（零 diff，exit 0） |
| cargo deny | `CARGO_TARGET_DIR=~/afbuild-phase2b cargo deny --offline check` | advisories / bans / licenses / sources 四类 ok（exit 0）——af-credential/af-meter/af-stop 入列无环实证（方案 §1.2-3） |
| 只读约束 | `git diff 4cba6cb..HEAD --stat -- crates/af-bench crates/af-workorder crates/af-engine` | 输出为空（0 字节，零改动） |
| 迁移源一致性 | `diff` 主仓 af/crates/af-bench/src/{swev,tb,main}.rs vs 本工作树 | 逐字节一致（IDENTICAL，开工与收尾两次实测） |
| git 序列 | `git log --oneline 4cba6cb..HEAD` | b8c4e44（RED）→ 941c40c（GREEN）→ 6df0f6b（重构）→ 本文件提交，红→绿→重构成立 |
| 资源纪律 | `df -h /`（开工/收尾） | 开工 27G 可用 → 收尾 26G 可用（>15G 线）；构建全走 ~/afbuild-phase2b；未碰其他工作树与主仓 af/target；零 docker/LLM/网络 |

# poc-s17-harness-gate — harness 版本级回归门（D-54）证据件

- **卡**：`s17-harness-regression-gate`（债 **D-54**，父债 P3-B1；wave-02 签发登记：
  `计划/阶段欠账台账.md:117`「未开始（wave-02 签发）」；占用闸已放行）
- **设计稿**：`30-工件/strategies17-供给面设计-20261106.md` §七.1（st-b-C108 跨面判据：
  「被防护对象是 harness 工程过程（版本回归集+发布阻断阈值），不是任何一次 agent
  运行图；AFB 的 policies 只约束本图执行」——`strategies/st-b-C108.yaml:13` na_reason 原文）
- **分支**：`feature/s17-harness-regression-gate`，基于 `origin/product/main`
  `04b5a5ea`（显式 refspec 取回）。代码提交 `6a4783b3`（9 文件，+1418/−19，零 .rs）；
  本证据件为后续提交。
- **跑批主机**：anolis（anolis-gpu-01），工作树
  `/data/bench/s17-harness-regression-gate-wt`（HEAD=6a4783b3，与 origin 分支同点），
  `CARGO_TARGET_DIR=/data/bench/targets/s17-harness-regression-gate`，
  `env -u <代理变量> LC_CTYPE=C.UTF-8` 前缀（红线口径）。原始读数另存于 anolis
  `/data/bench/s17-harness-evidence/`（本目录 `raw/` 为其逐字复制）。

## 一、交付物

| 件 | 路径 | 性质 |
|---|---|---|
| 新门 | `ci/harness_regression_gate.py` | 定义面（恒验 fail-closed）+ 读数面（劣化阈值阻断）+ `--selftest` 负例夹具 19 枚 |
| 回归集定义件（钉） | `ci/baselines/harness-regression-set.json` | 固定模型+seed+120 题任务集（sha256 钉）+每版双跑+双阈值+校准锚+provenance |
| 回归任务集（钉） | `ci/baselines/harness-regression-tasks.json` | 120 题（D1×60/D2×40/D3×20，12 仓），digest 双钉（定义件内 + gate-inputs） |
| 门数账四件 | `ci/subgates.list` / `ci/run.sh`（14e 接线）/ `numbers.json`（三键 4→5）/ `ci/baselines/gate-inputs.sha256`（重钉 16 件） | subgates.list 头注纪律的同批同步 |
| 判据正本造册 | `ci/gate_inputs_check.sh`（GATE_INPUTS +2）/ `ci/test_gate_inputs_rte6_pins.py`（CONSUMED +2） | 新增判据正本入族钉（读数台账是 append-only 可变账，不入钉面） |

门面语义（`ci/harness_regression_gate.py` 模块 docstring 逐条可核）：

1. **定义面（恒验，fail-closed）**：定义件必填全钉；任务集须在场、条目数==钉计数、
   sha256==钉摘要、instance_id 唯一；**空回归集（零条目）＝无判据 ⇒ 硬红**；
   **校准锚必须超过 resolve 阈值**（S130 实录回归 Δ6.9pp 拦不住 ⇒ 阈值弱化红，防「放宽
   阈值静默关门」）。
2. **读数面（有数才判）**：发布管线把每版读数追加进 `ci/baselines/harness-readings.jsonl`
   （append-only JSONL：release/model/seed/runs/resolved/total/tokens_per_task_k）。
   逐条核对口径一致（模型/seed/双跑/**任务集全覆盖——total 必须==120，子集跑分不得
   过门**；release 不得重号）；基线=首条、当前=末条；**resolve 率劣化 >5.0pp 或
   token/题 通胀 >15.0% ⇒ 硬红阻断发布**（S130 原文「任一劣化超过阈值即阻断」）。
   缺席：默认 INFO（首跑前正常态）；`--require-readings`（发布管线模式）下缺席即红；
   在场但零记录 ⇒ 恒红。
3. **selftest（--selftest，随 step14 14e 每次 CI 实跑）**：19 枚夹具全落临时目录
   （tempfile），子进程实跑 CLI、断言退出码与阻断读数文案，不触真仓任何树。

## 二、口径与阈值锚（全部可坐标）

来源 S130 = `harness策略调研/strategies.csv#line:131`（Qwen Code CLI，arXiv 2607.03691）：

- 「每次改动都跑固定回归集（同一批大仓库任务 + 固定模型 + 固定 seed），同时报
  resolved 率与 token/题 两条曲线，任一劣化超过阈值即阻断发布」（body.测试设计原文）；
- 35 个连续 release × 50 道分层 SWE-bench Verified × 每版两跑：resolve rate
  23.0%–39.0% 大幅波动、均值 30.5%、与版本序无显著趋势（Spearman ρ=0.208, p=0.231）；
  双跑任务级一致 87.7% ⇒ 波动归因 harness 改动而非采样噪声；
- **实录回归**：v0.4.1→v0.5.0（OpenAI converter，PR #1235，+346/−8 行）resolve rate
  39.4%→32.5%（Δ−6.9pp）——「这些回归没有任何现有自动化测试能捕获」；
- token：前 9 版约 391K/题 → 最近约 668K/题（+70%，ρ=0.743, p<0.0001）；最新版相对
  per-task 基线最多 +19.6%。

本定义件的钉值（选型依据见定义件 provenance）：

| 钉 | 值 | 锚 |
|---|---|---|
| model | `Qwen3-Coder-30B-A3B-Q4_K_M-local-llama.cpp` | 计划/50-bench实测方案.md §2.2 S1 投放（Qwen3-Coder-30B 本地 × 120 SWE-V） |
| task set | 120 题（`计划/s1-swev-120.json` 逐字节复制，digest `af217e4a…`） | 同上；D1×60/D2×40/D3×20、12 仓 |
| seed | 20261029 | 计划/研究组/B线核心批-20261029/experiments-v1.json EXP-1 randomization.seed（本项目既有 seed 钉先例） |
| runs_per_release | 2 | S130 双跑口径（87.7% 任务级一致） |
| resolve_degradation_pp | 5.0 | 必捕获实录回归 6.9pp；窗口内单版跌幅 ≤5pp 视为 23.0–39.0% 历史摆动带内正常波动 |
| token_degradation_pct | 15.0 | 必捕获实录 +19.6%；低于 35 版累积 +70%（非单版事件） |

## 三、anolis 跑批读数（工作树 @6a4783b3）

点名门集（D-56 先例「五门」= anolis-build 验收面 + 本卡触门的 step14 兄弟件）。
**全量十四步 `bash ci/run.sh` 在基线 04b5a5ea 上 step0 即退出**（mutations 探针清单钉
漂移，预存债——见 §五 对照实测），故按波次先例跑点名门集；`raw/run-full.log` 为该次
全量尝试的 step0 实录。

| # | 门（命令） | rc | 关键读数 |
|---|---|---|---|
| P1 | `python3 -B ci/harness_regression_gate.py` | **0** | 定义面在役：模型/seed 20261029/120 题/每版 2 跑/阈值 5.0pp+15.0%；读数面待首跑（INFO） |
| P2 | `python3 -B ci/harness_regression_gate.py --selftest` | **0** | 19/19 夹具全绿（raw/py-gates.txt） |
| P3 | `bash ci/gate_inputs_check.sh check` | **0** | 判据正本钉对账一致 16 件 |
| W1 | 14e 接线行原型（`gate && gate --selftest`） | **0** | 与 `ci/run.sh:660` 逐字同型（raw/w1-14e-wiring-line.txt） |
| G1 | `cargo fmt --check` | **0** | 零 diff |
| G2 | `python3 -B ci/strategies_check.py` | **0** | 61 条目 compilable=44 / not_applicable=17（与基线一致，策略库零回归）；R5 跨树一致（基线 6a4783b） |
| G3 | `cargo test -p af-compiler -p af-dsl` | **0** | 全过 |
| G4 | `cargo test -j4 --workspace` | **0** | 全过（0 failed） |
| G5 | `cargo clippy --workspace --all-targets -j4` | **0** | 退出 0（告警不阻原始 clippy） |
| G5c | clippy **门**（run.sh step2 同型：`--message-format=json` + `ci/clippy_gate.py`） | 1 | **预存红**：+2 `clippy::collapsible_if` `crates/af-execline/src/span_produce.rs`（现 2/基线 0）——见 §五 |
| G6 | `python3 ci/branches_check.py` | **0** | 登记 10 分支；红项 0 |
| G7 | `python3 ci/xcheck.py`（缺省参数） | 1 | anolis 上 FileNotFoundError：`--agent-md`/`--plan-doc` 缺省指向本机工作区文书（xcheck.py:540 等），anolis 无此文件——环境性，预存（raw/g7b-g5b.txt） |
| G7c | `python3 ci/xcheck.py --agent-md … --plan-doc …`（两件工作区文书复制上岛） | 1 | **subgates-roster-binding PASS**（名册 5 件==run.sh step14 接线集合，含 harness_regression_gate）；packages/redlist 两项预存红（同 §五）；obs-dates 因陪衬文书未全复制而红（环境性） |
| G8 | `python3 ci/criteria_check.py --emit-checklist …` | **0** | 六要素 schema 过 |
| G9 | `python3 ci/step12_check.py …` | 1 | **名册三臂 ==5 一致 + 名册集合==接线集合 OK**；registry.pkgs/adr.count 两红为预存（同 §五） |
| G10 | `bash ci/gate_inputs_check.sh check` | **0** | 16 件一致 |

## 四、负例夹具（验收①②直证）

`raw/negative-readings.txt`（anolis 实跑，定义件=仓内真件、读数=注入夹具）：

| # | 场景 | 期望 | 实跑 |
|---|---|---|---|
| N1 | 注入 resolve 劣化：55.0%（66/120）→45.0%（54/120），Δ10.0pp > 5.0pp | BLOCK | rc=1：「劣化阻断：resolve rate 基线 55.0%（#1 af-harness@baseline-sha）→ 当前 45.0%（#2 af-harness@injected-regression-sha），劣化 10.0pp > 阈值 5.0pp——S130 口径『任一劣化超过阈值即阻断发布』」 |
| N2 | 注入 token/题 劣化：391K→480K，+22.8% > 15.0% | BLOCK | rc=1：「劣化阻断：token/题 基线 391.0K → 当前 480.0K，通胀 22.8% > 阈值 15.0%——S130 口径『两条曲线任一劣化即阻断』」 |
| N3 | 阈值内波动：55.0%→53.3%（Δ1.7pp）、token +2.3% | PASS | rc=0：「读数面：劣化 1.7pp ≤ 阈值 5.0pp（token/题通胀 2.3% ≤ 阈值 15.0%）——未触发阻断」 |
| N4 | **空回归集**（任务集零条目） | fail-closed BLOCK | rc=1：「task_set.count 须为 >0 整数（实 0）——零条目回归集无判据」+「空回归集：任务集在场但零条目——回归集未定义即无判据，fail-closed 阻断」 |

`--selftest` 19 枚夹具（raw/py-gates.txt P2 段）另覆盖：任务集计数漂移红/摘要漂移红/
定义缺 seed 红/阈值弱化红/读数口径漂移（模型）红/子集跑分红/记录不可解析红/release
重号红/空读数台账红/`--require-readings` 缺席红/读数缺席默认不拦/**S130 实录回归
39.4%→32.5%（Δ6.9pp）必拦**/窗口内波动 30.5%→28.0%（Δ2.5pp）不拦/真仓定义件 rc=0。
夹具件（demo 读数与空集定义）逐字存 `raw/fixtures/`。

## 五、基线预存红（非本卡所致；pristine 对照实测）

`raw/base-control/pristine-base-04b5a5ea.txt`：以 `git archive 04b5a5ea` 抽取零改动
基线树实跑，四项红在本卡分支上逐字复现，在基线上同样成立：

| 红 | 读数 | 归属 |
|---|---|---|
| run.sh step0 mutations | 探针清单漂移：期望 13 条/哈希 `188e157a…`，实得 13 条/哈希 `881781d9…`（`mutations/registry.yaml` vs `ci/baselines/mutations-registry.sha256`） | 2026-11-03 一致性收缩批改 registry.yaml 后未同批重钉（numbers.json mutations.probes 键 11→13 同批改账，钉文件漏改） |
| step12 registry.pkgs | 登记 37 vs 实算 82（`af/packages/components.json` 根级 pkg.*） | E9001 供给批 37→82 后 numbers.json 未同步（键 last_recomputed 2026-10-28） |
| step12 adr.count | 登记 14 vs 实算 15（docs/adr） | ADR 新增后未同步 |
| xcheck packages | README 主张十包 37 件 vs components.json 实测 82 件（24 个新命名空间无文书分解） | 同上 E9001 代际 |
| xcheck redlist | 方案 §1.3 主张 23 项 vs `ci/neutrality-expected-fail.txt` 实测 24 行（多 `af-compiler::registry_decl_test::all_e9001_declarations_resolve_with_builtin_impl_ref`） | E9001 供给批红名单 +1 后工作区陪衬文书（计划/研究组/G4-任务中立性机器检查方案.md）未同步 |
| clippy 门 | +2 `collapsible_if` `span_produce.rs`（现 2/基线 0） | 基线钉于 1f5f4de3（2026-10-19）；span_produce.rs 改于 6ef49c80（2026-11-05，P3-A2 单根 trace）；两者均为基线祖先，本卡零 .rs（`git diff 04b5a5ea 6a4783b3 --name-only` 实测 0 个 .rs）⇒ 读数与基线逐位相同 |

本卡触门（名册三臂、xcheck 子门级绑定、gate_inputs 族钉、14e 门面+selftest、
strategies_check 三态、branches/criteria）在基线上与本卡分支上同为绿——即本卡只做
增量、不制造新红，也不吸收既有红。

## 六、边界与如实登记

1. **读数台账未首跑**：`ci/baselines/harness-readings.jsonl` 缺席（定义面 INFO）。
   首跑属 anolis-build 重轨（固定模型+120 题+seed 20261029 跑两遍的容器/LLM 跑批），
   超出本 local-api 卡范围；落账后本门读数面自动生效，发布管线以
   `--require-readings` 强制（缺席即红）。
2. **全量十四步管道在基线上不可过**（§五 step0 即退出）：本卡按波次先例跑点名门集；
   基线的四项预存红建议由对应代际批（E9001 供给批/2026-11-03 一致性批）重钉了账，
   不在本卡 scope（判据件零弱化纪律）。
3. **anolis 上 xcheck 缺省参数不可跑**（G7）：`--agent-md`/`--plan-doc` 缺省指向本机
   工作区绝对路径（xcheck.py:540 等），anolis 无该文书——预存环境缺口（D-56 五门集
   不含 xcheck 故此前未暴露）。本卡以 G7c（两件工作区文书复制上岛）取得名册绑定读数。
4. **本机一次意外 cargo 触发（红线，如实登记）**：本地预跑 `ci/branches_check.py` 时
   未带 `--test-list-file`，该脚本自跑 `cargo test --workspace -- --list`
   （`ci/branches_check.py:117`）——本机出现一次 cargo 调用（target/ 为既有已构建态，
   `--list` 未触发全量编译；10:05 后无 rlib/rmeta 新写出）。本卡结论不依赖本机 cargo
   读数：anolis 侧 G1–G6 全门重跑为正。此后不再本机跑该门。
5. **判据件零弱化**：`ci/strategies_check.py`/`ci/criteria.yaml`/既有门脚本零触；
   本卡只新增门 + 门数账四件同批 + 两件判据正本造册入族钉。

## 七、复跑入口

```bash
# anolis（红线口径）
ssh anolis 'cd /data/bench/s17-harness-regression-gate-wt && \
  export PATH=$HOME/.cargo/bin:$PATH && \
  export CARGO_TARGET_DIR=/data/bench/targets/s17-harness-regression-gate && \
  env -u http_proxy -u https_proxy -u all_proxy -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY \
    LC_CTYPE=C.UTF-8 python3 -B ci/harness_regression_gate.py && \
  env -u http_proxy -u https_proxy -u all_proxy -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY \
    LC_CTYPE=C.UTF-8 python3 -B ci/harness_regression_gate.py --selftest'
# 注入劣化演示（夹具见 raw/fixtures/）
python3 -B ci/harness_regression_gate.py --readings <demo.jsonl> --require-readings
```

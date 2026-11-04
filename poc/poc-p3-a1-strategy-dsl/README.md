# POC · P3-A1 策略→DSL 整合（59 簇三态清单 + 可编译者过 CI + 闭环最小接缝）

- 基线：`origin/product/main` 钉死提交 **56ff52a**（工作树 `wt/p3-a1-strategy-dsl`，
  detach，无分支指针移动；远端 ref 会话期间已前移 6 提交，见 §五边界）。
- 本批唯一仓面改动：`strategies/st-b-C102.yaml`（dsl_snippet 内 `passed`→`pass`
  字段名对齐，2 行；三态不变）。`crates/`、`Cargo.toml/lock`、`ci/`、`tools/`、
  `docs/`、`af/`（含 af/packages 生产注册表）零改动。
- 工件索引：`three-state-list.json`（机器可读）/ `three-state-list.md`（人读表）/
  `raw/`（原始读数）/ `seam/`（最小接缝工程）/ `run/`（执行读数现场）。

## 一、现状读准

**格式契约**（`strategies/README.md` §一，亲读）：每条目必填 `id/name/tier/claim/
claimed_value/sources/consensus/experiment/dsl_state`；`dsl_state` 三态闭集
**`compilable | needs_syntax | not_applicable`**，判据：

| 三态 | 必填 | 互斥（须缺席或 `-`） | 判定语义 |
|---|---|---|---|
| compilable | `dsl_snippet` | needs_syntax_note、na_reason | snippet 须过 `af_dsl::load_str`+`validate` 零违例 |
| needs_syntax | `needs_syntax_note` | dsl_snippet、na_reason | 语法缺表达手段，note 写缺什么+扩展示例 |
| not_applicable | `na_reason` | dsl_snippet、needs_syntax_note | 策略本质非图结构语义 |

**门规则**（`ci/strategies_check.py` 亲读，挂 `ci/run.sh` step14 段尾 14d，run.sh:596
以 `python3 -B` 调用）：

- **R0 入库对账**（硬红）：`strategies/` 全库 + `crates/af-dsl/tests/
  strategy_library_test.rs` 必须 git 跟踪；在场未跟踪=红（防 checkout 灭失绿灯掩盖）。
- **R1 schema**（硬红）：逐条目必填键、枚举闭集（tier∈{A,B,C,D}，C/D 须
  experiment.status≠none；dsl_state 三态；experiment.status 五态）、三态↔字段互斥、
  `id`↔文件名 stem 一致、id 正则 `^st-(b-)?C[0-9]+$`。
- **R2 index 完备**（硬红）：磁盘条目必须全登记、登记 file 必须在场、index 的
  tier/dsl_state 与条目文件实测一致（双向）。
- **R3 计数对账**（硬红）：`counts.{total,tiers,dsl_states}` 登记值 vs 按 index 机算值
  逐键相等；缺非零键（登记面缩水）亦红。
- **R4 编译断言**（硬红）：先过本树 provenance 门（防他树残留测试二进制假绿），
  再 `cargo test -p af-dsl --test strategy_library_test`（全库逐条 load_str+validate，
  compilable 零违例）；非零退出=红。`--skip-compile` 非常跑形态，输出如实声明未执行
  （RT-E81 修复后无假陈述）。**全部规则均 fail-closed，无 advisory 档。**

**基线实跑**：`python3 -B ci/strategies_check.py` → **exit 0 全绿**
（`raw/strategies_check.baseline.txt`）：R0 被测对象 66 件全跟踪；命名空间 A 线
13 条（=A 档 11+C 档 2）+B 线 48 条全登记；计数 total=61 tiers{A:11,B:48,C:2}
dsl_states{compilable:32, needs_syntax:12, not_applicable:17}。
装载测试逐条读数（`raw/strategy_library_test.nocapture.txt`）：**32 条 compilable
全 COMPILE-OK，12 条 NEEDS-SYNTAX，17 条 NOT-APPLICABLE，零失败**。

**61 vs 59 差**：台账「59 簇」= libA A 档 11 + libB B 档 48（`index.yaml` batch 头
自述）。多出的两条是 **tier C 实验承载档回写条目**：`st-C122`（rank 70，EXP-1 终裁
inconclusive 回写）、`st-C103`（rank 81，EXP-2 回写）——自我改进循环把实验终裁自动
建条目回写（v1.2 契约修订收编 tier 闭集扩 A|B|C|D，strategies_check.py:58-62）。
以 `index.yaml` 为全集正本（61 条）；目录实测 `st-*.yaml` 亦 61 件（另有 2 份 `.bak`
审计面历史，非条目，未删未动）。两口径一致，差异仅在台账统计口径取 A+B。

## 二、三态清单（61 条，逐条判定依据）

- 机器可读：`three-state-list.json`（每条：id/file/tier/topic/rank/dsl_state/
  in_59_clusters/basis/experiment_status；分布与 59 簇差结构化）。
- 人读表：`three-state-list.md`（61 行逐条：rank/id/tier/topic/三态/判定依据/59 簇内
  外标记）。生成器 `build_three_state_list.py`（依据三源：compilable=编译门实测
  COMPILE-OK 行 + snippet 行数；needs_syntax=note 摘录；not_applicable=na_reason
  摘录；生成前断言 index↔文件 dsl_state 一致）。
- 分布：compilable 32 / needs_syntax 12 / not_applicable 17；59 簇外 =
  {st-C122, st-C103}（C 档回写，均 needs_syntax，claimed_value 待投影）。

## 三、可编译者逐条过 CI 读数

**定义门（14d 编译断言）**：`cargo test -p af-dsl --test strategy_library_test
-- --nocapture` → **exit 0，32/32 COMPILE-OK，零失败**（`raw/
strategy_library_test.nocapture.txt` 逐条行）。**无因此改三态的条目（零条）。**

**修掉的失败（1 处，执行面暴露）**：seam 推进到执行面时 st-b-C102 被 EX4006 证伪
——check 节点 builtin `pkg.sweng.test_unit` 产出的 verdict envelope 键为 `pass`
（sop_full@1.1.afb:53 与 A 档 st-C006/C008/C016/C018 同用 `pass`），而该条目
test_verdict 类型声明 `passed`。**修法：snippet 内 `passed`→`pass` 对齐正本契约**
（st-b-C102.yaml 4 行 diff，三态不变仍 compilable），修后 14d 门重跑仍全绿
（`raw/strategies_check.after-edit.txt`）。**同型漂移登记未修**（另有别的阻断，
不宜在本批顺手改）：st-b-C036:47、st-b-C071:46、st-b-C097:47 同用 `passed`。

**四段编译链深度读数**（seam `compile-all`，对生产注册表 af/packages，
`raw/compile-all.prod-registry.json`）：32 条 compilable 中 **8 条过全链**
（load+validate+lower+bind+plan：st-C005/C012/C016/b-C003/b-C037/b-C048/b-C055/
b-C056），**24 条断在 bind（E9001 组件未注册）**——策略 snippet 引用的
`pkg.cohesion.*`/`pkg.aci.*`/`route.retrieval|critic|scorer|chooser|weak|mid|strong`
等 26 件组件不在注册表（共 63 条声明）。此为**注册表覆盖缺口**（执行面供给不足），
非 snippet 语法错误——按契约定义它们仍是如实的 compilable，本批不以此改任何三态。

## 四、闭环断点读数 + 最小接缝设计与实测

**闭环四环**：① 策略（知识，strategies/）→ ② DSL 可编译（af-dsl load+validate，
14d 门）→ ③ 被跑（af-compiler 四段→af-execline 图执行器）→ ④ 回流（实验终裁回写
建条目，EXP-1/EXP-2→st-C122/st-C103 已存在）。

**断点实测（grep 全仓正本）**：消费 `dsl_snippet` 的只有装载测试与 14d 门——
**环③整环缺失**：零「snippet→plan」接线、零执行接线；图执行器只消费
`af/packages/*/graphs/*.afb` 包内图。且三重供给缺口使 32 条 compilable 今天一条
都跑不起来：(a) 全部 32 条引用注册表缺失的 prompt 模板（`pkg.*_system` 生产面
只有 8 份，条目引用 0 命中）；(b) 24 条 bind 即断（组件未注册）；(c) 预算/
构造面：节点级 cents 预算（st-b-C037，EX4005 实测拒收）、tool_calls 维度
（st-b-C035/C036）、loop/map 构造（12 条）、route 边守卫（st-b-C071）、
builtin 表外工具（st-C019 pkg.aci.* 六件）均超 v1 执行器强制集/驱动范围。

**最小接缝**（`seam/`，standalone cargo 工程——空 `[workspace]` 表自成根，path
依赖仓内 crates 正本，不入 workspace、零改根 Cargo.toml/lock）：

1. `compile-all`：61 条逐条四段链 stage 级读数（§三已引）。
2. `run-one`：单条 compilable——四段全链编译（覆盖层注册表）→ plan.json+task.json
   任务包（判据面/计划面 sha256 钉死，W1-C59/W2-C55 语义）→ **真
   `GraphExecuteUnit::execute`**（注入 `ScriptRunner` 零模型：终态消息 JSON=envelope
   回退源；`FakeBridge` 零容器：docker 面脚本化）→ 读数落 `FileRecordSink`。
3. 覆盖层注册表（`make_overlay_registry.py`）：af/packages 全量拷贝 + 仅补缺失供给
   （route.strong/route.mid 声明、pkg.coder.* 4 份与 pkg.explore/pkg.sweng 2 份
   模板）；components.json 声明零改动 ⇒ registry_digest 与生产正本逐位一致；
   **生产 af/packages 零改动**。

**实测跑通（st-b-C102「CodeR 五角色任务图」）**——`raw/run-one.st-b-C102.json`：

- 编译链 5 段全 pass，plan_id `plan.coder_role_task_graph@1.0+573fcbaaa072`。
- 执行 **exit ok，verdict=true，gate=ok**：4 个 task 节点按拓扑序真驱动
  （analyze：GLM-5.3 → locate：GLM-5.3-Flash[sweng.locate] → patch_role：
  GLM-5.3[sweng.edit] → review：GLM-5.3-Flash），check 节点 verify 走
  `pkg.sweng.test_unit` builtin 直调（判据命令随任务包冻结）。
- 读数回流：`run/out/records.jsonl`（af.plan.record.v1 行：status=completed、
  verdict=true、usage turns=12/unit_calls=20、wall_ms=40、note
  `graph nodes=5 cents=20 tokens_total=640`）+ `run/attempt/artifacts/` 5 份
  typed envelope 工件 + `run/attempt/nodes/verify/{test.log,test-evidence.json,
  agent.diff}` 判分证据树（diff sha256 留档）。
- 保留的如实负读数：`raw/run-one.st-b-C037.json`——st-b-C037（唯一线性但带节点级
  cents 预算）被 EX4005 拒收「预算声明在强制集外」，证明该策略语义本体（阶段独立
  预算）需执行器扩展方可回流，非 snippet 可修。

## 五、门读数与边界

| 门 | 命令 | 读数 |
|---|---|---|
| fmt | `cargo fmt --check --all` | exit 0 |
| test | `cargo test --workspace --no-fail-fast` | 见 `raw/gate-test-workspace.txt`（三元组+exit） |
| clippy | touch 全部 .rs 后 `cargo clippy --workspace --all-targets` | 见 `raw/gate-clippy.txt`；`ci/clippy-baseline.txt` sha256 `a48639e7…326ba4` 与基线逐位一致 |
| gate_inputs | `bash ci/gate_inputs_check.sh check` | exit 0（12 件判据正本钉对账一致；本批未改被钉件，无需重钉） |
| 专属门 | `python3 -B ci/strategies_check.py` | exit 0 全绿（改 st-b-C102 前后各跑一次，`raw/strategies_check.*.txt`） |
| 零弱化 | `git diff --stat 56ff52a` | 仅 `strategies/st-b-C102.yaml | 4 ++--`；crates/ 0、Cargo.toml/lock 0、crate 数 22→22（`raw/zero-weakening.txt`） |

**门禁放宽：零**（未改 `ci/strategies_check.py`，无重钉需求）。

**fail-open 缺口探查（登记，未动手）**：对 strategies_check.py 逐条探查候选——
R0 非 git 树形态跳过（INFO，语义如实）、R2 index↔磁盘双向对账、R3 计数双向、
R1 枚举/互斥/stem、R4 exit 码语义、--skip-compile 诚实化（RT-E81 已修）——
**未复现可夹具化的 fail-open 洞**。一条观察项（非门洞，审计面事项）：R3 只对账
登记↔机算一致，不锁库总量下限——「缩库+同步改计数」可一致通过，防缩水靠 git
历史审计面（R0 已保证被测对象入库可审计）。

**边界（没做的、做不到的）**：
1. `python3 -I ci/strategies_check.py` 在本机 exit 2——`-I` 隔离态排除用户
   site-packages，而 PyYAML 装在 `~/.local/lib/python3.11/site-packages`；仓内
   正本挂法是 `-B`（run.sh:596），`-B` 形态全绿。环境限制，如实登记。
2. **origin/product/main 会话期间前移 6 提交**（f489601 等，别批合流）——零弱化核
   以任务书钉死的 56ff52a 为基线；对现 ref 的 diff 面 52 文件全部为远端新提交内容，
   非本批改动（`raw/zero-weakening.txt` 尾注）。
3. 24 条 bind 断、模板 0 命中、执行器 v1 范围（loop/map/route 守卫/节点级 cents
   预算/builtin 表外工具）均为**跨批供给面工程**，本批只量化+登记，不顺手扩
   （扩 af/packages 或 graph_unit.rs 均越只许改清单）。
4. seam 是最小一次性接缝（命令行单发，无守护/监听/服务）；产物零凭证正文
   （覆盖层 route 声明镜像生产正本的 key_ref 引用名，非明文密钥）。
5. st-b-C036/C071/C097 的 `passed` 同型漂移登记未修（各有别的阻断，修了也跑不起）。

## 六、指纹

- 基线：`56ff52a57c022983d74d994fd4e1c63e98f45235`（HEAD，detach）。
- 本批仓面 diff：`strategies/st-b-C102.yaml` 仅 4 行（passed→pass ×2 处）。
- 执行读数锚：plan_id `plan.coder_role_task_graph@1.0+573fcbaaa072`；record 行
  verdict=true / wall_ms=40 / nodes=5；verify diff sha256
  `3ee41db3fc36fa58da2ae1dddb9d3994346fbe9ecb9047cc3176859f7a06222a`。
- clippy-baseline sha256：`a48639e75b5203fef7e1ffc7026b8f0d9de20f3ce862fa3988a1ec6b49326ba4`（=基线）。
- raw/ 清单：strategies_check.{baseline,after-edit}.txt、strategy_library_test.
  nocapture.txt、compile-all.prod-registry.json、run-one.st-b-C037.json（负读数）、
  run-one.st-b-C102.json、records.jsonl、gate-fmt.txt、gate-test-workspace.txt、
  gate-clippy.txt、gate-inputs.txt、zero-weakening.txt、clippy-baseline.sha256、
  seam-build.log。
- 运行环境：anolis 工作树 wt/p3-a1-strategy-dsl；CARGO_TARGET_DIR=
  /data/bench/targets/p3-a1-strategy-dsl（/data 盘，独占）。

# POC · P3-A1 策略→DSL 整合（59 簇三态清单 + 可编译者过 CI + 闭环最小接缝）

- 基线（2026-11-05 v2 重植后实况，原 salvage 态叙述见文末存照）：父提交=
  `59403d1`（poc-e 合流提交）；本分支=`feature/a2-p3a1-12clusters-v2`（已
  push origin，HEAD=e9557aa 及其后证据同步提交）；远端 `origin/product/main`
  现已前移到 `04f4d99`（A1/A4 合流后）。零弱化核以 59403d1 为基线。
- 本批仓面改动（相对 56ff52a，排除 poc/）——【存照：重植前口径】：
  1. `strategies/st-b-C102.yaml`：dsl_snippet 内 `passed`→`pass` 字段名对齐
     （2 处，三态仍 compilable）；
  2. `strategies/README.md`：**只增** §四（闭环接缝指针），既有 §一–§三零改。
- `crates/`、`Cargo.toml`/`Cargo.lock`、`ci/`、`tools/`、`docs/`、`af/packages`
  生产注册表零改动。`.bak` 审计面历史未删。
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

**门规则**（`ci/strategies_check.py` 亲读，挂 `ci/run.sh` step14 段尾 14d，
`run.sh:596` 以 `python3 -B` 调用）。全部规则 fail-closed，无 advisory 档：

- **R0 入库对账**（硬红）：`strategies/` 全库 + `crates/af-dsl/tests/
  strategy_library_test.rs` 必须 git 跟踪；在场未跟踪=红。非 git 树形态 INFO 跳过。
- **R1 schema**（硬红）：逐条目必填键、枚举闭集（tier∈{A,B,C,D}，C/D 须
  experiment.status≠none；dsl_state 三态；experiment.status 五态）、三态↔字段互斥、
  `id`↔文件名 stem 一致、id 正则 `^st-(b-)?C[0-9]+$`。
- **R2 index 完备**（硬红）：磁盘条目必须全登记、登记 file 必须在场、index 的
  tier/dsl_state 与条目文件实测一致（双向）。
- **R3 计数对账**（硬红）：`counts.{total,tiers,dsl_states}` 登记值 vs 按 index 机算值
  逐键相等；缺非零键（登记面缩水）亦红。
- **R4 编译断言**（硬红）：先过本树 provenance 门，再
  `cargo test -p af-dsl --test strategy_library_test`（全库逐条 load_str+validate，
  compilable 零违例）；非零退出=红。`--skip-compile` 非常跑形态，输出如实声明未执行。

**当刻实跑**：

- `python3 -B ci/strategies_check.py` → **exit 0**（`raw/strategies_check.rerun-B.txt`
  与 `raw/strategies_check.after-edit.txt`）：R0 被测对象 66 件全跟踪；A 线 13 条
  + B 线 48 条全登记；计数 total=61 tiers{A:11,B:48,C:2}
  dsl_states{compilable:44, needs_syntax:0, not_applicable:17}。
- `python3 -I ci/strategies_check.py` → **exit 2**（`raw/strategies_check.rerun-I.txt`）：
  isolated 模式排除用户 site-packages，本机 PyYAML 在
  `~/.local/lib/python3.11/site-packages`。仓内正本挂法是 `-B`。
- 装载测试逐条（`raw/strategy_library_test.nocapture.txt`）：**44 条 COMPILE-OK，
  0 条 NEEDS-SYNTAX，17 条 NOT-APPLICABLE，零失败**。12 簇
  （st-C103/C122、st-b-C001/C028/C029/C038/C062/C064/C079/C086/C124/C125）
  已由 NEEDS-SYNTAX 转为 COMPILE-OK。

**61 vs 59 差**：台账「59 簇」= libA A 档 11 + libB B 档 48（`index.yaml` batch 头
与 `strategies/README.md` §二自述）。多出的两条是 **tier C 实验承载档回写条目**：
`st-C122`（rank 70，EXP-1 终裁 inconclusive 回写）、`st-C103`（rank 81，EXP-2 回写）。
v1.2 契约修订收编 tier 闭集扩 A|B|C|D（`strategies_check.py:58-62`）。
全集正本 = `index.yaml` 61 条；目录实测 `st-*.yaml` 亦 61 件（另 2 份 `.bak`
审计面历史，非条目，未删未动）。两口径一致，差只在台账取 A+B。

## 二、三态清单（61 条，逐条判定依据）

- 机器可读：`three-state-list.json`（每条：id/file/tier/topic/rank/dsl_state/
  in_59_clusters/basis/experiment_status；分布与 59 簇差结构化）。
- 人读表：`three-state-list.md`（61 行逐条：rank/id/tier/topic/三态/判定依据/59 簇内
  外标记）。生成器 `build_three_state_list.py`（compilable=编译门实测 COMPILE-OK
  + snippet 行数；needs_syntax=note 摘录；not_applicable=na_reason 摘录）。
- 分布：compilable 44 / needs_syntax 0 / not_applicable 17；59 簇外 =
  {st-C122, st-C103}（C 档回写，均已 COMPILE-OK）。

## 三、可编译者逐条过 CI 读数

**定义门（14d 编译断言）**：`cargo test -p af-dsl --test strategy_library_test
-- --nocapture` → **exit 0，44/44 COMPILE-OK，零失败**
（`raw/strategy_library_test.nocapture.txt`）。12 簇 NEEDS-SYNTAX→COMPILE-OK。

COMPILE-OK 44 条：st-C005/C006/C007/C008/C009/C010/C012/C015/C016/C018/C019/
C103/C122、st-b-C001/C003/C028/C029/C035/C036/C037/C038/C042/C048/C055/C056/
C057/C060/C062/C064/C069/C071/C077/C078/C079/C086/C092/C095/C096/C097/C102/
C109/C111/C124/C125。

**修掉的失败（1 处，执行面暴露）**：seam 推进到执行面时 st-b-C102 被 EX4006 证伪
——check 节点 builtin `pkg.sweng.test_unit` 产出的 verdict envelope 键为 `pass`
（sop_full@1.1.afb:53 与 A 档 st-C006/C008/C016/C018 同用 `pass`），而该条目
test_verdict 类型声明 `passed`。**修法：snippet 内 `passed`→`pass` 对齐正本契约**
（st-b-C102.yaml 4 行 diff，三态不变仍 compilable），修后 14d 门重跑仍全绿。
**同型漂移登记未修**：st-b-C036:47、st-b-C071:46、st-b-C097:47 同用 `passed`
（各有别的执行面阻断，修了也跑不起，本批不顺手改）。

**四段编译链深度读数**（seam `compile-all`，对生产注册表 af/packages，
`raw/compile-all.prod-registry.json`）：44 条 compilable 中 **10 条过全链**
（load+validate+lower+bind+plan：st-C005/C012/C016/C103/b-C003/b-C037/b-C048/
b-C055/b-C056/b-C124），**34 条断在 bind（E9001 组件未注册）**。此为注册表覆盖缺口，
非 snippet 语法错误——按契约定义它们仍是如实的 compilable，本批不以此改任何三态。
12 簇新 compilable 中 st-C103 与 st-b-C124 过全链，其余 10 条断在 bind。

## 四、闭环断点读数 + 最小接缝设计与实测

**闭环四环**：① 策略（知识，strategies/）→ ② DSL 可编译（af-dsl load+validate，
14d 门）→ ③ 被跑（af-compiler 四段→af-execline 图执行器）→ ④ 回流（实验终裁回写
建条目，EXP-1/EXP-2→st-C122/st-C103 已存在）。

**断点**：消费 `dsl_snippet` 的只有装载测试与 14d 门——**环③整环缺失**。图执行器
只消费 `af/packages/*/graphs/*.afb` 包内图。三重供给缺口使 44 条 compilable
今天一条都跑不起来（对生产注册表）：(a) 全部 44 条引用注册表缺失的 prompt 模板；
(b) 34 条 bind 即断（组件未注册）；(c) 预算/构造面：节点级 cents 预算（st-b-C037，
EX4005 实测拒收）、tool_calls 维度、loop/map 构造、route 边守卫、builtin 表外工具
均超 v1 执行器强制集。

**最小接缝**（`seam/`，standalone cargo 工程——空 `[workspace]` 表自成根，path
依赖仓内 crates 正本，不入 workspace、零改根 Cargo.toml/lock）：

1. `compile-all`：61 条逐条四段链 stage 级读数。
2. `run-one`：单条 compilable——四段全链编译（覆盖层注册表）→ plan.json+task.json
   → **真 `GraphExecuteUnit::execute`**（注入 `ScriptRunner` 零模型：终态消息
   JSON=envelope 回退源；`FakeBridge` 零容器）→ 读数落 `FileRecordSink`。
3. 覆盖层注册表（`make_overlay_registry.py`）：`af/packages` 全量拷贝到
   `run/registry/`，再补缺失供给（prompt 模板 + 根 `components.json` 追加
   `route.strong`/`route.mid` 两条，按 name 排序）。根清单 44→46 条，
   **registry_digest 与生产正本不同**（多两档路由声明）。**生产 `af/packages`
   零改动**。

**实测跑通（st-b-C102「CodeR 五角色任务图」）**——`raw/run-one.st-b-C102.json`：

- 编译链 5 段全 pass，plan_id `plan.coder_role_task_graph@1.0+573fcbaaa072`。
- 执行 **exit ok，verdict=true，gate=ok**：4 个 task 节点按拓扑序由 ScriptRunner
  驱动（analyze / locate / patch_role / review；模型档位来自计划绑定的
  GLM-5.3 / GLM-5.3-Flash，零真实模型调用），check 节点 verify 走
  `pkg.sweng.test_unit` builtin 直调。
- 读数回流：`run/out/records.jsonl`（af.plan.record.v1：status=completed、
  verdict=true、usage turns=12/unit_calls=20、wall_ms=40）+
  `run/attempt/artifacts/` 5 份 typed envelope +
  `run/attempt/nodes/verify/{test.log,test-evidence.json,agent.diff}`。
  verify diff sha256
  `3ee41db3fc36fa58da2ae1dddb9d3994346fbe9ecb9047cc3176859f7a06222a`。
- 保留的如实负读数：`raw/run-one.st-b-C037.json`——st-b-C037 被 EX4005 拒收
  「预算声明在强制集外」（节点级 cents），策略语义本体需执行器扩展方可回流。

## 五、门读数与边界

| 门 | 命令 | 读数 |
|---|---|---|
| fmt | `cargo fmt --check --all` | **exit 0**（`raw/gate-fmt.txt`） |
| test | `cargo test --workspace --no-fail-fast` | **exit 101**；三元组 passed=**1577** / failed=**1** / ignored=9（158 套件；`raw/gate-test.summary.txt`） |
| clippy | touch 323 个 `crates/**/*.rs` 后 `cargo clippy --workspace --all-targets -j8 --message-format=json` | json **exit 0**，error 级 **0** 条，warning 级 0 条；`python3 ci/clippy_gate.py` **PASS**（0 签名 / 基线 0 签名）；`ci/clippy-baseline.txt` sha256 `a48639e75b5203fef7e1ffc7026b8f0d9de20f3ce862fa3988a1ec6b49326ba4` 与钉逐位一致 |
| gate_inputs | `bash ci/gate_inputs_check.sh check` | **exit 0**（12 件判据正本钉对账一致；本批未改被钉件，无需重钉） |
| 专属门 | `python3 -B ci/strategies_check.py` | **exit 0** 全绿；`python3 -I` **exit 2**（缺 PyYAML，见边界） |
| 零弱化 | `git diff --stat 56ff52a`（排除 poc/，存照） | `strategies/README.md` +18、`strategies/st-b-C102.yaml` 4 ±；crates/ 0、Cargo.toml/lock 0、crate 数 22→22 |

**test 红因（本批未改 crates，禁改既有测试）**：唯一失败
`af-execline --test p3_judge`
`judge_invoke_argv_contract_no_env_injection_no_wrapper_shell`
（`crates/af-execline/tests/p3_judge.rs:164`）。断言「子进程环境 = 父环境 +
两道判分防线变量」。本执行器 shell 把 `which` 导出为 bash 函数，环境里带
`BASH_FUNC_which%%`，值含换行；测试把 env dump 按行 `key=value` 解析，换行把
函数体拆碎导致 HashMap 不等。摘录 `raw/gate-test.p3_judge-fail.txt`。单测复现
exit 101（`raw/p3_judge.rerun.txt`）；诊断态 `env -u 'BASH_FUNC_which%%'` 同测
exit 0（`raw/p3_judge.rerun-no-bashfunc.txt`）——**不是**任务书命令，只说明红因。
本批不改 crates、不改测试、不把诊断态写成门绿。

**门禁放宽：零**（未改 `ci/strategies_check.py`）。

**fail-open 观察项（登记，未动手收紧）**：R3（`ci/strategies_check.py:351-368`
`cmp_counts`）只对账登记↔机算一致，不锁库总量下限。夹具
`raw/fail-open-r3-nofloor/`：仓外拷贝树只留 1 条 not_applicable + 同步
counts，`--skip-compile` → **exit 0**（`raw/fail-open-r3-nofloor/run.txt`）。
空库仍被 `main()`「无条目 YAML」硬红。缩库须走 git 历史；R0 保证被测对象入库
可审计。本批不顺手加下限（产品决策）。

**边界**：
1. `python3 -I` 本机 exit 2（PyYAML 在用户 site-packages）；正本挂法 `-B` 全绿。
2. `origin/product/main` 现已前移到 `04f4d99`；本分支基线 59403d1 与其文件集
   不相交（A1/A4 只动 crates/otel/poc-a1，本批只动 strategies/与本 poc），合流无冲突预期。零弱化以 59403d1 为基线。
3. 34 条 bind 断、模板缺席、执行器 v1 范围均为跨批供给面工程，本批只量化+登记。
4. seam 是一次性命令行接缝（无守护/监听/服务）；覆盖层 route 声明镜像生产正本
   的 `key_ref` 引用名，非明文密钥。
5. st-b-C036/C071/C097 的 `passed` 同型漂移登记未修。
6. 未改 `docs/`（证据在 poc/ + strategies/README §四）。

## 六、指纹

- 基线：`59403d1c27c447c290cf09cfd43ab293ccf975dc`（poc-e 合流提交=v2 重植父提交；原 56ff52a 系重植前任务书钉线，存照保留）。
- 工作树 HEAD（v2 实况）：`e9557aae1594c966803777940b968ba0d5eb0e1e`（salvage 存档 a429e6f 系重植前父，存照）；
  本地分支 `feat/p3-a1-strategy-dsl`（无 commit）。
- 仓面 diff vs 56ff52a（排除 poc/，存照口径；v2 实况以 59403d1 为基见上）：`strategies/st-b-C102.yaml` sha256
  `11ef78343fa565ed7d5e304ef7d776e122d9302b9dd41807bf05d4c800e0e1f9`；
  `strategies/README.md` sha256
  `bfec6b9f86fc10eb5eb049b1a7a168925907037535ab04689b014b23a8d2bd3a`。
- 执行读数锚：plan_id `plan.coder_role_task_graph@1.0+573fcbaaa072`；record 行
  verdict=true / wall_ms=40 / nodes=5；verify diff sha256
  `3ee41db3fc36fa58da2ae1dddb9d3994346fbe9ecb9047cc3176859f7a06222a`。
- clippy-baseline sha256：`a48639e75b5203fef7e1ffc7026b8f0d9de20f3ce862fa3988a1ec6b49326ba4`。
- 运行环境：anolis 工作树 `wt/p3-a1-strategy-dsl`；CARGO_TARGET_DIR=
  `/data/bench/targets/p3-a1-strategy-dsl`（/data 盘，独占）；
  HTTPS_PROXY=`http://<内网地址>:7890`（grok 通道出口，未改）。


### gate-clippy.gate.txt 捕获口径（2026-11-05 回补批注）

该件为**手工捕获件**（非 CI 产物；== AF CI 管道开始 2026-11-05 23:14:16 root=/data/bench/wt-clippy-gateshape
step0 FAIL: mutations 探针清单漂移（删/改探针未走 update-registry-baseline 同批评审） step2 只打控制台不落 raw/）。再捕获时必须
连门退出码一并落盘——否则再生即丢机读字段（2026-11-05 e9557aae 曾因只重定向 stdout
丢掉  行，本批按历史 blob  形状回补）：



先例： 的  即同模式（json 重定向+exit 追加）。

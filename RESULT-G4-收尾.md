# RESULT · G4 收尾批 —— sop 三图物化 + 15 稿快照终态核 + 中立性再收敛 + 裁决登记

> 施工批：G4 收尾实现员（单树 af-wt-phase2-execline，分支 phase2@5b8b41e 八门绿基线）。2026-10-25。
> 依据：《G4-施工裁决-20261025.md》+ 编排者收官指令。TDD/零回归纪律；一切结论亲跑命令复核。

## 0. 一页结论

| 项 | 结果 |
|---|---|
| sop 三图物化 | `sweng/graphs/{sop_full,sop_no_qa,sop_minimal}@1.0.afb` 落盘（selector 时代「声明无源」销项）+ 新模板 ×3（planner/engineer/reviewer_system.md）入 `MANIFEST.sha256`；三图干跑编译 exit 0 ×3、stderr 空、双跑 cmp 逐字节一致，产物入档 `ci/out/g4/` |
| registry_digest 前后 | **逐位不变**：`63c3478e9321e1c58318620a592e9953ef34cee3a5d19ea4c3300a722fafe15b`（物化前 fix_swe_task 实测 = 物化后三图+两包图复测；digest 只盖 components.json 声明集——`components.rs:287-294`，图源/模板不进装载面） |
| 15 稿快照终态核 | 本树 registry 重跑 15/15 exit 0，与 `计划/研究组/对齐计划快照/*.plan.json` **cmp 15/15 逐字节全等**（plan_id 逐字一致；registry_digest 行已是当前值 63c3478e…）——快照零改动即终态，无双态 |
| 中立性 | N3 登记表再核登记（值维持 63c3478e…）；词表机械源②再生成 **41→46 词**（+reflectkit 补登、+reportkit/paper/web_fetch/pdf），白名单零新增（新词核心 src 实测 0 命中）；step8 独立重跑 **12 判据全 PASS exit 0** |
| 测试/门 | `cargo test -j4 --workspace` **1198 passed / 0 failed**；`bash ci/run.sh` **八门全绿 exit 0**（fmt/clippy/test/cue/scrub/cosign/deny/neutrality） |

## 1. sop 三图物化（裁决书 §五-17 铺单批前置；F-第四单多 agent 载体）

形态遵循 fix_swe_task 同款 AFB 写法（线性主链——v1 装订器解释范围）+ 包内
prompts/template_ref 机制（裁决10）；组件引用全部在 63 条既有声明内解析：

| 图 | 节点链（kind/uses） | 边 | 输出 | plan_id |
|---|---|---|---|---|
| sop_full | planner(task, route.plan)→coder(task, route.main + pkg.sweng.locate/view/edit，workspace 唯一写者)→reviewer(task, route.judge)→tester(check, pkg.sweng.test_unit) | 5 | qa_report from tester | `plan.sweng_sop_full@1.0+7dd9c5aa253b` |
| sop_no_qa | planner→coder（去 reviewer/tester 环节） | 1 | list<code_file> from coder | `plan.sweng_sop_no_qa@1.0+0b751294ef7e` |
| sop_minimal | repair(task, route.main + pkg.sweagent.env)单节点 | 0 | patch from repair | `plan.sweng_sop_minimal@1.0+e5753d57d7ba` |

- 干跑命令：`~/afbuild-phase2i/debug/af-compiler-plan --registry af/packages
  af/packages/sweng/graphs/<名>@1.0.afb` → **exit 0 ×3、stderr 空、双跑 cmp
  逐字节一致**；plan 落盘 `ci/out/g4/{sop_full,sop_no_qa,sop_minimal}.plan.json`
  （承 fix_swe_task/paper_report 入档先例，commit 12c3197）。
- 模板：`sweng/prompts/{planner,engineer,reviewer}_system.md` ×3 新增并登记
  `MANIFEST.sha256`（闭集测试 `sweng_prompt_manifest_is_closed_over_files_on_disk`
  绿）；`sop_minimal` 复用既有 `pkg.sweng.swev_system@1`（单修复节点=fix_swe_task
  fix 节点同面，零新模板）。
- 绑定复核（计划 node_exec 亲读）：coder=route.main+sweng 三工具（persist/pure×2）、
  planner=route.plan、reviewer=route.judge、tester unit=`pkg.sweng.test_unit@1`、
  repair tools=`pkg.sweagent.env@1`（external_once）。
- **多 agent 差异化机理如实登记（复核观察 3a）**：三 route（plan/main/judge）在
  components.json 声明中绑定**同一底层模型 GLM-5.3、同一端点、同参数**
  （reasoning off/非流式——根级 route.* 七条同端点面，P2-L12 扩容登记）；
  三 agent 的差异化靠 system 模板（planner/engineer/reviewer_system）/
  工具面（sweng 三件 vs 零工具）/预算（node 级 turns）实现，**非异构模型**。
  『多 agent』判定的成立面=角色分化（独立节点+独立提示词+独立工具面+typed
  工件交接），模型异构化属档位面（route 声明扩展），后者非本批口径。
- 新增测试 `sop_graphs_materialized_with_resolvable_template_refs`
  （pkg_sweng_package.rs，与资产同 commit——TDD 语义）：三图在盘 + template_ref
  全解析；pkg_sweng_package 6/6 绿。
- README 同步：`af/packages/README.md` prompts 段枚举补 SOP 三模板 + 图源段
  （声明↔在盘闭合说明）。

## 2. registry_digest 前后对比（预期不变——实核不变）

```
物化前  fix_swe_task 干跑    = 63c3478e9321e1c58318620a592e9953ef34cee3a5d19ea4c3300a722fafe15b
物化后  sop_full            = 63c3478e…afe15b（逐位同）
物化后  sop_no_qa           = 63c3478e…afe15b
物化后  sop_minimal         = 63c3478e…afe15b
物化后  fix_swe_task 复测   = 63c3478e…afe15b
物化后  paper_report 复测   = 63c3478e…afe15b
```

机理：`ComponentRegistry::load` 只读根级+包级 components.json
（`components.rs:209-285`），digest=全量声明规范序列化 SHA-256——图源 .afb、
prompts/*.md、MANIFEST.sha256 均不在装载面。**无 15 稿快照联动更新需求
（无双态）**。

**档存件对齐（复核观察 1 处置，2026-10-25 复核后补）**：`ci/out/g4/
fix_swe_task.plan.json` 为 Phase 1 时点档存（digest `d507bf61…`，57 条面），
落后 63 条终态——已重刷为当前注册面新鲜干跑产物（plan_id 不变、digest=
`63c3478e…`、双跑 cmp 逐字节一致）；刷新缘由与 Phase 1 历史数值登记于
`ci/out/g4/dryrun-and-digest-note.md`。`paper_report.plan.json` 与 sop 三件
档存复核 cmp 已逐字节同相（无需动）。

## 3. 15 稿快照终态核（裁决8；逐位核对登记）

命令：`af-compiler-plan --registry af/packages crates/af-compiler/testdata/src/<名>.yaml`
对 15 稿逐一重跑（本树 af/packages 注册面），产物与
`计划/研究组/对齐计划快照/<名>.plan.json` `cmp`——**15/15 逐字节全等**
（exit 0 ×15；plan_id 逐字一致；registry_digest 行均=当前值 63c3478e…，无需
改写即终态）。逐份 plan_id：

```
ADAS         plan.adas_mas_search@1.0+bae4333cfa67     AFlow        plan.aflow_search@1.0+8ba8217f03d6
Agensh-DeLM  plan.agentsh_delm_harness@1.0+8ef971fa81d6 AutoGen     plan.autogen_team@1.0+6f06546b62c7
CAMEL        plan.camel_roleplay@1.0+afdfb924fde3      CrewAI       plan.crewai_yaml_crew@1.0+071cd8c7a0e9
GPTSwarm     plan.gptswarm_candidate_graph@1.0+16f9891c3bef LangGraph plan.langgraph_stategraph@1.0+7c9f5946956c
MemGPT       plan.memgpt_layered@1.0+416f7afc92ff      MetaGPT      plan.metagpt_sop_pipeline@1.0+c09249e849a8
Multi-Agent-Debate plan.multi_agent_debate@1.0+e7b1b0479afb ReAct   plan.react_interleaved@1.0+c3c19f4c8140
Reflexion    plan.reflexion_episodic@1.0+040f2d168ee2  SWE-agent    plan.sweagent_aci@1.0+afab59b9cf32
Voyager      plan.voyager_curriculum@1.0+c3aa4b53250d
```

（MetaGPT 稿 topology_selector 候选 `pkg.sweng.sop_full/no_qa/minimal@1.0`
自本批起「声明且有源」——候选核对面仍只看 manifest 声明集，行为不变。）

## 4. 中立性 N3+词表（裁决19 移交项执行）

- **N3 登记表再登记**：`ci/neutrality-golden-registry.tsv` 头注补 G4 收尾批
  再核行（digest 逐位不变复测 + 15/15 cmp 全等）；登记值维持
  d5f3bd3a… → **63c3478e…**（63 条终态，Phase 2 已登记，本批复核维持）。
- **词表再生成**（机械源②=components.json+manifest 图名再拆段；G4 新组件/
  新模板纳入考量）：黑名单 **41→46 词**——补登 `reflectkit`（pkg.reflexkit.*
  既有段，历史收敛正本漏登）+ 新增 `reportkit`（包段）/`paper`
  （pkg.reportkit.paper_report 图段）/`web_fetch`（pkg.report.web_fetch，同
  web_search ③④ 先例）/`pdf`（pkg.report.pdf_extract 截段）。不入表（收敛
  判定登记于词表头注）：`report`（rule-1 核心机制词）、`render/doc/notes/
  extract/fetch/web` 与新模板名段 `planner/engineer/reviewer/reader/writer/
  system`（通用英语词剔除规则）。
- **白名单重收敛**：新增 5 词核心 src 实测 0 命中（`grep -rniE` 亲测）→ 白名单
  零新增、零失效；`ci/neutrality_scan.py` 实跑 S1=PASS（黑名单 46 词；命中
  14 处=白名单豁免 14/白名单外 0/失效条目 0）。
- **step8 重跑**：独立实跑 `bash ci/neutrality_gate.sh` → **PASS=12 FAIL=0
  exit 0**（Z0-pre/C1/I1/I2/I3/I4/P-A/P-B/P-C/S1/R7/Z0；I3 集合精确相等 19 项
  无漂移；R7 21 份 IR golden 闭集）；词表 SHA-256
  `70970efcceebee93e5757ef67c683895c3c3ec9725e710f5f5b5bbbdf831b72c`（随门日志
  冻结落盘）。

## 5. 裁决登记（三句，编排者已批）

1. **--plan 档 v1 = 约定路径 `<data_root>/<task>/plan.json`**（编排者批准；
   CLI `--plan` flag 留 v0.2 再定——承 RESULT-G4-reportkit §2.4/§5-2 的
   开放问题 8 收口）。
2. **planbind / --unit 已终态**（复核员 8/8 已验——E9004→EX4004 唯一消费
   正本 planbind.rs + `--unit swev|report|stub` 路由，无遗留施工项）。
3. **impl_ref 6 件预留形与判分脚本实际落点统一归 G4+ 真包化**（编排者接受；
   承 RESULT-G4-reportkit §5-3——6 件 reportkit impl_ref 预留包路径形态与
   `af/packages/reportkit/{units,tools}/` 实住位的统一，随 sweagent 件真包化
   同批处理）。

## 6. 门与收官记录（本批实跑）

- `CARGO_TARGET_DIR=~/afbuild-phase2i cargo test -j4 --workspace`：**1198
  passed / 0 failed**（ci/out/step3.log 权威计数）。
- `bash ci/run.sh`：**八门全绿 exit 0**（step1 fmt 596ms / step2 clippy 458ms /
  step3 test 9904ms / step4 cue 59ms / step5 scrub 237ms / step6 cosign 135ms /
  step7 deny 641ms / step8 neutrality 2925ms；ci/out/SUMMARY.txt）。
  计时为单跑采样，跑次间正常波动（独立复核 2026-10-25 12:17 亲跑八门亦
  全绿 exit 0，计时 579/294/9989/36/234/70/547/2882ms——两组数值均登记，
  结论以全绿为准，不以下沉到毫秒的门时为判据）。
- commit + push origin phase2（本批单 commit，含全部资产/登记/门日志）。

## 6.1 独立复核记录（2026-10-25 12:17，复核员）

八门亲跑全绿 exit 0；step3 计数 1198/0 独立复算一致；digest 抽 3 稿
（ADAS/MetaGPT/SWE-agent）整文件 cmp 快照 3/3 逐字节全等；五图干跑
exit 0×5、四件档存 cmp 逐字节一致；63 条声明清点（根 44+adaskit 2+
autogen 5+reportkit 6+swarmkit 1+sweng 5）、MANIFEST 6/6 闭集、词表 46 词条
+SHA `70970efc…` 门内一致、N3 21 行含收尾再核头注——全部与本文登记相符。
三条如实观察（档存 fix_swe_task digest 落后/门时波动/三 route 同模型）
已逐项处置于 §2（档存重刷）、§6（计时波动登记）、§1（差异化机理登记）。

## 7. 范围外（如实）

- sop 三图的真实模型执行端到端 = 铺单批 F-第四单（多 agent）——本批止于
  可执行计划 + 干跑验证（裁决 §五-17 口径）；**『多 agent 形态成立』的判定
  面止于声明+编译计划层**（角色分化见 §1；真实模型下的运行时行为不在本批
  证据面——复核观察 3b 如实保留）。
- proofs/ 目录为仓内既有未跟踪目录（非本批产物），未纳入本批 commit。

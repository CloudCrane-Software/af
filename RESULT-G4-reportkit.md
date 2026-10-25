# RESULT · G4 Phase 2 —— 研究报告配置包 reportkit v1 施工交付

> 施工批：G4 施工员（单树 phase2-g4；Phase 2 于合流主线 phase2@e0836dd 后收官——
> 裁决16/§五）。2026-10-25。正本依据：《G4-施工裁决-20261025.md》+ 修订后
> 《G4-研究报告包设计-v1.md》（施工修订 v1.1）。TDD 先红后绿；六位点终态
> 同步与包落地同 commit；15 稿快照 digest 行更新逐位登记（裁决8 G4 终态）。

## 0. 一页结论

| 项 | 结果 |
|---|---|
| 交付 | reportkit 包 v1（第 5 包：manifest+paper_report@1.0 图+pkg.report.* 6 件入册+prompts/MANIFEST+双判分脚本）+ planbind.rs（E9004→EX4004 **唯一消费者**，裁决9）+ ReportExecuteUnit + af-order `--unit swev\|report\|stub` 路由（裁决7）+ citations 五字段 required（裁决4）+ sources 工件口记账（裁决5） |
| 图干跑 | plan_id = **plan.paper_report@1.0+13a1799adc04**、registry_digest = **63c3478e9321e1c58318620a592e9953ef34cee3a5d19ea4c3300a722fafe15b**（63 条终态）、双跑逐字节一致；reader artifact_contract 携 `sources`（provenance.envelope_of=source_snapshot）——裁决5「进图记账、不进 store」闭环 |
| 六位点终态 | 组件 57→**63**（bind_test+registry_decl_test）、package_count 4→**5**（registry_test:261/bind_test:66/golden_test）、图闭包 14→**16**（白名单 +fix_swe_task +paper_report）——**全部与包落地同 commit（5c887a1）** |
| 15 稿快照（裁决8） | 重跑 15/15 恰 `registry_digest` 一行变化（d5f3bd3a…→63c3478e… **全同**）、plan_id 逐字不变；**快照文件已更新**（字节级单串替换，备份 /tmp/g4r-snapbak）且与重编译产物 cmp 逐字节全等；合流农田修复后复验 15/15 仍逐字节全等 |
| 测试 | `cargo test -j4 --workspace` **1197 passed / 0 failed**（合流后）；新增 pkg_reportkit 14 + pkg_reportkit_package 5 + af-order unit_route 5 |
| 七门 | fmt/clippy（0 警告）/test/cue/scrub/cosign/deny = **7/7 PASS exit 0**（§6） |
| 主线 | phase2 前进至 e0836dd（农田修复批合入）——已 merge（f63e700，冲突仅 ci/out 产物），终门全部在合流后跑（§6） |

## 1. 包落地（C2' 5c887a1）

- `reportkit/manifest.json`：第 5 包，`pkg.reportkit.paper_report@1.0` 在册。
- `components.json`：6 件（设计 §3 照抄——tool 3：web_search/web_fetch external_once、
  pdf_extract pure；unit 2：citation_unit/rubric_unit；memory 1：mem.notes；
  impl_ref 包路径预留形 `packages/reportkit@1.0.0/...`——判分脚本**实际住包内**
  `af/packages/reportkit/{units,tools}/`，由 ReportJudgeLocator 定位（判分固定
  本机先例），登记差在 §5 如实说明）。model_route 零新声明（route.main 复用，
  设计 §2.7——档位差异化走单元 spec 覆写，裁决11）。
- `graphs/paper_report@1.0.afb`：设计附录 A 逐字节采用（191 行）+ reader 增
  `emits sources` 工件口（附录 A 注记的施工项兑现）；citations required =
  **[n,url,quote,snapshot_sha256,snapshot_ref] 五字段**（裁决4，计划 types 段
  断言在案）。
- `prompts/{reader_system,writer_system}.md + MANIFEST.sha256`（裁决10 统一
  机制，与 sweng 同一解析器一套；tests 断言 pkg.reportkit.* 可解析+版本门）。

## 2. 执行面（C1' 18005d3 + C3' 7e18b03）

### 2.1 planbind.rs（E9004 执行面消费唯一正本——裁决9）

装载期四核对，任一不符 fail-loud：①E9004→**EX4004**（`ComponentRegistry::load`
重算 digest ≠ 计划携带值；文案首 token `E9004` + 双 digest 点名）；②信封
v==1 拒绝式 decode；③一致性（graph_version 三面/line 绑定/node_exec 与
graph.nodes 按 id 对齐——**EX4005** 开位）；④模板装载（裁决10 机制汇流，
EX4001-4003 透传）。负例实测：篡改注册表一条（model 字段）→ 拒启动
E9004/EX4004；恢复即过。ReportExecuteUnit 复用之，**零第二套 digest 核对**。

### 2.2 ReportExecuteUnit（suite/report_unit.rs）

分相（§5.4，零容器）：setup（任务包+planbind 三核对）→ ingest（快照副作用
落 `attempt_dir/sources/` + 摄取面登记——裁决5「谁写、写哪」）→ draft
（**DraftFn 缝**）→ gate（快照/引用计数，记账非门控）→ verify（citation+
rubric 双子进程，判定只取 stdout JSON）→ 行落盘双轨（record.rs 复用，
suite=report，工件资格=双 passed）。装载三重门：判分双指纹锚 + profile +
模板引用（EX3002/EX4001-4003 fail-loud）。

**分级（裁决17）**：本批=装载链+快照面+判分链+行落盘全真实现（14 用例）；
**真实模型环=铺单批 F-第五单**——DraftFn 为缝（`draft_fn_unwired` 如实报
「不在批内」，不假装配）；web 摄取面（arXiv 出网/代理/robots，§10-6）与
PDF 抽取质量（§10-7）同为 F-第五单实测面，LocalFileFetcher 承载本地/file://。

### 2.3 判分链（§5.2；裁决6）

- `units/citation_unit.py`（argv 恰三位置参数 [task.json, report.json,
  sources_dir]）：marker_align 双向对齐/quote_hit 规范化空白子串/sha_match/
  snapshot_present/citations_total≥min——零 LLM、零网络。
- `units/rubric_unit.py`（argv 恰两位置参数）：R1 章节闭集/R2 章节下限/R3
  paper.sha256 对 task 登记/R4 引用数+quote 长度 ∈[20,600]/R5 report.md 与
  同一渲染器（`tools/render.py`）重渲染一致。
- 指纹门：`ReportJudgeLocator` 双锚（**0c34b595…/af63af2c…=落盘实测**，spec
  与脚本**同 commit** 原子生成——裁决6）；装载期+verify 入口双道，不符
  **EX3002 拒跑**。
- **判别力五反例**（判据 6/9，全部实跑）：篡改 quote 一字符→仅 quote_hit
  红；删一快照→仅 snapshot_present 红；正文加未登记 [9]→仅 marker_align
  红；quote 过短→仅 R4 红；**篡改判分脚本一字→EX3002 拒跑**——各恰触发、
  无串扰（tests/pkg_reportkit.rs 逐断言）。

### 2.4 af-order `--unit` 路由（裁决7；C3' 7e18b03）

`--unit swev|report|stub`（缺省=现行为：三件套在→swev、缺→stub——
exec_contract.rs 既有断言**零破**，14/14 原样绿）；swev/report 需三件套、
stub 与三件套互斥（OR1001）。report 臂：任务包材料（v1 约定
`<data_root>/<task>/{task.json,plan.json}`——**submit 级 `--plan` 档形态=
开放问题 8，本批不预判**；registry_root 取 task.json 声明，缺省机器本地
常量）+ ReportExecuteUnit 装载即过三重门。5 用例先红后绿。

## 3. 六位点终态同步账（裁决§二，验讫）

| 位点 | G4 前 | 终态 | 同 commit |
|---|---|---|---|
| af-dsl registry_test.rs:261 package_count | 4 | **5** | 5c887a1 |
| af-dsl registry_test 图闭包（白名单常量） | 14 | **16**（+fix_swe_task+paper_report） | 5c887a1 |
| af-compiler bind_test:66 package_count | 4 | **5** | 5c887a1 |
| af-compiler bind_test:68 图计数 | 14 | **16** | 5c887a1 |
| af-compiler bind_test 组件计数 | 57 | **63** | 5c887a1 |
| af-compiler registry_decl_test 组件总量 | 57 | **63**（测试名保留历史口径——中立性预期红名单按名引用） | 5c887a1 |

golden_test package_count 4→5 同批。DECLS_50 表计数保留 P2-L12 历史闭包
（50 名类计数 5/28/13/4 不动——G4 增量只入总量断言）。

## 4. 15 稿 digest 联动（裁决8，G4 终态）

- 更新前核对：15/15 重编译（63 条注册面）与快照 diff **恰
  `compiled_from.registry_digest` 一行**（`d5f3bd3aa0f7…d57977` →
  `63c3478e9321…afe15b`，15 份**全同**），plan_id 与其余字段逐位不变。
- 快照文件更新：`计划/研究组/对齐计划快照/*.plan.json` 15 份 digest 行
  （黑灯工厂正本，字节级单串替换；**备份 /tmp/g4r-snapbak**）；更新后与
  重编译产物 `cmp` **15/15 逐字节全等**。
- 合流复验：merge 农田修复（af-dsl 词法/校验变更）后重跑——15/15 仍逐字节
  全等、paper_report/fix_swe_task 干跑产物不变（E2205 output.ref 新判仅
  掐中本批测试夹具的既有形状缺陷，修正后全绿；两包图正本零触碰）。

## 5. 范围外与如实登记

1. **真实模型执行端到端**（reader→writer 环+MES 真写+arXiv 真摄取）=铺单批
   F-第五单（裁决17）——DraftFn 缝/web 面/PDF 质量三缺口如上。
2. **`--plan` 档形态**（CLI flag vs OrderSpec 字段）=开放问题 8 仍待裁决；
   v1 以「计划随任务包」约定承载，不预判两形态。
3. **impl_ref 预留形与实住位差**：6 件 impl_ref 照抄设计 §3 的
   `packages/reportkit@1.0.0/...` 预留形；判分脚本实际住
   `af/packages/reportkit/{units,tools}/`（locator 定位）。两者统一归 G4+
   真包化（与 sweng 预留形同先例）。
4. **N3 登记表联动**（中立性方案 §3.2 的 d5f3bd3a→新值再登记）：Phase 3
   落 `ci/neutrality-golden-registry.tsv` 时以本 RESULT §4 值（63c3478e…）
   为准。
5. **词表再生成**（裁决19）：reportkit/report/citation/rubric 等段入黑名单
   的机械源②再生成——Phase 3 移交项（方案 §5-8）。
6. `DEFAULT_REGISTRY_ROOT` 机器本地常量（judge.rs production 同款 R2 口径）；
   F-第五单任务包显式声明 registry_root。

## 6. 七门复跑记录（合流后终态）

`bash ci/run.sh` @ merge f63e700：**7/7 PASS，exit 0**——step1 fmt / step2
clippy+基线门（0 警告 0 签名）/ step3 `cargo test -j4 --workspace` 1197/0 /
step4 cue / step5 scrub / step6 cosign 断网验签 / step7 cargo deny。产物
`ci/out/`（含 g4/ 干跑归档：paper_report.plan.json + 两包 digest 核对记）。
merge 冲突面=ci/out 产物文件（双方各跑过门），取主线侧后由本次复跑重生成。

## 7. 提交清单（Phase 2 本批）

| commit | 内容 |
|---|---|
| 18005d3 | feat(execline): planbind（E9004→EX4004 唯一消费者）+ ReportExecuteUnit + 双判分器（锚=落盘实测同 commit）+ 14 用例 |
| 5c887a1 | feat(packages): reportkit v1 落地 + 六位点终态同步（同 commit）+ 5 包面用例 |
| 7e18b03 | feat(order): --unit swev\|report\|stub 路由（缺省不变）+ 5 用例 |
| f63e700 | merge phase2@e0836dd（农田修复批；冲突仅 ci/out 产物） |
| （本件） | RESULT-G4-reportkit.md + ci/out 七门产物与干跑归档 |

分支 `phase2-g4` → `origin/phase2-g4`（推送即达）。Phase 1 交付见
RESULT-G4-sweng.md（sweng 包 v1：15 件真实现 + fix_swe_task 干跑）。

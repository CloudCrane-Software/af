# 终验清单（criteria.yaml 机生成——禁止手写，复盘 PP2：手写清单即 W1-C06 总账失真载体）
# 源：/home/pan-ding/workspace/af-wt-g0merge/ci/criteria.yaml；schema 校验过后方可生成；清单条目数==criteria 条目数

## gate G4（4 判据）
| # | 判据 | 分子 | 分母 | 正本 | 无数据=未达成 |
|---|---|---|---|---|---|
| 1 | 软件工程包 sweng v1 真实现（manifest+图×4+组件声明+prompts/template_ref+MANIFEST.sha256+15 件分级实现） | sweng 包在位交付件数（manifest 1+图 4+组件声明+prompts 6+MANIFEST.sha256+15 件分级实现登记） | 同上清单全量件数 | af/packages/sweng/*（manifest/components/prompts/graphs）；证据=RESULT-G4-sweng.md | 是 |
| 2 | 研究报告包 reportkit v1 真实现（paper_report 图+pkg.report.*×6+citations 五字段 schema+快照落盘归属+planbind 消费） | reportkit 六件组件（web_search/web_fetch/pdf_extract/citation_unit/rubric_unit/mem.notes）实现在位数 | 6 | af/packages/reportkit/components.json（pkg.report.* 六件）；证据=RESULT-G4-reportkit.md | 是 |
| 3 | 各 1 个真实端到端任务（sweng=F4D scikit-learn-10908；reportkit=F5c arXiv 2610.31351） | 完成端到端任务数（在径 verdict=true） | 2 | af-state 订单档案 F4D/F5c + MES/CH 读回记录 | 是 |
| 4 | 任务中立性机器检查常设化（CI step8：N1 删包自洽+N2 领域词扫描白名单外 0 命中+N3 golden 登记） | step8 中立性门判据通过数 | 13 | ci/out/step8.log + ci/neutrality-expected-fail.txt（23 项）+ ci/neutrality-wordlist.txt | 是 |

## gate G0（2 判据）
| # | 判据 | 分子 | 分母 | 正本 | 无数据=未达成 |
|---|---|---|---|---|---|
| 1 | CI 机制批落地（复盘 §5.1 P0 子集：deliverables.yaml/step10-12/branches.yaml/xcheck/criteria.yaml 等按计划 §2.1 表） | 计划 §2.1 表已合流入 ci/run.sh 并带 mutation 探针的机制数 | 12 | 计划/第三阶段开发计划-20261027.md §2.1 表 ↔ af/ci/run.sh step 序列 | 是 |
| 2 | 现状开口先行修复（计划 §2.2：report_unit.rs:1112/1262 未饱和 cast、ci/run.sh:338 手写基线 32、:357 ADVISORY 两态死代码） | 三开口已修并附回归证据数 | 3 | 计划/第三阶段开发计划-20261027.md §2.2 ↔ AF crates/af-execline/src/suite/report_unit.rs + ci/run.sh | 是 |
- 观察窗 G0-OBS-1: 旧入口退役观察期跨天复核（义务收口）（采集面 5 项；期满 期满（2026-10-29 01:05:52）打完成标记并自停定时器；in_window 参与判定）

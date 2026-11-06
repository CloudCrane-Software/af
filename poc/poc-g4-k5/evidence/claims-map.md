# evidence/claims-map.md —— README 逐条断言的证据/边界对应表（G4 返修批）

口径：**实证结论**=有亲跑读数或源码/仓面事实可查；**带边界表述**=断言只在
明示边界内成立；**待裁决**=已登记 `PENDING-ADJUDICATION.md`，本批不动。
证据缩写：`scen`=evidence/scenarios.tsv + evidence/logs/（G4 批 S0-S12）、
`S13`=evidence/s13-scenarios.tsv + evidence/logs/S13-*（返修批）、
`REV`=REVIEW.md（复核方亲跑，独立夹具）、`byte`=evidence/noncomment-bytecheck.log、
`gates`=evidence/fix-batch-gates.log、`dvm`=evidence/derived-vs-manual.txt。

| # | README 断言（位置） | 类别 | 证据/边界 |
|---|---|---|---|
| 1 | 基线 `288b0d9`、分支普通 push、G4 改动面仅 `ci/k5-path-lint.py`（+109/−9） | 实证 | `git`（REV §4：merge-base..f01bb99 非 poc 改动仅此一件） |
| 2 | 返修批 F1 仅注释（+11/−3）、非注释行逐字节未变 | 实证 | `byte`：剥离整行注释后两侧 sha256 均 `34612478…` IDENTICAL；git diff 行级分类（改动行全为注释行） |
| 3 | `before/k5-path-lint.orig.py` sha256 `bbef41e7…` = 基线版 | 实证 | REV 开篇亲验逐字节=288b0d9 同路径 blob |
| 4 | §1 四件角色与表锚（judge.rs:80 冻结锚、report_unit.rs:63-77 三锚） | 实证 | 真树源码可查（行号引用）；表锚=ci/scorechain-manifest.txt 现值 |
| 5 | §2 真集合判定：af/ 下仅 reportkit 三件 .py；judge.rs 唯一门本体；同名不同物不入面 | 实证 | `dvm`；REV §1 双路复算（模块法 vs 独立 shell find/grep）逐件相同 |
| 6 | §2 派生集合 ⊇ 手工四件、双向差集 ∅（分支树与当前主线树均然） | 实证 | `dvm` + REV §1（主线树 2542929 archive 模拟合流亦 True） |
| 7 | §2 `report_unit.rs` 是现行保护面缺口、补齐须裁决 | 待裁决 | §5.1 登记；REV §6 复核认登记成立（主线实测无标记、扩入即误红） |
| 8 | §3 臂1 规则：reportkit 包内 *.py 全集；新增不登记即红 | 实证 | S6 双跑（改前漏钉照绿→改后红）`scen`；REV R1 同 |
| 9 | §3 排除项（`__pycache__`/`*.bak`）不扩面 | 实证 | S8 双绿 4 件 `scen`；REV R9 同 |
| 10 | §3 fail-closed：包缺席/臂派生为空即红 | 实证 | S10 双红 `scen`；S9 改后红 `scen`；S13-R5b 改后红 `S13`；REV R3a/R3b/R8 同 |
| 11 | §3 撞名/tests 错位件不误入下界 | 实证 | S11/S12 双绿 `scen`；REV R0/R11 同；撞名件**带**标记则入面（REV R10 红） |
| 12 | §3 跨锚关系：在册成员改字节先红腿1、删行留件先红下界 | 实证 | S2/S3 双红 `scen`；REV R2/R6 同（改后红条件与 exit 逐字同，仅文案尾指针措辞更新） |
| 13 | §3 边界：R5/R5b 部臂协同收缩单门不红（防线 2→1）；实际拦截者=step0 表钉（exit=1）；派生下界管漏钉面、不覆盖协同收缩 | **带边界**（本批核心修正） | S13 四读（R5/R5bak 改后单门 `PASS（3 件）` exit 0；管道攻击三跑全红 exit 1；管道对照未攻击全绿 12 件）`S13`；REV §3 R5/R5b 行亲跑同读数 |
| 14 | §4 矩阵内只严不松：11/13 一致 + 2 更严（S6/S9） | **带边界**（边界=13 场景矩阵内） | `scen` 全表；REV §3 独立 13 场景（形态对应）复核同 |
| 15 | §4/§4.1 矩阵外存在单门变松形态（R5/R5b 红转绿）；管道级无变松 | **带边界**（明示的单门/管道双层口径） | 同 #13；REV P1 机理段 |
| 16 | §4 S1/S2/S10 红文案尾指针随机制更新、红条件与 exit 逐字同 | 实证 | `scen`（REQUIRED_MEMBERS→派生规则一处措辞）；REV §6 认指针指对了对象 |
| 17 | §5.3 判定逻辑与强度逐字不动（腿1/腿2 红条件、exit 语义） | 实证 | `byte`（返修批）；REV §2（G4 批：main() 三段与基线逐字节相同） |
| 18 | §5.4 标记改名的合法演化会红（故意收紧） | 实证 | S9 `scen` |
| 19 | §5.5 MUT-007 期望（exit 1+「被收缩」）改后仍成立 | 实证 | S1 双跑 `scen`；REV §6/R2 |
| 20 | §5.6 R5/R5b 单门变松已登记、是否恢复双层拦截属裁决题 | 待裁决 | `PENDING-ADJUDICATION.md` ②（技术分析+建议） |
| 21 | §5.7 MUT-007 description 陈旧指针（仍指 REQUIRED_MEMBERS） | 待裁决 | `PENDING-ADJUDICATION.md` ①（现值/应值+影响面：仅描述文本，行为期望 S1 双绿） |
| 22 | §6 门读数（G4 批三门） | 实证 | evidence/{real-tree-*,clippy-gate.log,gate-inputs-check.log}；REV §5 亲跑同读数 |
| 23 | §8 返修批终读数：三门全绿 + S13 + 真树零改动 | 实证 | `gates`（本批亲跑终值） |
| 24 | §8 保留既有结论（派生替手工、⊇、真树双跑一致、合流预演 PASS、S0-S12 零改写） | 实证 | #5/#6/#17 + REV §1（合流预演 PASS 4 件）+ 本批 git status（S0-S12 证据件零改动） |

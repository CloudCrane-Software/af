# poc/poc-g4-k5 —— G4 复核面独立复核报告（REVIEW.md）

- 复核人：外派复核方（未参与施工；施工方证据只做抽查比对，全部结论以亲跑为准）
- 复核日期：2026-11-04；工作树 `~/af-dev/af/wt/g4-k5/`（分支 `feature/g4-k5-derived`，未切支/未重置/未动共享树）
- 基线（自己算）：`git merge-base f01bb99 2542929` = **`288b0d9`**（= 施工 README 自述基线；
  `origin/product/main` 现值 `2542929`，已前进 18 提交，未拿来当"改前"）
- 分支独有提交恰 1 个：`f01bb99`；远端 `git ls-remote` = 本地 HEAD（一致）
- 改动面（merge-base..HEAD）：`ci/k5-path-lint.py`（+109/−9，与 README 自述一致）+ `poc/poc-g4-k5/**`（全新增）
- `poc/poc-g4-k5/before/k5-path-lint.orig.py` sha256 `bbef41e7…` 逐字节=基线版（亲验）
- 复核沙箱：**夹具全部自造**（`/tmp/g4review/my_scenarios.py`，13 场景 × 改前/改后双跑），
  未复制施工方沙箱；施工方 `evidence/logs/` 仅做事后抽查比对（S2/S6/S9/S10 与我的复现逐条吻合）

---

## checked（六项全核，逐条亲跑）

### §1 派生真的替掉了手工清单（核心）——过

- **手工清单已消失**：AST 扫改后 `ci/k5-path-lint.py` 全部代码级字符串常量——**无任何成员
  路径名**（`judge.rs`/`citation_unit`/`rubric_unit`/`render.py` 只出现在注释里）；代码常量面
  仅规则件（`af/packages/reportkit` 扫描根、`*/src`、`script_sha256` 内容标记、
  `__pycache__`/`.bak` 排除项）——是"扫什么/怎么判"的规则，不是成员名单。
- **派生规则可复算、可解释**：两臂规则与文件头注释一致；模块法
  （`derive_report_arm`/`derive_swev_arm`/`derive_required_members`）与**独立 shell 复算法**
  （`find af/packages/reportkit -name '*.py'` + `grep -rl script_sha256 crates/*/src --include='*.rs'`）
  双路结果**逐件相同**。
- **派生集合 ⊇ 改前手工四件**（改前四件从**基线 ref** `git show 288b0d9:ci/k5-path-lint.py`
  自取）：分支树与**当前主线树 `2542929`**（`git archive` 出树+装改后 lint 模拟合流）上均为
  `MANUAL-4 ⊆ DERIVED = True`，双向差集 ∅——恰等四件，无少覆盖。
- 主线模拟（合流预演）：`PASS（4 件判分链成员逐位对锚）` exit=0 ——**合入当前主线不会误红**；
  `report_unit.rs`/`swev.rs` 实测不含 `script_sha256`（README §5.1 待裁决登记的事实前提属实）。

### §2 判据强度没被放松——过（判据面）；基线臂样本对照见 §3 的 R5 例外

- **判定逻辑与红条件逐字未动**：`main()` 三段（登记表解析/空表红、逐位对锚漂移臂、
  FAIL/PASS 输出与 exit）与基线版**逐字节相同**（diff + 程序化切段比对）；唯一逻辑差异 =
  下界成员来源（`REQUIRED_MEMBERS` 元组 → `derive_required_members()`）+ 收缩红文案尾
  一处措辞（见 §6）。红消息头、exit 语义、漂移文案逐字未动。
- **真树双跑**：改前（经模块导入重定向 REPO_ROOT，只读）与改后输出**逐字节相同**：
  `K5 判分链路径 lint: PASS（4 件判分链成员逐位对锚）`，exit=0/0。

### §3 新语义只更严、绝不变松——任务书四场景全过；但 blankets 断言被加验场景推翻（见 problems P1）

自造夹具沙箱（撞名件 `af-credential/judge.rs` 无标记 + `tests/p3_probe.rs` 带标记 均在场）13 场景双跑：

| 场景 | 改前 | 改后 | 判定 |
|---|---|---|---|
| R0 原样（含撞名件+tests 标记件在场） | PASS 4 | PASS 4 | 一致（**§3④ 不误纳入下界** ✓） |
| R1 判分包新增 .py 不登记 | **PASS 4（漏钉照绿）** | **红·被收缩（点名 new_unit.py）** | 改后更严 ✓（**§3①，G4 修复点**） |
| R2 删 citation 登记行（留件） | 红·被收缩 | 红·被收缩 | 一致 ✓（**§3②**；文案尾差异见 §6） |
| R3a 判分包整目录删+3 行同批摘 | 红·被收缩(3件) | 红·载体包缺席 | 一致（均红）✓（**§3③**） |
| R3b 删包目录（行保留） | 红·漂移(缺席) | 红·载体包缺席 | 一致（均红）✓ |
| **R5 删成员文件+删登记行同批** | **红·被收缩** | **PASS 3（绿）** | **改后更松（红→绿）——见 P1** |
| **R5b 改名 .bak+删登记行同批** | **红·被收缩** | **PASS 3（绿）** | **同上** |
| R6 在册成员字节漂移 | 红·漂移 | 红·漂移（输出同） | 一致 ✓ |
| R7 登记表只剩注释 | 红·表为空 | 红·表为空（输出同） | 一致 ✓ |
| R8 swev 臂协同收缩（摘标记+换锚） | PASS 4（漏拦） | 红·swev 臂派生为空 | 改后更严 ✓ |
| R9 `__pycache__/*.py`+`*.bak` 在场 | PASS 4 | PASS 4（排除项生效） | 一致 ✓ |
| R10 撞名件**带**标记（内容型判定） | PASS 4 | 红·被收缩（点名撞名件） | 改后更严（内容型非文件名 ✓） |
| R11 tests/ 面新增标记件不登记 | PASS 4 | PASS 4（不在扫描面） | 一致 ✓ |

- 任务书指定四场景：①R1 红 ✓ ②R2 红 ✓ ③R3a/R3b 红（不静默缩面）✓ ④R0/R11 撞名/错位件
  不入下界 ✓。
- 加验 R5/R5b 发现**部分臂协同收缩在 k5 单门由红转绿**——施工方 13 场景矩阵未覆盖
  （S2=删行留件、S10=删整目录、S9=swev 臂全摘），详见 problems。
- **管道级补验**：R5 形态的登记表改动（`--root` 沙箱+真脚本）被 step0 表钉拦红：
  `gate_inputs_check: 判据正本漂移：ci/scorechain-manifest.txt 期望 sha256=121c1d094b50… 实得
  1b45bb5fbc19…——判据被改须走 update 同批评审重钉`，exit=1（`ci/run.sh:149` 挂 step0）。

### §4 判据正本与钉桩面零改动——过

- `git diff --name-only 288b0d9..f01bb99 | grep -v '^poc/'` ⇒ **仅 `ci/k5-path-lint.py`**。
- blob 级比对：`ci/clippy_gate.py`、`ci/clippy-baseline.txt`、`ci/run.sh`、
  `ci/scorechain-manifest.txt`、`ci/gate_inputs_check.sh`、`ci/baselines/gate-inputs.sha256`
  **全部 SAME**；`git diff --name-only -- crates/ af/` = **0 行**。
- diff 里出现的 `judge.rs`/`scorechain-manifest.txt` 全部在 `poc/poc-g4-k5/evidence/sandboxes/`
  沙箱夹具副本下；抽验 S0 沙箱 judge.rs sha256 前缀 `da945f50` = 真件 = 登记表锚前缀
  （夹具副本属实）；S9 副本不同属场景设计（摘标记）。

### §5 门真跑（全部亲跑）——过

| 门 | 命令 | exit | 输出 |
|---|---|---|---|
| K5 判分链 lint（真树） | `python3 -I ci/k5-path-lint.py` | 0 | `PASS（4 件判分链成员逐位对锚）` |
| 〖两臂读数〗 | 模块导入 `derive_report_arm()/derive_swev_arm()` | — | report 臂 3 件 + swev 臂 1 件（上面 §1） |
| 门输入全集钉 | `bash ci/gate_inputs_check.sh check` | 0 | `判据正本钉对账一致: 12 件` |
| clippy 门对应项 | `python3 -I ci/clippy_gate.py ci/out/step2-clippy.jsonl`（既有产物，本批零 cargo） | 0 | `CLIPPY GATE: PASS —— 0 警告/0 签名，均在基线内` |

（与施工方 `evidence/clippy-gate.log`、`gate-inputs-check.log` 记录一致。）

### §6 说法诚实性——过

- `report_unit.rs`（按规则够格但未登记）在 README §5.1 **如实登记为待裁决**："本批不扩、
  不偷偷兜底"——AST 扫描证代码确无手工兜底残留；主线树实测其不含标记，扩进下界确会
  对主线误红（登记的权衡成立）。
- 收缩红文案 "同批改 REQUIRED_MEMBERS 下界清单"→"同批改本文件下界派生规则"：指针**指对
  了对象**（`REQUIRED_MEMBERS` 已不存在，派生规则常量+函数确在本文件）；R2 双跑证**红条件
  与 exit 逐字未变**（同一触发、同 exit 1，仅该尾句变化）。
- MUT-007 期望（exit 1 + stderr 含「判分链登记表被收缩」）在改后消息头保留（R2 复现命中），
  `mutations/registry.yaml` 零改动。

---

## problems

**P1（实质，单门强度）——「删成员文件+删登记行同批」在改后 k5 单门由红转绿（改前红）。**
命令与输出（我的沙箱 `R5-file+row-co-deleted`，夹具绿基线同 S0）：

```
改前 exit=1 | k5-path-lint: 判分链登记表被收缩（必需成员缺席：af/packages/reportkit/units/citation_unit.py）…（REQUIRED_MEMBERS 仍点名 ⇒ 红）
改后 exit=0 | K5 判分链路径 lint: PASS（3 件判分链成员逐位对锚）
```

机理：下界改从仓面现算后**随树收缩**——删掉 citation_unit.py 文件+其登记行后，report 臂派生
{rubric,render} 非空（不触发 fail-closed）且 ⊆ 在册集，腿1 无行可查 ⇒ 双腿皆绿。改前的
手工四件是与树独立的第二锚，恰好拦该路径。R5b（改名 `.bak`+删行）同型同结论。**该场景
不在施工方 13 场景矩阵内**（S2 删行留件/S10 删整目录/S9 swev 臂全摘，均非"部分臂协同收缩"）。
**管道级无逃逸**（亲验）：改登记表必被 `gate_inputs_check` 的 GATE_INPUTS 表钉拦红
（`run.sh:149`，step0）⇒ CI 全管道仍红；且删掉的件若日后重新落盘会自动重入派生集、不登记
即红（派生面自复装，优于被编辑掉的手工清单）。但**单门层面防线由 2 层（k5 手工清单 +
step0 表钉）降为 1 层（仅 step0 表钉）**——对本门而言是真实的变松实例。

**P2（由 P1 派生，说法失实）——两处 blankets 断言被 P1 推翻：**
- `ci/k5-path-lint.py` 头注「互为跨锚…**任何收缩动作先红腿1**」：R5 路径腿1 无行可红、
  下界随树缩也不红——真正拦它的是 step0 表钉（外部跨锚），不是腿1。
- `poc/poc-g4-k5/README.md` §3/§4「强度不弱于手写清单」「**无任何场景改后变松**」：
  在 k5 单门语义下对 R5/R5b 不成立（改前红/改后绿）；在 run.sh 管道语义下成立（step0 拦）。
  说法未带此边界，属过度断言。

**P3（轻微，留痕）——`mutations/registry.yaml` MUT-007 description 仍写
"ci/k5-path-lint.py REQUIRED_MEMBERS"**（该机制已改为仓面派生）。registry 非本批可改面
（本批零改动正确），属陈旧指针，留待后续裁决批顺手更新（同款先例=本批已修的收缩红文案指针）。

## fixes

- **F1（必改，面仅注释+证据，不触判据）**：修正 `ci/k5-path-lint.py` 头注「互为跨锚」段与
  README §3 末条/§4 结论行，如实表述：*"删登记行+删成员文件同批（部分臂收缩）不改红本门
  （腿1 无行可查、派生随树收缩）——该路径由 ci/gate_inputs_check.sh 对 scorechain-manifest.txt
  的表钉（run.sh step0）拦截；故单门层面该场景改后较改前松（防线 2→1），管道层面无变松"*；
  并把 R5 场景补进 poc 证据面（run-scenarios.py 加 S13 或 evidence/logs/ 补一组双跑留证）。
- **F2（必改）**：README §4「无任何场景改后变松」改为带边界表述（覆盖矩阵 13 场景 +
  未覆盖的协同收缩类由 step0 拦截的明示）。
- **F3（后续裁决批，非本批）**：MUT-007 description 指针更新；若要恢复该路径的单门双层
  拦截（如本门自钉 manifest 摘要），须走裁决——与 step0 表钉职责重叠，建议优先保持现状、
  以 F1 的边界写法为准。

## overall

**不可收卷（当前态；整改面小且路径明确）**——六项复核中 §1/§2 判据面/§4/§5/§6 全过、任务书
§3 四场景全过、管道级保护无洞且合流预演不误红；但「只严不松」与「任何收缩动作先红腿1」
两条断言被亲跑实证的 R5/R5b（部分臂协同收缩：单门红→绿、防线 2→1）推翻，按本仓 fail-closed
纪律，须先落 F1/F2（注释+文档+补证，零判据改动）再收卷；F1/F2 落地后本复核意见为可收卷。

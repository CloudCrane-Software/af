# poc/poc-g4-k5 —— G4：K5 判分链保护面由手工四件改为门自派生（留证包）

- 基线：开工时 `origin/product/main` = `288b0d9ecd4b0ceaaf5b55faae5a2411ae48cef3`
- 分支：`feature/g4-k5-derived`（普通 push，未动 product/main 指针）
- 改动面：**仅 `ci/k5-path-lint.py`**（+109/−9，任务书写作 af/ci/，仓内实路径为 ci/）；
  本 poc/ 目录为证据件（新增，不属门码面）。零 cargo（未跑任何 cargo 命令）。
- 原版快照：`before/k5-path-lint.orig.py`（sha256 `bbef41e7…`，逐字节=基线 288b0d9）

---

## 1. 现状读准（任务书 §一 ①：四件是什么、怎么被消费）

`ci/k5-path-lint.py` 两条腿：

- **腿1（逐位对锚）**：`ci/scorechain-manifest.txt` 每行 `<路径 TAB sha256 TAB 用途>`，
  逐件对仓面文件 sha256；缺席/漂移 = exit 1。
- **腿2（下界对账，RT-E6/E51 修复面）**：手写元组 `REQUIRED_MEMBERS` 四件必须全部
  在册；缺一即 exit 1「判分链登记表被收缩」。防的是：登记表被收缩（删行）后
  在册行仍逐位对锚、腿1 照绿，被摘行的判分件脱离哈希锚保护面。

四件逐件（路径 → 在判分链中的角色 → 在 lint 里的用法）：

| # | 路径 | 角色（证据） | 用法 |
|---|------|------|------|
| 1 | `crates/af-execline/src/judge.rs` | swev 判分**门本体**：装载期指纹门，锚死仓外判分核 `tools/swev-judge.py`（`/home/pan-ding/workspace/agent黑灯工厂/…`，judge.rs:80 冻结锚 `a292547f…`）+ 导入链六件锚（judge.rs:84-115） | 腿2 下界点名 + 腿1 表锚 `da945f50…` |
| 2 | `af/packages/reportkit/units/citation_unit.py` | report 判分**核**（citation）：`ReportJudgeLocator.citation_script` 装载、锚 `0c34b595…`（report_unit.rs:64-66） | 腿2 下界点名 + 腿1 表锚 |
| 3 | `af/packages/reportkit/units/rubric_unit.py` | report 判分**核**（rubric）：`rubric_script` 装载、锚 `e8b67eb6…`（report_unit.rs:69-74，加载侧自证 W2-C50/W3-C-06） | 腿2 下界点名 + 腿1 表锚 |
| 4 | `af/packages/reportkit/tools/render.py` | report 判分**基准**（R5 渲染器，W1-C61 入指纹门）：`render_script` 装载、锚 `255f68f7…`（report_unit.rs:75-77） | 腿2 下界点名 + 腿1 表锚 |

## 2. 真集合判定与差集证据（任务书 §一 ②③）

**仓面判分链真集合 = swev/report 两判分臂的「判分核 + 判分门本体」仓内成员**
（口径=登记表头与 k5 lint 头的自述）：

- **swev 臂**：判分核 `swev-judge.py` **不在仓内**（judge.rs:80 绝对路径在仓外工厂工作区）
  ⇒ 仓内成员 = 冻结锚所在的判分门本体 `crates/af-execline/src/judge.rs`
  （登记表头第 10-15 行自述规则：「仓内保护面对该锚所在的判分门本体 judge.rs 生效」）。
- **report 臂**：`ReportJudgeLocator` 装载/锚定的判分脚本全部住在
  `af/packages/reportkit/{units,tools}`（report_unit.rs:63-77 的 `pkg_root` 相对锚）
  ⇒ 仓内成员 = 该三件 .py。`find af -name "*.py"` 全仓证：af/ 下**仅有此三件**
  .py，别无判分 python 源。
- **同名不同物**：`crates/af-credential/src/judge.rs` 是权限交集判定器
  （Cedar∩biscuit，模块首行自述），与判分链**无关**——文件名撞名不等于成员。

**差集**（证据文件 `evidence/derived-vs-manual.txt`）：

- 手写 − 真集合（多出来的）：**∅** —— 四件逐件如上举证，均为真成员。
- 真集合 − 手写（漏掉的）：
  - **`crates/af-execline/src/suite/report_unit.rs`**（report 臂判分门本体/锚载体：
    `ReportJudgeLocator` 定义 + 三锚现产值 + `check_fingerprints`，report_unit.rs:40-115）。
    按登记表头自述的同一规则（「仓内保护面 = 锚所在的门本体」），它够格为真成员，
    但**登记表未登记、下界未点名** ⇒ 改它不红 step1（哈希锚保护面外）。
    这是现行保护面的**缺口**，非本批可修（补它须裁决 + 扩登记表 + 同批评审，
    本任务硬约束禁改登记表/判据）——**如实登记，见 §5**。
  - `crates/af-execline/src/suite/swev.rs`（判分**调用面**：capture_diff/invoke/eval 执行
    臂）——按「判分核+判分门本体」口径属执行 plumbing 而非判分核/门本体，不入真集合。
  - `crates/af-execline/tests/pkg_reportkit*.rs`、`p3_judge.rs` 等——测试面，非成员。

## 3. 派生规则（任务书 §一 ②：扫什么、怎么判、排除项）

下界 = `derive_required_members()` 仓面现算（**无手工成员清单**；两臂并集、排序稳定可复算）：

- **臂1 report 判分臂**：扫 `af/packages/reportkit/` 递归 `*.py` 全集。
  判据=判分包内 python 源即 report 判分脚本面（ReportJudgeLocator 的 pkg_root 锚 +
  登记表头「report 判分 = reportkit 三件」）。新增 .py **自动入下界**（未登记即红）。
  - **显式排除项**（按文件种类，同 `ci/fix_verify.py` 排除面先例）：`__pycache__/`
    目录（字节码产物——运行侧另有 W2-C52 在场性门独立守）；`*.bak`（历史快照，
    任何位置的 .bak 都不是正本）。
  - **fail-closed**：判分包目录缺席 / 该臂派生为空 = 红（不静默缩面）。
- **臂2 swev 判分臂**：扫 `crates/*/src/` 递归 `*.rs`（**生产源面**；tests/ 不在面内），
  含 `script_sha256`（judge.rs:64-65「判分脚本 sha256 锚」字段，即对仓外 swev-judge.py
  的冻结锚声明）的文件 = 判分门本体。**内容型判定、非文件名判定**
  （af-credential/judge.rs 撞名不误伤；tests 里的 script_sha256 是构造 locator 不算门本体）。
  - **fail-closed**：该臂派生为空 = 红。
- **互为跨锚（强度不弱于手写清单）**：下界只能通过「改判分链本体字节」收缩——
  在册成员字节被腿1 的登记表 sha256 锚死，任何收缩动作先红腿1、走裁决
  （冻结纪律不变；沙箱 S9 实证：协同换锚+摘标记的收缩在改前照绿、改后红）。

**基线仓面实测**：派生集合 = 手写集合（恰四件），`派生 ⊇ 手写 = True`，
双向差集 = ∅ —— 无「派生不出的手写成员」（故无需人工钉残留），无多余覆盖。

## 4. 改前改后 lint 结论对照（任务书 §一 ④⑤）

- 真树（`evidence/real-tree-{before,after}.log`）：改前/改后均
  `PASS（4 件判分链成员逐位对锚）`，exit=0，**输出逐字节相同**。
- 沙箱对照跑批（`run-scenarios.py` → `evidence/scenarios.tsv` + `evidence/logs/`，
  13 场景 × 两版 lint 同沙箱双跑）：

| 场景 | 改前 | 改后 | 判定 |
|---|---|---|---|
| S0 原样基线 | PASS 4 | PASS 4 | 一致 |
| S1 MUT-007 原样重放（删 judge.rs 登记行） | 红·被收缩 | 红·被收缩 | 一致 |
| S2 删 citation_unit.py 登记行 | 红·被收缩 | 红·被收缩 | 一致 |
| S3 在册成员字节漂移 | 红·漂移 | 红·漂移 | 一致 |
| S4 删在册成员文件 | 红·缺席 | 红·缺席 | 一致 |
| S5 登记表只剩注释 | 红·表为空 | 红·表为空 | 一致 |
| **S6 判分包新增 .py 未登记** | **PASS 4（漏钉）** | **红·被收缩（点名新件）** | **分叉=本 G4 修复点** |
| S7 新成员走正道登记新锚 | PASS 5 | PASS 5 | 一致 |
| S8 __pycache__/*.pyc 与 *.bak 在场 | PASS 4 | PASS 4（排除项生效） | 一致 |
| **S9 协同收缩（摘锚声明字段+登记行换新锚）** | **PASS 4（漏拦）** | **红·swev 臂派生为空** | **分叉=改后更严（fail-closed）** |
| S10 判分包整目录删+行同批摘 | 红·被收缩(3件) | 红·载体包缺席 | 一致（均红） |
| S11 af-credential/judge.rs 撞名非成员 | PASS 4 | PASS 4（不误伤） | 一致 |
| S12 tests/ 面出现锚字段 | PASS 4 | PASS 4（不误扫） | 一致 |

结论：**既有保护面 11/13 场景结论逐条一致**（S1/S2/S10 的红文案里
「同批改 REQUIRED_MEMBERS 下界清单」→「同批改本文件下界派生规则」一处指针
措辞随机制更新，红条件与 exit 逐字同）；仅 2 处按设计分叉且均为**改后更严**
（S6=本 G4 要堵的漏钉；S9=派生 fail-closed 拦下协同收缩）。无任何场景改后变松。

## 5. 如实登记的边界

1. **`crates/af-execline/src/suite/report_unit.rs`（report 臂锚载体）不在下界、不在登记表**
   ——它够格为真成员（§2），但登记表未登记；把派生下界扩到它会对现行主线误红
   （下界必须 ⊆ 在册集才不破「改前改后一致」）。补齐须走裁决：登记表加行 +
   派生规则同批评审。**本批不扩、不偷偷兜底。**
2. **两臂的扫描根（`af/packages/reportkit`、`crates/*/src`）与内容标记
   （`script_sha256`）是派生规则的常量**，不是成员清单——它们定义「扫什么、
   怎么判」，改动=改判据，走本文件 diff 评审（同 fix_verify 排除面纪律）。
3. **判定逻辑与强度逐字不动**：腿1/腿2 的红条件、判据、exit 语义原样；
   摘除的只是「成员从哪来」（手写元组 → 仓面现算）。唯一文案变更是收缩红里的
   指针措辞（REQUIRED_MEMBERS 已不存在，原文会指错对象）。
4. **标记改名的合法演化会红**（设计内）：judge.rs 若裁决改名 `script_sha256`
   字段，臂2 派生为空即红——逼该裁决批同批改派生规则（S9 证明此门是故意收紧）。
5. `mutations/registry.yaml` MUT-007 的期望（exit 1 + stderr 含「判分链登记表被收缩」）
   由 S1 沙箱原样重放双绿（改前改后均中）；MUT-001（删门脚本）不受影响。

## 6. 门读数（任务书 §四）

| 门 | 命令 | exit | 结论 |
|---|---|---|---|
| K5 判分链 lint（改后真树） | `python3 -I ci/k5-path-lint.py` | 0 | `PASS（4 件判分链成员逐位对锚）` |
| clippy 门对应项 | `python3 -I ci/clippy_gate.py ci/out/step2-clippy.jsonl` | 0 | `PASS —— 0 警告/0 签名，均在基线内`（本批零 Rust 改动，喂既有 step2 产物复算；未跑 cargo） |
| 门输入全集钉（自检入口） | `bash ci/gate_inputs_check.sh check` | 0 | `判据正本钉对账一致: 12 件` |
| MUT-007 语义重放 | S1 沙箱 | 1/1 | 改前改后均「判分链登记表被收缩」红 |

证据文件清单：

- `before/k5-path-lint.orig.py` 基线原版（sha256 `bbef41e7…`）
- `evidence/real-tree-{before,after}.log` 真树双跑
- `evidence/derived-vs-manual.txt` 派生 vs 手写 + 两臂分解 + 双向差集
- `evidence/scenarios.tsv` + `evidence/scenarios-run.log` + `evidence/logs/*.log` 13 场景双跑全录
- `evidence/clippy-gate.log`、`evidence/gate-inputs-check.log` 门读数
- `evidence/sandboxes/` 各场景沙箱终态（审计可复核）

## 7. 提交与远端核验

- 提交：见 `git log feature/g4-k5-derived`（提交号在交付报告回填）
- 远端：`git push origin feature/g4-k5-derived`（普通 push，无 force）
- 核验：`git ls-remote origin feature/g4-k5-derived` 与本地一致（交付报告回填读数）

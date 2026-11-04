# T7② integrator 复核制常设化 —— 施工证据

> 批次：v0.3-T7②（integrator 复核制：base_sha+source_sha+tested_tree+suite_digest）
> 分支：`feat/t7-2-integrator-bind-check`（基线 = `origin/product/main` = `56ff52a`，**未 commit、未 push**）
> 工作树：`/home/pan-ding/workspace/af-wt-t7-2`（`git worktree add -b`，主仓 `/home/pan-ding/workspace/agent黑灯工厂/af` 未动）
> 交付件：`ci/integrator_bind_check.py`（新增）、`docs/INTEGRATOR-SOP.md`（新增）、
> 本证据目录（新增）。**未改** `crates/**`、`ci/` 既有件、`Cargo.toml`/`Cargo.lock`、
> `af/poc/poc-e-codeadapter/**`、既有测试断言。

---

## ① 复核器设计（记录侧 / 实测侧 / 集合定义 / fail-closed 语义）

### 1.1 四项绑定：两侧各怎么算

| 键 | 语义 | 记录侧（expected） | 实测侧（actual，复核器自算） |
|---|---|---|---|
| `base_sha` | 分支与目标分支的 **fork 点**（单提交分支时 = 分支父提交，与 PoC-E `HEAD~1` 同值） | `emit` 时由工具自算 | `git merge-base <base_ref> <source_sha>` |
| `source_sha` | **远端**该分支头（推送的就是测过的那个提交） | 同上 | `git ls-remote <remote> refs/heads/<branch>` |
| `tested_tree` | `source_sha^{tree}`（合入内容逐位 = 门跑过的内容） | 同上 | `git rev-parse <source_sha>^{tree}` |
| `suite_digest` | 门执行面的**文件内容摘要**（门没被换） | 同上 | 同一算法在**落地树**上重算 |
| 另 `verdict` | 门结论，**严格等于** `"pass"` | 记录件字段（由内嵌门摘要自证） | 与记录侧逐字比 |

★ 语义来源（**只读参考，未改**）：PoC-E `af/poc/poc-e-codeadapter/codeadapter.py:144`
`four_binding_check`；verdict 词汇表：`crates/af-workorder/src/gate_admit.rs:55`
（`if item.verdict != "pass"` —— 严格等于，大小写敏感）。

★ **机制差异披露（v0.3-T7② 返修批补）**：verdict 的**推导机制**与 PoC-E **不同** ——
PoC-E `codeadapter.py:272` 认的是**显式绿标** `PIPELINE-GREEN`
（`verdict = "pass" if "PIPELINE-GREEN" in summary else "fail"`）；
本器（`ci/integrator_bind_check.py`）按 **`ci/run.sh` 的 step 状态词表**判定
（代码常量 `STEP_STATUS_ALLOWED`，与 `docs/INTEGRATOR-SOP.md` §一.3 的词表**逐字一致**）：

| 词 | 判定 | 依据 |
|---|---|---|
| `PASS` | 绿 | `ci/run.sh:65`（步骤 rc=0） |
| `PASS-WITH-WARN` | 绿 | `ci/run.sh:615`（step14 rc=2=advisory WARN）；`ci/run.sh:618` **只在 rc==1** 时把 OVERALL 置 1 ⇒ 门 exit 0 仍绿 |
| `ADVISORY-PASS` | 绿 | `ci/run.sh:477` |
| `ADVISORY-WARN` | 绿 | `ci/run.sh:483`（工具缺席） |
| `FAIL` | **红** | `ci/run.sh:68`（硬红 ⇒ OVERALL=1） |
| `ADVISORY-FAIL` | **红** | `ci/run.sh:480`＋仓内纪律（`ci/run.sh:476-489`：未消化的 ADVISORY-FAIL 不得结案） |
| 未知词 | **红** | fail-closed |

**理由**：真实 `ci/run.sh` **从不写** `PIPELINE-GREEN`（`grep PIPELINE-GREEN ci/run.sh .gitea/`
无命中）⇒ 照抄 PoC-E 口径会让**真实绿门恒红**（假红）⇒ 词表口径是**必要适配**；
**四项绑定判定语义未放宽**（四项任一不符/为空/缺项 ⇒ 拒），白名单只收**真绿**状态。

### 1.2 `suite_digest` 的集合定义（`af.gate_suite_digest.v1`）

- **成员**（树内枚举、路径升序）：`ci/` 下全部跟踪文件（门脚本 + 判据正本：
  clippy 基线 / 白名单 / 词表 / golden 册 / debt 册 / 门输入钉 / 判据 yaml……）
  ＋ `.gitea/workflows/` 下全部文件（F1 快速门载体）。
- **排除**：`ci/out/**`（门跑出来的**产物**，每次跑门都变 ⇒ 纳入则摘要恒变）、
  `ci/__pycache__/**`、`ci/advisory-db/**`（未入库）、末段 `.bak` 的文件（历史备份件）。
- **理由**：① **覆盖完整**——门脚本与判据正本都在 `ci/` 下，「门没被换」不必逐件
  枚举就不漏面；② **免漂移**——成员由**树枚举**得出，新增门脚本自动入集合，
  不会出现"新门脚本上线但摘要不变"的空子；③ **排除项有据**——`ci/out/**` 是产物、
  `.bak` 是备份，纳入只会造"摘要恒变"的假红；④ **与仓内纪律同向**——
  `ci/run.sh:136-152` 已明写「改判据正本即改门」。
- **算法**（确定性，双方可独立复算）：
  `digest = sha256( 逐成员: path + "\0" + sha256(blob 内容) + "\n" 拼接 )`。
- **实测读数**（本工作树 `HEAD` 树 = `56ff52a`）：成员 **44 件**，摘要
  `80e7578c9d6f83db1b81b219c8459ca9d12523640b6b838504dea8b91cf0dee3`
  （`python3 -I ci/integrator_bind_check.py digest --tree HEAD`）。

### 1.3 fail-closed 语义（一条都不许放宽）

复核器 **exit 1（拒绝）** 的情形：`RECORD-ABSENT` / `RECORD-UNPARSEABLE` /
`RECORD-SCHEMA`（记录件缺席/不可解析/schema 不认 ⇒ **拒绝，不是跳过**）、
`KEY-MISSING` / `KEY-EMPTY` / `KEY-MALFORMED`、`BIND-MISMATCH`（四项任一不符）、
`VERDICT-NOT-PASS`（**严格等于** `"pass"`，`"PASS"`/`"fail"`/`""` 一律拒）、
`GATE-SUMMARY-ABSENT` / `GATE-SUMMARY-DIGEST` / `GATE-VERDICT` / `GATE-EXIT-NONZERO`
（★ **门不可跑 ⇒ 拒绝**）、`GATE-BASE-DRIFT`（门跑完后主线前移）、
`COVERAGE-TESTED-TREE`（落地树 ≠ 记录侧 tested_tree ⇒ **候选树门覆盖不成立**）、
`SUITE-DRIFT`、`MERGE-TREE-CONFLICT`、`REMOTE-UNREACHABLE` / `REMOTE-HEAD-UNRESOLVED`、
`REF-UNRESOLVED` / `MERGE-BASE-UNRESOLVED`、
`LOCAL-SOURCE-NOT-ALLOWED`（`verify --source <本地提交>` 未显式 `--allow-local-source`）。

★ **门摘要回读是强制的（`verify` 无开关可绕）**：`GATE-SUMMARY-DIGEST` 现在覆盖
「磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文」**三方比对**（初版只比内嵌两样 ⇒ 内嵌摘要可被
记录件作者完全控制）。回读路径缺省 = 记录件的 `gate_summary_path`，
也可用 `verify --gate-summary <路径>` 显式指定（内容仍须逐字节相符）。

★ **机器仲裁员首版不启用**：复核器 exit 0 只表示「机器侧未发现不符」，
**合流裁决仍由人工 integrator 依 SOP 作**（`docs/INTEGRATOR-SOP.md` §〇）。
复核器**不含任何自动合流动作**（不 push、不开 PR、不改 ref、不动 `product/main` 指针）。

### 1.4 复核器子命令

| 子命令 | 用途 | 是否判定 |
|---|---|---|
| `emit` | 门绿后写记录件（四项**本器自算**，禁手抄；内嵌门 SUMMARY 原文 + sha256 + 退出码） | 写件（门红/摘要判不出绿 ⇒ 拒写） |
| `verify` | 核对记录侧 vs 实测侧（**强制回读**门摘要文件：磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文 三方比对） | ★ 判定（exit 0/1） |
| `actual` | 只算实测侧并打印（审计 / 历史合流核验用） | 否（无记录侧 ⇒ 无结论） |
| `digest` | 打印某棵树的门套件摘要 | 否 |

---

## ② 合流 SOP 全文摘要 + rebase 条款（可核动作）

全文见 `docs/INTEGRATOR-SOP.md`（十节）。要点：

- **§〇 效力边界**：人工 integrator + 机器辅助核对；**机器仲裁员首版不启用**；
  判据只许变严；**禁"门不可跑就跳过复核"**。
- **§二 何时必须走复核**：判据 = **落地内容进入 `product/main`**（唯一合流目标，
  `docs/CI-BOUNDARY.md` §五）。**不走**的边界：不进 af 仓的落账、进主线前的中间搬运、
  只读核验、对照线 `main`（冻结只读）；指针回退/force push **一律禁止**（不是豁免）。
  **不许豁免**：纯文档面合流（step11 `doc-claims` 对账 + step12 `numbers.json` 复算
  都读仓内文档 ⇒ 文档改动能改门结论）、门执行面/判据正本被改、有冲突解法。
- **§三 常设流程**（谁·何时·做什么，11 行表）：候选树定稿（集成分支从
  `origin/product/main` 拉）→ 推远端候选分支 → F1 快速门（Actions，base=product/main；
  绿是**必要非充分**条件）→ F2 全量门 `bash ci/run.sh` 在**候选树**上跑 → **门绿后**
  `emit` 写记录件 → **integrator 跑 `verify`** → 人工判读 → 合流（`--ff-only`）→
  **合流后核**（`git rev-parse <合并提交>^{tree}` == 复核输出的 `merge_tree`）→ 推
  `product/main`（普通推，禁 force）→ 留证 → 落账。串行度 = 1（方案裁决）。
- **§四 记录侧**：格式 `af.integrator_bind_record.v1`；**工具写、禁手抄**；
  门绿之后、合流动作之前写；落 `ci/out/integrator-record-<batch>-<branch-slug>.json`；
  ★ **不入候选树**（入了就改树 ⇒ `tested_tree` 失配 ⇒ 自证失效）；可合流后作工件签入；
  ★ **不许为历史合流补造记录件**。
- **§五 rebase 条款（可核动作，不是散文）**：
  - **三个可核判据**：① 基点漂移（记录侧 `gate_base_sha` vs 现目标头 ⇒
    `GATE-BASE-DRIFT`）；② **树等价**（记录侧 `tested_tree` vs 实测 `source_sha^{tree}`
    与**落地树** ⇒ `BIND-MISMATCH` / `COVERAGE-TESTED-TREE`）；③ 门没被换
    （`suite_digest`）。
  - **动作**：判（跑 `verify` 看 reason_code）→ **重落基点**（rebase/合入当前主线）→
    **重跑门**（按记录件 `gate_id` 重跑那一门；F1 由 Actions 在新推送上自动重跑，
    F1 绿**不替代** F2 记录件）→ **重写记录件**（新文件名 `-r2`，旧件不许删）→
    **再复核** → 绿则进 §六 判读。
  - ★ **不许拿"四项里前三项对上了"搪塞过去**：`COVERAGE-TESTED-TREE` 就是那个独立确认。
  - rebase **未改内容**（`tested_tree` 相等 ⇒ 复核绿）⇒ 可继续，但**基点变了要留痕**。
- **§六 自动判过 vs 人工看**：可自动判过（候选树只动 `docs/**`/根级 `*.md`/`ci/out/**`
  ＋ 快进 ＋ 门摘要无 `ADVISORY-WARN` ＋ 无基点漂移）⇒ 人工只读一行；
  必须人工看（触及 `crates/**`/`ci/**`/`Cargo.*`/判据正本/`strategies/**`/`proofs/**`/
  `.gitea/**`/`poc/**`；真合并形态看冲突解法；`ADVISORY-WARN`；rebase 留痕；门面被改；
  `review_points` 非空）。
- **§七 失败怎么办**：红 ⇒ 停（不许 push/合 PR/跳过复核/改记录件凑绿）；
  按 reason_code 回步（表列全）；★ **记录件不许删、不许覆盖**（被拒的照样留档，
  重写用新文件名）；复核输出与门产物同样不许删。
- **§八 与现有合流事实的一致性**：见下 ④。

---

## ③ ★ 削弱变体自证读数（逐变体逐条）

命令：`python3 -I poc/poc-t7-2-integrator/selftest.py --out poc/poc-t7-2-integrator/raw`
（夹具仓真跑门 ⇒ 正常记录件是**真记录**，非手编；git 读数全来自夹具仓真实提交）。
自证脚本自身在任一变体仍绿时判红 exit 1。

**读数（原始件 `raw/selftest-readings.json`）：总 20 例 = 1 绿 + 19 红，`SELFTEST-GREEN`，`failures: []`**

| # | 变体 | exit | reason_code | 期望 |
|---|---|---|---|---|
| 0 | **正常记录件（未削弱）** | **0** | `OK` | 绿 ✓ |
| 1 | `base_sha` 改一位 | 1 | `BIND-MISMATCH` | 红 ✓ |
| 2 | `source_sha` 改一位 | 1 | `BIND-MISMATCH` | 红 ✓ |
| 3 | `tested_tree` 换成另一棵树（基线树的） | 1 | `BIND-MISMATCH` | 红 ✓ |
| 4 | `suite_digest` 改一位 | 1 | `BIND-MISMATCH` | 红 ✓ |
| 5 | `verdict` = `"fail"` | 1 | `VERDICT-NOT-PASS` | 红 ✓ |
| 6 | `verdict` = `"PASS"`（**大小写差异**） | 1 | `VERDICT-NOT-PASS` | 红 ✓ |
| 7 | `verdict` = 空串 | 1 | `VERDICT-NOT-PASS` | 红 ✓ |
| 8 | `base_sha` 空串 | 1 | `KEY-EMPTY` | 红 ✓ |
| 9 | `base_sha` 缺字段 | 1 | `KEY-MISSING` | 红 ✓ |
| 10 | `source_sha` 空串 | 1 | `KEY-EMPTY` | 红 ✓ |
| 11 | `source_sha` 缺字段 | 1 | `KEY-MISSING` | 红 ✓ |
| 12 | `tested_tree` 空串 | 1 | `KEY-EMPTY` | 红 ✓ |
| 13 | `tested_tree` 缺字段 | 1 | `KEY-MISSING` | 红 ✓ |
| 14 | `suite_digest` 空串 | 1 | `KEY-EMPTY` | 红 ✓ |
| 15 | `suite_digest` 缺字段 | 1 | `KEY-MISSING` | 红 ✓ |
| 16 | 门摘要被抹（**门不可跑**） | 1 | `GATE-SUMMARY-ABSENT` | 红 ✓ |
| 17 | 门摘要被改（摘要不自证） | 1 | `GATE-SUMMARY-DIGEST` | 红 ✓ |
| 18 | 门退出码非零 | 1 | `GATE-EXIT-NONZERO` | 红 ✓ |
| 19 | 记录件缺席 | 1 | `RECORD-ABSENT` | 红 ✓ |

★ 全程**未为让任何变体绿而放宽判定**：verdict 判定是 `!= "pass"` 逐字比
（`"PASS"` 红）；空值/缺字段各自独立 reason_code；门不可跑是**拒绝**而非跳过。

正常记录件那次复核的实测侧读数（夹具；**夹具每次重建 ⇒ base/source 的 sha 每次不同**，
树与门集摘要稳定，因夹具内容固定）：
`base_sha=cec30c45a8bb…` / `source_sha=b3de81100d68…` / `tested_tree=d71a142c11b8…`
/ `suite_digest=b9e2ff4a6f3e…`（门集成员 3 件），落地树 == 候选树（fast-forward），
候选树改动面 `["crates/demo/extra.rs"]`。
（上表 sha 取自本次留档的 `raw/selftest-readings.json`；复跑时 sha 会变，属夹具重建，
不影响任何判据结论。）

---

## ④ ★ 真实历史合流核验读数

命令：`bash poc/poc-t7-2-integrator/history_check.sh`（原始件 `raw/history-readings.txt`）。
★ 本脚本**不写任何记录件**（禁事后伪造）——历史合流只能给读数。

### 4.1 历史记录面核证

- `git log --all --oneline --name-only -- '*integrator*'` ⇒ **空**（全史无 integrator 相关文件）；
- `git ls-files | grep -iE 'integrator|gate_result|gate-result'` ⇒ **空**（仓内无记录类跟踪件）；
- ⇒ **历史合流无等价记录面**。复核器对历史合流必然拒绝（`RECORD-ABSENT`）——
  这是设计意图，不是缺陷。

### 4.2 逐笔真实读数（复核器 `actual` 子命令自算）

| 合流 | 时间 | 被合分支 | base_ref（合流时目标头） | source（合流时分支头） | base_sha（fork 点） | tested_tree | 落地树 vs 候选树 |
|---|---|---|---|---|---|---|---|
| `83bb71c` | 2026-11-04 04:57 | `feature/main-integration`（集成批**快进**：product/main 由 `0bb3b14` → `83bb71c`） | `0bb3b14` | `83bb71c` | `06f76d0d3e83…` | `ee4a1ed6fda9…` | **相等** ⇒ 覆盖形态成立 |
| `ae6869d` | 2026-11-04 21:03 | `feature/poc-e-codeadapter`（**真合并提交**直并入 product/main） | `e106b3f`（= 合并提交第一父） | `f14e79e` | `83bb71c12e80…` | `3516c675b2eb…` | **不等** ⇒ 落地内容整体没过门 |
| `df7c6e3` | 2026-11-04 22:33 | `feat/l3-l3-independent-port`（真合并提交） | `b0cfa8e` | `b99bbb9` | `8ef705d0e800…` | `cc6c9b6c3024…` | **不等** |
| `e448453` | 2026-11-04 22:33 | `fix/gate-inputs-repin`（真合并提交） | `df7c6e3` | `0e0e76a` | `8ef705d0e800…` | `f7e89ced196a…` | **不等** |

- 三笔真合并的**被合分支现在的远端头**仍等于合流时的第二父
  （`feature/poc-e-codeadapter=f14e79e`、`feat/l3-l3-independent-port=b99bbb9`、
  `fix/gate-inputs-repin=0e0e76a`）⇒ 复核器"远端该分支头"的读数可逐位复现历史 source_sha。
- 四笔的**机算 ort 合并树 == 真实合并提交树**（逐位相同：
  `be3a91c1…` / `bd6306ba…` / `3ebe27d7…` / `c347b604…`）⇒ 历史合流都是干净的 ort 合并
  （无 evil merge 夹带）。
- ★ **`suite_digest` 的历史限制（如实）**：历史时点**没有任何记录侧摘要值**
  （全史无记录件），且本批的集合定义 `af.gate_suite_digest.v1` 当时并不存在 ⇒
  **历史值不可反推为"当时记下的值"**。读数里出现的门集摘要只是**按本批定义在历史树上
  反算的重建值**（如 `ae6869d` 的 `400ac09f58b3…`（源树）/ `fee9564b9d95…`（合并树）），
  **不冒充**历史记录值。

---

## ⑤ 门读数与边界

### 5.1 门读数（原始件 `raw/gate-cargo-fmt.txt`、`raw/gate-checks.txt`）

| 门 | 命令 | 读数 |
|---|---|---|
| 1 | `cargo fmt --check --all` | **exit 0**（绿；cargo 1.99.1 / rustfmt 1.10.0-stable；本批不改 Rust ⇒ 这门必绿） |
| 2 | `python3 -I poc/poc-t7-2-integrator/selftest.py` | **exit 0**（`SELFTEST-GREEN`；1 绿 + 19 红，见 ③） |
| 2b | 复核器在**真实 af 仓 + 真实远端**上的读数 | `actual --base-ref origin/product/main --branch feature/poc-e-codeadapter`：`source_origin=origin refs/heads/feature/poc-e-codeadapter 的远端头`、`source_sha=f14e79e1123f…`（= 历史合流 `ae6869d` 的第二父，逐位复现）、`tested_tree=3516c675b2eb…`、`base_sha=f14e79e…`（该支已并入当前主线 ⇒ 与现主线的 merge-base 即分支头）、`suite_digest=80e7578c9d6f…`（合并树 44 件）/ `400ac09f58b3…`（源树 42 件）、`merge_tree=13883f0d1e29…`、`merge_mode=merge-commit`。<br>`verify`（**无记录件**）⇒ **exit 1 / `RECORD-ABSENT`** —— 历史合流无记录面 ⇒ 复核**拒绝**，不是跳过 ✓ |
| 3 | af 主仓零改动核 | `git status --porcelain` 只有开工前既存的 `?? redteam/scans/2026-11-04-e4484537/`；主仓 HEAD = `56ff52a`（与开工时同值）；`product/main` = `56ff52a`（禁改 ✓）；远端 `product/main` = `56ff52a`（**禁 push：未推任何东西** ✓）；★ **本地分支数 = 94，其中本批仅新增 1 条**（`feat/t7-2-integrator-bind-check`，指向 `56ff52a`，未 commit）——<br>★ 「开工前 93」**无可独立复核的快照**（返修批如实登记：初版 README 写「93→94」但未留快照）⇒ 本批**改写为「当前 94，本批仅新增 1 条」**，不再引用无来源的「开工前 93」；PoC-E 参考件 blob `ebc01dac0da8…`（origin 版 == 主仓工作树版 ⇒ **逐字未动** ✓） |
| 4 | 判据面零弱化 | `git diff origin/product/main -- ci/` **空**（本批唯一新增件是未跟踪的 `ci/integrator_bind_check.py`，未改任何既有 ci/ 件）；`ci/run.sh` **零 diff**（⇒ 未新增 step 编号）；`ci/clippy-baseline.txt` sha256 `a48639e75b52…` 前后一致 ✓；`crates/` **零 diff**、crate 数 22→22、**零新增 crate** ✓ |
| 5 | `cargo build` / `cargo test` / `cargo clippy` | ★ **未跑**——本批硬约束**禁改 `crates/`** ⇒ 这三项产物由主线全量门复验批承担（anolis 在跑，tip `56ff52a`）；且本机 14G 内存禁高并发编译，不在本地跑重编译。**不写成"已跑"。** |

### 5.2 如实登记的边界

1. **机器不判"记录件是不是真门产出的"**：复核器核的是「记录侧 ↔ 实测侧」一致性 ＋
   **回读**门摘要文件的三方自证（磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文）
   ＋门退出码 + step 词表；
   "这份门摘要文件确实出自某次真门"仍属**流程纪律**（SOP §四：谁在何时写、门产物同目录留证、
   门产物不许删）——文件与记录件一致**不等于**文件是某次真跑的门产出的；
   机器仲裁员首版不启用 ⇒ 机器不越权裁决。
2. **门套件集合只覆盖 F2 门执行面**（`ci/` ＋ `.gitea/workflows/`）。门的**数据输入**
   （仓根判据正本 `deliverables.yaml`/`numbers.json`/`doc-claims.txt`/`deny.toml`
   ＋ `strategies/` ＋ `poc/` 资产）在"候选树那一侧"由 `tested_tree` 绑定；
   "主线侧前移"由 `GATE-BASE-DRIFT` ＋ `COVERAGE-TESTED-TREE` 兜住（⇒ 触发重跑门）。
3. **真合并形态的冲突解法由人工判读**（SOP §六 b）；复核器机算的 `merge_tree` 只保证
   "无冲突时的 ort 结果"，人工解冲突改了内容 ⇒ SOP 步 8 的合并树核对会红。
4. **复核器未进 `ci/run.sh`**（禁新增 step 编号）：它是**独立脚本 ＋ SOP**，
   由 integrator 在合流前手动跑一次（秒级）。
5. **`suite_digest` 随门面新增而变**：新增门脚本 ⇒ 成员数变 ⇒ 在合流的候选树必须
   **重跑门**（设计意图，非缺陷）。
6. **历史合流无记录面**（见 ④）：本批**不**为历史合流补造记录件；历史只能如实登记。
7. **主仓既存 `ci/__pycache__/clippy_gate.cpython-314.pyc`**（2026-11-03，**非本批产生**）；
   本工作树**零** `__pycache__`/`*.pyc`（复核器与自证脚本均以 `python3 -I` 运行、
   只作为 `__main__` 执行 ⇒ 不产字节码；返修批新增的 `r2_repro.py` 因要 import 同目录的
   `selftest.py`（复用夹具构造）⇒ 在 import **之前**置 `sys.dont_write_bytecode = True`
   ⇒ 同样不落字节码）。

### 5.3 本目录内的可复跑脚本

| 文件 | 作用 |
|---|---|
| `selftest.py` | 削弱变体自证（③）＋**返修批新增**（真实词表 / 三方比对 / `--source` 绕口）；`python3 -I poc/poc-t7-2-integrator/selftest.py --out <dir> --tag=-r2` |
| `r2_repro.py` | **返修批新增**：复核员实测场景的独立复现（初版假红 + 初版洗白路径）；`python3 -I poc/poc-t7-2-integrator/r2_repro.py --out <dir>` |
| `history_check.sh` | 真实历史合流核验（④）；`bash poc/poc-t7-2-integrator/history_check.sh` |
| `gate_checks.sh` | 门读数与零改动核（⑤）＋**返修批新增**（门 2a 复现面、门 4b 词表逐字一致核）；`bash poc/poc-t7-2-integrator/gate_checks.sh` |
| `raw/` | 原始读数：`selftest-readings.json`（初版，**未改**）、`history-readings.txt`（**未改**）、`gate-cargo-fmt.txt`（**未改**）、`gate-checks.txt`（**未改**）＋返修批新增 `selftest-readings-r2.json`、`r2-repro-readings.{json,txt}`、`gate-cargo-fmt-r2.txt`、`gate-checks-r2.txt`、`selftest-v1.py`（初版 selftest 逐字快照，sha256 `bf6e0a2d…`） |

---

## ⑥ ★ v0.3-T7② 返修批（两项必修补 + 三项建议）—— 追加段

> 本段为**返修批追加**，§①–§⑤ 的初版读数与结论**原样保留、未改**；
> 初版自证脚本逐字快照 = `raw/selftest-v1.py`（sha256 `bf6e0a2d51cab4d269bed19bd82bab45f7ff3342f9d658f3649cf4d05ddf3d2f`）。
> 返修后自证脚本新增用例 ⇒ 读数落 `raw/selftest-readings-r2.json`（**不覆盖**初版件）。

### 6.1 必修①：verdict 词表漏 `PASS-WITH-WARN` ⇒ 合法绿门被当红（已修）

- **位置**：`ci/integrator_bind_check.py:138`（原 `:121`）。
- **改前**：`STEP_STATUS_ALLOWED = frozenset({"PASS", "ADVISORY-PASS", "ADVISORY-WARN"})`
- **改后**：`STEP_STATUS_ALLOWED = frozenset({"PASS", "PASS-WITH-WARN", "ADVISORY-PASS", "ADVISORY-WARN"})`
- **依据**：`ci/run.sh:613-618` —— step14 rc=2（advisory WARN）⇒ `b14_verdict="PASS-WITH-WARN"`；
  `run.sh:618` **只在 rc==1** 时把 OVERALL 置 1 ⇒ 该状态**门 exit 0 是绿**。
- **★ 判据只许变严的证明**：新增的白名单词**只多收一个真绿状态**；
  `FAIL`（`ci/run.sh:68`）与 `ADVISORY-FAIL`（`ci/run.sh:480`）**仍不在白名单 ⇒ 仍红**
  （逐条读数见 6.3；`ci/run.sh:476-489` 的「未消化的 ADVISORY-FAIL 不得结案」纪律不变）。
  ★ **初版假红读数（`emit` exit=2）是复核员实测**（任务书 §三①），本返修员**未重测**
  （初版代码已就地返修、覆盖）；返修后读数见 6.3/6.5。
- **连带加严**（不在任务书点名的范围内，但同一漏项的同源面，一并收严）：
  - 复核输出的人工看点新增 `PASS-WITH-WARN` 行（`ci/integrator_bind_check.py:590-594`）——
    step14 的 advisory WARN **仍须人眼看** `ci/out/step14.log`；
  - SOP §6.1「可自动判过路径」新增前置：**门摘要无 `PASS-WITH-WARN` 行**
    ⇒ 带 WARN 的门不许走自动判过路径（`docs/INTEGRATOR-SOP.md` §六）。

### 6.2 必修②：摘要自证链漏洞（`verify` 从不回读摘要文件）—— 已修

- **位置**：`ci/integrator_bind_check.py:470-495`（回读与三方比对）、
  `:328-356`（`resolve_gate_summary_path`：定位要回读的文件）、
  `:358-371`（`read_gate_summary_file`：回读，fail-closed）。
- **改前**：`cmd_verify` 只用记录件**内嵌**的 `gate_summary_text` 及其 sha256 自证
  ⇒ 复核员实测「改内嵌摘要为全 PASS 行并同步重算 sha256」⇒ **verify exit=0（reason=OK）**。
- **改后**：`verify` **强制回读**门摘要文件（`gate_summary_path` 指向的那份；也可
  `--gate-summary <路径>` 显式指定），做**三方比对**：
  **磁盘文件 sha256 ↔ 记录件内嵌 sha256 ↔ 记录件内嵌原文**；任一不一致 ⇒
  `GATE-SUMMARY-DIGEST` **拒绝**；文件读不出 / 记录件缺路径 ⇒ `GATE-SUMMARY-ABSENT` **拒绝**。
- **★ 缺省行为（README + SOP 都写了）**：**缺省＝回读，且是强制的、无开关可绕**；
  SOP §一.3 与 §三 步 4/步 5 均明写「★ 复核**强制回读**门摘要文件……**未回读 ⇒ 门摘要自证
  这条路径不成立**」。
- **★ 激励路径已封死**（复核员指出「合法绿门被误拒 ⇒ 改内嵌摘要洗掉 WARN 行即得绿」）：
  洗白路径现在必红（读数见 6.4 与 6.5 复现 2）。

### 6.3 真实词表逐条绿/红读数（`raw/selftest-readings-r2.json`）

自证脚本 `selftest.py` 新增第 ④ 组用例：按 `ci/run.sh` 的真实 SUMMARY 词表逐词造摘要，
走**真跑**的夹具门 ⇒ 逐条读数（`emit` 必须能写出记录件 ＋ `verify` 必须绿 / 红）：

| 词 | `emit` 读数 | `verify` 读数 | 判定 |
|---|---|---|---|
| `PASS` | exit **0**（写出记录件） | exit **0** / `OK` | 绿 ✓ |
| `ADVISORY-PASS` | exit **0** | exit **0** / `OK` | 绿 ✓ |
| `ADVISORY-WARN` | exit **0** | exit **0** / `OK` | 绿 ✓ |
| `PASS-WITH-WARN` | exit **0** | exit **0** / `OK` | 绿 ✓（**返修前 `emit` exit=2 假红**） |
| `FAIL` | exit **2**「门摘要判不出绿（不许写绿记录件）：step 非绿: step14  FAIL …」 | exit **1** / `GATE-VERDICT` | 红 ✓ |
| `ADVISORY-FAIL` | exit **2**「…step 非绿: step9  ADVISORY-FAIL …」 | exit **1** / `GATE-VERDICT` | 红 ✓ |

- 词表绿样本走**真跑**的夹具门（`ci/run.sh` 是 fixture 里的真脚本，真写 SUMMARY），
  git 读数全来自夹具仓真实提交。
- `verify` 侧的红样本是**手编记录件**（内嵌红词摘要 + **同步重算 sha256** + 指向同一文件）
  ⇒ 回读三方一致 ⇒ **只能靠词表判红** ⇒ `GATE-VERDICT` ⇒ 证明词表本身在拦。

### 6.4 摘要三方比对逐条读数（`raw/selftest-readings-r2.json`）

| 用例 | 期望 | 实测 exit / reason_code |
|---|---|---|
| 基线一致（文件 ↔ 内嵌 sha ↔ 内嵌原文） | 绿 | exit **0** / `OK` |
| **磁盘文件被改**（记录件不动） | 红 | exit **1** / `GATE-SUMMARY-DIGEST` |
| **内嵌摘要被改成全 PASS 行且 sha 同步重算**（磁盘文件不动） | 红 | exit **1** / `GATE-SUMMARY-DIGEST` |
| 内嵌 sha256 被改（与内嵌原文不符） | 红 | exit **1** / `GATE-SUMMARY-DIGEST` |
| 记录件缺 `gate_summary_path`（且不给 `--gate-summary`） | 红 | exit **1** / `GATE-SUMMARY-ABSENT` |
| 门摘要文件缺席 | 红 | exit **1** / `GATE-SUMMARY-ABSENT` |
| `--gate-summary` 显式指向**异内容**文件 | 红 | exit **1** / `GATE-SUMMARY-DIGEST` |

### 6.5 复核员实测场景的独立复现（`raw/r2-repro-readings.txt` / `.json`）

命令：`python3 -I poc/poc-t7-2-integrator/r2_repro.py --out poc/poc-t7-2-integrator/raw`
⇒ exit **0**，`R2-REPRO-GREEN`，`failures=[]`：

★ **口径说明**：下表「初版读数」一列 = **复核员实测**（原样引自任务书
`30-工件/外派任务书/三项返修批-G1-T7②-T5②T6③.md` §三①②），
**不是**本返修员重测的（初版代码已在原工作树**就地**返修、覆盖）⇒ 本返修员**独立重跑的是
「返修后读数」一列**；「初版读数」列在返修后**不可能再复现**（代码已改）。

| 复现 | 场景 | 初版读数（复核员实测，任务书 §三） | 返修后读数（本返修员实跑） |
|---|---|---|---|
| 1 | 门摘要 `step1 PASS / step9 ADVISORY-PASS / step14 PASS-WITH-WARN` ⇒ `emit` | **exit 2**「门摘要判不出绿：step 非绿: step14 PASS-WITH-WARN」（**合法绿门被当红**） | **exit 0**（写出记录件）+ `verify` **exit 0 / `OK`** |
| 2 | 把内嵌摘要洗成全 PASS 行并**同步重算 sha256**（磁盘文件不动）⇒ `verify` | **exit 0 / `OK`**（门摘要原文可被记录件作者完全控制） | **exit 1 / `GATE-SUMMARY-DIGEST`** |
| 3 | 把**磁盘上的**门摘要文件改掉（记录件不动）⇒ `verify` | （初版不回读 ⇒ 不受影响，绿） | **exit 1 / `GATE-SUMMARY-DIGEST`** |
| 4 | `verify --source <本地提交>`（不带 `--allow-local-source`） | **exit 0**（绕开「远端该分支头」绑定） | **exit 1 / `LOCAL-SOURCE-NOT-ALLOWED`** |
| 5 | `verify --source <本地提交> --allow-local-source` | —（无此开关） | **exit 0 / `OK`**，且输出留 `source_origin=本地提交 …（--allow-local-source；**未绑定远端头**）` |
| 6 | 红词表（`step14 FAIL` / `step9 ADVISORY-FAIL`）⇒ `emit` | exit 2（拒写） | exit **2**（拒写，**逐字未变**） |

### 6.6 三项建议的处置

| 建议 | 处置 | 落点与读数 |
|---|---|---|
| ① `--source` 绕口 | **做**：verify 侧 `--source` 必须配 `--allow-local-source`，否则 `LOCAL-SOURCE-NOT-ALLOWED`；记录件与复核输出都留 `source_origin`；用了本地提交还会在 `review_points` 里点名「须人工确认该提交已推送」 | `ci/integrator_bind_check.py:499-510`、`:595-599`、`:614-618`、`:658-661`、`:693`、`:820-829`；读数 6.5 复现 4/5 ＋ 自证「source绕口-未加allow-local-source」「source绕口-加allow-local-source」 |
| ② PIPELINE-GREEN 披露缺口 | **做**：README §1.1 与 SOP §一.3 各补一段「机制差异 + 理由 + 四项语义未放宽」；SOP §一.3 的词表与代码 `STEP_STATUS_ALLOWED` **逐字一致**（门 4b 机核） | README §1.1；`docs/INTEGRATOR-SOP.md` §一.3；读数：门 4b「代码 STEP_STATUS_ALLOWED = ['ADVISORY-PASS','ADVISORY-WARN','PASS','PASS-WITH-WARN']；SOP §一.3 未逐字列出的词 =（无）⇒ 逐字一致 ✓」 |
| ③ 分支数「93→94」无快照 | **做（改写）**：「开工前 93」**无可独立复核的快照** ⇒ 改写为「**当前 94，本批仅新增 1 条**」（`feat/t7-2-integrator-bind-check` → `56ff52a`，未 commit），并**删除**对「开工前 93」的引用 | README §5.1 门 3；`gate_checks.sh` 门 3 同改；读数：门 3「主仓本地分支数 = 94」＋ 本批分支指向 `56ff52a` |

### 6.7 返修批门读数

| 门 | 命令 | 读数 |
|---|---|---|
| 1 | `cargo fmt --check --all` | **exit 0**（绿；`raw/gate-cargo-fmt-r2.txt`） |
| 2 | `python3 -I poc/poc-t7-2-integrator/selftest.py --out poc/poc-t7-2-integrator/raw --tag=-r2` | **exit 0**；`SELFTEST-GREEN`，`failures=[]`；**41 例 = 11 绿 + 30 红，逐例全按预期**（`raw/selftest-readings-r2.json`） |
| 2a | `python3 -I poc/poc-t7-2-integrator/r2_repro.py --out poc/poc-t7-2-integrator/raw` | **exit 0**；`R2-REPRO-GREEN`，`failures=[]`（`raw/r2-repro-readings.txt`） |
| 2b | 复核器在真实 af 仓 + 真实远端上的读数 | `actual --base-ref origin/product/main --branch feature/poc-e-codeadapter` 照旧出读数（`source_origin=origin refs/heads/… 的远端头`）；`verify`（**无记录件**）⇒ **exit 1 / `RECORD-ABSENT`**（历史合流无记录面 ⇒ 拒绝，不是跳过）—— 读数见 `raw/gate-checks-r2.txt` 门 2b |
| 3 | af 主仓零改动核 | `git status --porcelain` 只有开工前既存的 `?? redteam/scans/2026-11-04-e4484537/`；主仓 HEAD = `56ff52a`；`product/main` = `56ff52a`（禁改 ✓）；远端 `product/main` = `56ff52a`（禁 push ✓）；**本地分支数 = 94，本批仅新增 1 条**；PoC-E blob `ebc01dac0da8…` 逐字未动 ✓ |
| 4 | 判据面零弱化 | `git diff origin/product/main -- ci/` **空**（唯一新增件是未跟踪的 `ci/integrator_bind_check.py`）；`ci/run.sh` **零 diff**（未新增 step 编号）；`ci/clippy-baseline.txt` sha256 `a48639e75b52…` 前后一致 ✓；`crates/` **零 diff**、crate 数 22→22 ✓ |
| 4b | SOP §一.3 词表 vs 代码 `STEP_STATUS_ALLOWED` | **逐字一致 ✓**（门 4b 机核，脚本在 `gate_checks.sh`） |
| 5 | `cargo build` / `cargo test` / `cargo clippy` | ★ **未跑**——本批硬约束**禁改 `crates/`** ⇒ 这三项产物由主线全量门复验批承担（anolis 在跑，tip `56ff52a`）；且本机 14G 内存禁高并发编译，不在本地跑重编译。**不写成「已跑」。** |

### 6.8 返修批如实登记的边界

1. **回读只证明"文件与记录件一致"，不证明"文件是某次真跑的门产出的"**：
   复核器读的是**磁盘上那份门摘要文件**，与记录件内嵌摘要三方一致 ⇒ 记录件作者
   **不能再单方面改内嵌摘要洗白**；但"这份 SUMMARY.txt 确实是某次真跑的门写的"
   仍是**流程纪律**（SOP §四/§七：谁在何时写、门产物不许删）——机器仲裁员首版不启用 ⇒ 不越权裁决。
2. **回读路径依赖门摘要文件仍在盘**：门产物被删 ⇒ `GATE-SUMMARY-ABSENT` ⇒ 复核红
   （这是设计意图，与 SOP §七「门产物不许删」同向；代价是**归档件若丢了 SUMMARY.txt 就复核不过**，
   须重跑门 + 重写记录件）。
3. **`--allow-local-source` 是**显式承认**的例外**：它允许以本地提交作候选头（远端绑定被跳过），
   但会在复核输出里留 `source_origin=本地提交 …` 与一条 `review_points` ⇒ 事后可区分；
   SOP §六 把它列入「必须人工看」，且它**不**进 §6.1 自动判过路径。
4. **本返修批**未改 `crates/**`、`ci/` 既有件、`Cargo.toml`/`Cargo.lock`、
   `af/poc/poc-e-codeadapter/**`、既有测试断言；**未新增 `ci/run.sh` step 编号**；
   **未 commit、未 push、未 amend/rebase、未 sudo、未改 `product/main` 指针**。
5. **初版读数与初版自证脚本**：原 `raw/selftest-readings.json`、`raw/gate-checks.txt`、
   `raw/gate-cargo-fmt.txt`、`raw/history-readings.txt` **逐字未改**（sha256 见 6.8 尾注），
   初版 `selftest.py` 逐字快照存 `raw/selftest-v1.py`；返修后用例的读数是**新增**件。
   ★ 初版 `raw/selftest-readings.json` 由**初版** `selftest.py` 产出（返修后脚本新增了用例，
   复跑会多出 §6.3/6.4/6.5 的用例 ⇒ 落 `raw/selftest-readings-r2.json`）。

> 原读数件 sha256（返修批未改）：
> `raw/selftest-readings.json` = `6f741cd45c6e9d2ac8f35472636fab165f0540b0547394f9d464b97f3747c0b6`；
> `raw/gate-cargo-fmt.txt` = `135baa99f7541c92509f7fe2482ac46d2a318dd3e7272e8b591686f9a72caed9`；
> `raw/gate-checks.txt` = `d2bd8a19b05cc989ed3191acb423ceea65541594c7493619e46da9372d6d4f33`；
> `raw/history-readings.txt` = `286d52e148fff93914eecd08eff105a1ddef21422b092e3f5ef25e2442b3a6e1`。

# integrator 复核制 · 合流 SOP（v0.3-T7② 常设化）

> 落地件：`ci/integrator_bind_check.py`（复核器）＋本文（流程）＋
> `poc/poc-t7-2-integrator/`（自证与读数）。
> 依据：方案裁决 `计划/产品化工作方案-v0.3-20261103.md:71`（T7 终裁 2）
> 「**候选树门覆盖 = integrator 复核制**（串行度=1）：绑定
> base_sha+source_sha+tested_tree+suite_digest，后到补丁 rebase 后先合时
> integrator 确认 tested_tree == 合并树，不等价重跑 F1。**机器仲裁员首版不启用**。」
> 语义来源（只读参考，未改）：PoC-E `af/poc/poc-e-codeadapter/codeadapter.py:144`
> `four_binding_check`；verdict 词汇表来源：`crates/af-workorder/src/gate_admit.rs:55`
> （`if item.verdict != "pass"` —— 严格等于，大小写敏感）。

---

## 〇、效力边界（先读这一节）

- **本流程是「人工 integrator 按 SOP 复核 + 机器辅助核对」**。
  复核器 `ci/integrator_bind_check.py` 的 exit 0 **只表示机器侧未发现不符**，
  **不构成合流裁决**；合流与否由人工 integrator 依本文决定。
  ★ **机器仲裁员首版不启用**（方案裁决明文）：本流程**没有**「机器自动裁决合入」
  这一步，也**不得**被改造成有。
- **判据只许变严**：复核器只做「多拦住不匹配」的事。任何对既有判定的放宽
  （例如把 verdict 改成大小写不敏感、把空值当缺省放过、把「门不可跑」当
  「跳过复核」）都属**违规**，一经发现即本流程作废重做。
- **★ 禁「门不可跑就跳过复核」**：门不可跑 ⇒ **复核拒绝**（`GATE-*` 系 reason_code），
  不是跳过。历史沿革与依据见 PoC-E `codeadapter.py:144-153` 头注同款纪律。

---

## 一、四项绑定：记录侧 / 实测侧各是什么

| 键 | 语义 | 记录侧（expected）从哪来 | 实测侧（actual）复核器怎么算 |
|---|---|---|---|
| `base_sha` | 分支基点：分支与目标分支的 **fork 点**（= `merge-base`）。分支确从记录的 main 头分出。单提交分支时它就是分支父提交（与 PoC-E 的 `HEAD~1` 同值） | 门跑完写记录件时，**复核器自算**（`merge-base <base_ref> <source_sha>`），禁手抄 | `git merge-base <base_ref> <source_sha>` |
| `source_sha` | 分支头：**远端**该分支头（推送的就是测过的那个提交） | 同上，自算 | `git ls-remote <remote> refs/heads/<branch>` |
| `tested_tree` | 测过的树：`source_sha^{tree}`（合入内容逐位 = 门跑过的内容） | 同上，自算 | `git rev-parse <source_sha>^{tree}` |
| `suite_digest` | 门套件：门执行面的**文件内容摘要**（门没被换） | 同上，自算 | 同一算法在**落地树**上重算（见 §一之 2） |
| 另：`verdict` | 门结论，**严格等于** `"pass"` | 记录件字段（由**回读**的门摘要文件自证，见 §一.3） | 与记录侧逐字比；`"PASS"`/`"fail"`/`""` 一律拒 |

### 1. fail-closed 语义（一条都不许放宽）

复核器 **exit 1（拒绝）** 的全部情形：

- 记录件**缺席 / 不可解析 / schema 不认**（`RECORD-*`）——**不许**因为"没找到记录"
  就当"没这回事"放行；
- 四项中任一**缺字段 / 空串 / 形态不合法**（`KEY-MISSING` / `KEY-EMPTY` / `KEY-MALFORMED`）；
- 四项中任一**不符**（`BIND-MISMATCH`）；
- `verdict` **不等于** `"pass"`（`VERDICT-NOT-PASS`，大小写敏感）；
- **门不可跑**：门摘要缺席 / 摘要 sha256 与内嵌原文不符 / **回读三方不一致** /
  摘要里判不出绿 / 门退出码非零（`GATE-SUMMARY-ABSENT` / `GATE-SUMMARY-DIGEST` /
  `GATE-VERDICT` / `GATE-EXIT-NONZERO`）——**拒绝，不是跳过**；
- **候选头来源**：用 `--source` 指定本地提交而未显式承认（`LOCAL-SOURCE-NOT-ALLOWED`）——
- **主线推进面**：门跑完后目标分支头前移（`GATE-BASE-DRIFT`）；
- **候选树门覆盖不成立**：落地树 ≠ 记录侧 `tested_tree`（`COVERAGE-TESTED-TREE`）；
- **门面漂移**：候选树与落地树的门执行面摘要不等（`SUITE-DRIFT`）；
- 合并树机算冲突（`MERGE-TREE-CONFLICT`）；远端读不出（`REMOTE-*`）；ref 解析不出
  （`REF-UNRESOLVED` / `MERGE-BASE-UNRESOLVED`）。

### 2. `suite_digest` 的集合定义（为什么是这些）

- **集合**（树内枚举，路径升序）：
  - `ci/` 下的全部跟踪文件（**门执行面**：门脚本 + 判据正本 —— clippy 基线、
    白名单、词表、golden 册、debt 册、门输入钉、判据 yaml……全在 `ci/` 下）；
  - `.gitea/workflows/` 下的全部文件（F1 快速门载体；F1/F2 两门同属"门"）。
- **排除**：`ci/out/**`（门跑出来的**产物**，每次跑门都变 ⇒ 纳入则摘要恒变）、
  `ci/__pycache__/**`（字节码缓存）、`ci/advisory-db/**`（外部语料，未入库）、
  末段为 `.bak` 的文件（历史备份件，门执行面不引用）。
- **理由**：
  1. **覆盖完整**：门脚本与判据正本都在 `ci/` 下 ⇒「门没被换」不必逐件枚举就不会漏面；
     少列一件 = 给逃逸留门。
  2. **免漂移**：成员由**树枚举**得出，不靠手写清单 ⇒ 将来新增门脚本自动入集合，
     不会出现"新门脚本上线但摘要不变"的空子。
  3. **排除项有据**：`ci/out/**` 是产物、`.bak` 是备份，纳入只会造"摘要恒变"的假红。
  4. **与仓内既有纪律同向**：`ci/run.sh:136-152` 头注已明写「改判据正本即改门」，
     本集合把这句话变成可机算的摘要。
- **算法**（确定性，双方可独立复算）：
  `digest = sha256( 逐成员: path + "\0" + sha256(blob 内容) + "\n" 拼接 )`。
  审计用：`python3 -I ci/integrator_bind_check.py digest --tree <rev> [--list-paths]`。

### 3. 门结论不自证 ⇒ 不能当绿

记录件内嵌**门 SUMMARY 原文 + 其 sha256 + 门退出码**。复核器**回读**门摘要文件
（`gate_summary_path` 指向的那份），把「**磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文**」
**三方比对**，再按 `ci/run.sh` 的 SUMMARY 词表判绿 ⇒ **"pass" 不是人手写出来的字**。

**★ 词表（与代码 `STEP_STATUS_ALLOWED` 逐字一致；改一处必须同批改另一处）**：

| 词 | 判定 | 依据（`ci/run.sh`） |
|---|---|---|
| `PASS` | 绿 | 步骤 rc=0（`ci/run.sh:65`） |
| `PASS-WITH-WARN` | 绿 | step14 机读一致性批 rc=2=advisory WARN（`ci/run.sh:615`）；`ci/run.sh:618` **只在 rc==1** 时把 OVERALL 置 1 ⇒ 门 exit 0 仍绿 |
| `ADVISORY-PASS` | 绿 | step9 FV advisory 跑了且判据全绿（`ci/run.sh:477`） |
| `ADVISORY-WARN` | 绿 | step9 工具缺席（coqc/Kani 不在）（`ci/run.sh:483`） |
| `FAIL` | **红** | 步骤 rc!=0（硬红 ⇒ OVERALL=1 ⇒ 门 exit 1）（`ci/run.sh:68`） |
| `ADVISORY-FAIL` | **红** | step9 跑了且 9a/9b 判据红（`ci/run.sh:480`）——仓内纪律：存在未消化的 `ADVISORY-FAIL` 不得结案（`ci/run.sh:476-489`） |
| 其他任何词 | **红** | 未知词一律非绿（fail-closed） |

**★ 与 PoC-E 的机制差异（披露）**：PoC-E `codeadapter.py:272` 判 verdict 的口径是
**显式绿标**（`verdict = "pass" if "PIPELINE-GREEN" in summary else "fail"`）；
本器判的是 **step 状态词表**。真实 `ci/run.sh` **从不写** `PIPELINE-GREEN`
（全仓 `grep PIPELINE-GREEN ci/run.sh .gitea/` 无命中）⇒ 若照抄 PoC-E 口径，
**真实绿门恒红**（假红）⇒ 词表口径是必要适配；**四项绑定判定语义未放宽**
（四项任一不符/为空/缺项 ⇒ 拒），白名单只收**真绿**状态，`FAIL`/`ADVISORY-FAIL` 仍红。

**★ 回读是强制的**（`verify` 无开关可绕）：内嵌摘要由记录件作者完全可控 ⇒
不回读就等于"门摘要原文"可自编 ⇒ 门结论自证链断裂。
缺省回读路径 = 记录件的 `gate_summary_path`；也可用 `verify --gate-summary <路径>`
显式指定（内容仍须逐字节相符，否则 `GATE-SUMMARY-DIGEST`）。
**未回读 ⇒ 门摘要自证这条路径不成立**（复核器直接拒绝，不给"未回读也认"的分支）。
门摘要文件读不出 ⇒ `GATE-SUMMARY-ABSENT`（拒绝，不是跳过）。

---

## 二、什么时候必须走复核（判据 + 理由）

### 2.1 必须走（判据）

**判据：合流的落地内容进入 `product/main`。**
凡落地内容进入 `product/main`（唯一合流目标，见 `docs/CI-BOUNDARY.md` §五），
**必须**走完整复核（四元组 + 门结论自证 + 落地树核）。

理由：`product/main` 的内容由 **F2 全量门**（`ci/run.sh` 十四步）背书，而 F2
不走 Actions、走收件箱派单 + 独立 runner（`docs/CI-BOUNDARY.md` §三）；F1 快速门
**绿不是合流充分条件**（同节原文）。四元组是把「门结论」绑到「具体内容」的唯一机制
——没有它，"门绿过"与"落地内容"之间没有任何可核的链。

### 2.2 不走（明确的边界，不是豁免）

| 情形 | 为什么不走 |
|---|---|
| 不进 af 仓的落账（工作区 `30-工件/`、`计划/` 等工件面更新） | 复核对象不存在（不在 af 仓内 ⇒ 无 base/source/tree 可算） |
| 进入 `product/main` **之前**的中间动作：候选分支推远端、建集成分支、worktree 搬运 | 闸门未到（还没进主线） |
| 只读核验 / 审计跑（复核器 `actual` / `digest` 子命令；历史合流核验） | 只出读数，不构成合流动作 |
| 对照线 `main` | 已冻结只读（`docs/CI-BOUNDARY.md` §五），不存在合流动作 |
| 指针回退 / force push / 历史重写 | ★ **一律禁止**——不是"不走复核"，是**不许做** |

### 2.3 ★ 不许豁免的情形（看着"轻"也要走）

- **纯文档面合流**：`ci/run.sh` 的 step11（`doc-claims.txt` 自述-实现对账）与
  step12（`numbers.json` 计数复算）**都读仓内文档** ⇒ 文档改动能改门结论 ⇒ 不能豁免。
- **门执行面（`ci/`）或判据正本被改**：改门即改判据 ⇒ 必须走（且 §六 的"必须人工看"命中）。
- **候选树含冲突解法**：解法本身改变落地内容 ⇒ 必须走，且必须人工看（§六 b）。

> 「不许把所有合流都变成手工负担」的落点：**只有"进 `product/main`"这一类需要复核**
> （§2.1）；中间搬运与只读核验不占复核成本；轻量合流走 §六 的**自动判过路径**，
> 人工成本 ≈ 读一行输出。复核器是**一次脚本跑**（秒级），不是人工流程。

---

## 三、常设流程（谁 · 何时 · 做什么）

> 本仓现状：合流由主代理/合流执行员手工执行（建设台账:148-149）；`product/main`
> 允许白名单直推（`docs/CI-BOUNDARY.md` §五：**本仓不是强制 PR 合流模型**）。
> 下述流程与该现状对齐，不改既有分支保护与直推面。

| 步 | 谁 | 何时 | 动作 | 产物 |
|---|---|---|---|---|
| 0 | 合流执行人 | 合流批开工 | 从 `origin/product/main` 拉**集成分支** `integrate/<batch>`；各源支 `--no-ff` 合入并解冲突；**候选树定稿**（此后不再改内容） | 候选分支头 |
| 1 | 合流执行人 | 候选树定稿后 | **把候选分支推到远端** `git push origin integrate/<batch>`（普通推，禁 force）——`source_sha` 的语义前提是"远端头" | 远端候选分支 |
| 2 | Actions（自动） | 每次推送 | **F1 快速门**（`.gitea/workflows/f1.yml`，仅 base=`product/main` 触发）：绿 = 合流的**必要非充分**条件；红 ⇒ **立即暂缓**，不进复核 | Actions run 结论 |
| 3 | F2 独立 runner（或本地全量门） | 步 1 之后 | **F2 全量门** `bash ci/run.sh` 在**候选树的工作树**上跑（十四步；`ci/out/SUMMARY.txt` 是门摘要） | `ci/out/` 门产物 + `SUMMARY.txt` |
| 4 | 合流执行人 | **门绿之后、合流动作之前** | **写记录件**（§四）：`python3 -I ci/integrator_bind_check.py emit …`；★ **门摘要文件（`--gate-summary`）必须与记录件同目录留存**（步 5 要回读它，步 9 要留证） | 记录件 JSON + 门摘要文件 |
| 5 | **integrator（人工）** | 步 4 之后 | **复核**：`python3 -I ci/integrator_bind_check.py verify --record <件> --branch integrate/<batch> --remote origin --base-ref origin/product/main`；exit 0 才进下一步；非 0 ⇒ §七<br>★ 复核**强制回读**门摘要文件（磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文 三方比对，无开关可绕）⇒ **未回读 ⇒ 门摘要自证这条路径不成立**；<br>★ 复核**默认读远端分支头**；确需以本地提交复核时须显式 `--allow-local-source`（输出留 `source_origin`） | 复核输出（stdout/stderr） |
| 6 | integrator（人工） | 步 5 绿 | **判读**：走 §六 的自动判过路径，或按人工看点逐条看 | 合流决定 |
| 7 | 合流执行人 | 步 6 通过 | **合流动作**：`git checkout product/main && git merge --ff-only integrate/<batch>`（快进形态；真合并形态见 §五 注） | 合并提交 |
| 8 | 合流执行人 | 步 7 之后 | **合流后核**：`git rev-parse <合并提交>^{tree}` **必须 == 步 5 输出的 `merge_tree`**（逐位）；再 `git push origin product/main`（普通推，**禁 force**），核 `git ls-remote origin refs/heads/product/main` == 本地 HEAD | 推送读数 |
| 9 | 合流执行人 | 步 8 之后 | **留证**：记录件 + 复核输出 + 门产物留在 `ci/out/` 与工位工件面；**不许删记录**（§七） | 证据面 |
| 10 | 合流执行人 | 收尾 | **落账**：把记录件路径、四元组、复核结论如实登记进建设台账/欠账总表 | 台账条目 |

**串行度 = 1**（方案裁决）：同一时刻只有一笔合流在 §三 的流程里；主线在步 3 之前
被别的合流推进 ⇒ 触发 §五 的 rebase 条款。

---

## 四、记录侧怎么产生

### 4.1 格式（`af.integrator_bind_record.v1`）

| 字段 | 含义 | 谁写 |
|---|---|---|
| `schema_version` | `af.integrator_bind_record.v1` | 工具 |
| `base_sha` / `source_sha` / `tested_tree` / `suite_digest` | 四项绑定 | **工具自算**（禁手抄） |
| `verdict` | `pass`（严格） | 工具（由内嵌摘要判出） |
| `gate_id` / `gate_command` / `gate_exit` | 哪一门、怎么跑的、退出码 | 工具（`--gate-id`/`--gate-command`/`--gate-exit`） |
| `gate_summary_path` / `gate_summary_sha256` / `gate_summary_text` | 门摘要原文与其摘要（**内嵌** ⇒ 可离线比对）＋ **门摘要文件路径**（复核要回读它 ⇒ 该文件**必须留存**，不许删） | 工具 |
| `base_ref` / `gate_base_sha` | 门跑时的目标分支 ref 与其头（主线推进面核的基准） | 工具 |
| `suite_digest_algo` / `suite_members` | 摘要算法与集合成员数 | 工具 |
| `branch` / `remote` / `recorded_at` / `recorded_by` | 候选分支、远端、时间、记录人 | 工具/参数 |
| `source_origin` | 候选头来源：**远端该分支头** vs **本地提交**（事后可区分；复核侧同样留痕） | 工具 |

### 4.2 谁在什么时刻写

- **谁**：合流执行人（= 本仓现状的 integrator 角色）**在门跑完且门绿之后、合流动作之前**写。
- **怎么写**：**用工具写**（`emit` 子命令自算四项 + 内嵌门证据）。
  ★ **禁手抄四项**：手抄 = 把「机器辅助核对」退化成「人抄人比」。
- **时刻**：步 3 之后、步 5 之前。**门红 ⇒ 不许写记录件**（`emit` 会拒）。

### 4.3 放哪 / 命名 / 签不签入仓

- **放哪**：候选树工作树的 **`ci/out/integrator-record-<batch>-<branch-slug>.json`**
  （与门产物同面，便于同批归档）。
- **命名**：`integrator-record-<batch>-<branch-slug>.json`（batch = 合流批次名，
  branch-slug = 候选分支名把 `/` 换成 `-`）。
- **★ 不入候选树**：记录件**不许**在候选树上被 commit（入了就改树 ⇒
  `tested_tree` 失配 ⇒ 复核必红，自证失效）。
- **签不签入仓**：**默认不签入候选树**；可**在合流之后**作为工件单独提交签入
  （那是一个独立提交，不影响被复核的树）。
  ★ **不许**为历史合流补造记录件来"补齐"（§八）。

---

## 五、★ rebase 情形的专门条款（可核动作，不是散文）

**场景**：后到补丁在门跑完**之后**被 rebase（基点变了），或前一笔补丁先合导致
主线前移 ⇒ 门跑过的那棵树与要落地的那棵树不再等价。

**方案裁决原文**：后到补丁 rebase 后先合时 integrator 确认 `tested_tree == 合并树`，
**不等价重跑 F1**。

### 5.1 三个可核判据（复核器逐条机器判，人按读数执行）

| 判据 | 复核器怎么判 | 不符时的 reason_code |
|---|---|---|
| ① 基点漂移 | 记录侧 `gate_base_sha` vs 实测 `--base-ref` 的当前头 | `GATE-BASE-DRIFT` |
| ② **树等价**（本条款的核心） | 记录侧 `tested_tree` vs 实测 `source_sha^{tree}`；并 vs **落地树**（合并树 `merge_tree`：快进形态=候选树，真合并形态=ort 机算合并树） | `BIND-MISMATCH` / `COVERAGE-TESTED-TREE` |
| ③ 门没被换 | 记录侧 `suite_digest` vs 落地树门集摘要 | `BIND-MISMATCH` / `SUITE-DRIFT` |

### 5.2 动作（可照做的步骤）

1. **判**：跑复核器（步 5）。看 `reason_code`：
   - `GATE-BASE-DRIFT` 或 `COVERAGE-TESTED-TREE` ⇒ **命中本条款**；
   - `BIND-MISMATCH(tested_tree)` ⇒ 同上（内容变了）。
2. **重落基点**（把候选树落到**当前**主线）：
   - 快进形态：`git rebase origin/product/main`（或把主线合入候选分支后重解冲突）；
   - 真合并形态：把 `origin/product/main` 合入候选分支并解冲突，重做候选树定稿。
3. **重跑门**（★ 不许拿"四项里前三项对上了"搪塞过去）：
   - 按记录件 `gate_id` 重跑**那一门**：F2 全量门 = `bash ci/run.sh`（候选树工作树上）；
   - F1 快速门由 Actions 在**新推送**上自动重跑；F1 绿是合流的必要非充分条件
     （`docs/CI-BOUNDARY.md` §三）——**F1 绿不替代 F2 记录件**。
4. **重写记录件**：`emit` 写新记录（新文件名带 `-r2`/`-r3`…；**旧记录件不许删**）。
5. **再复核**：再跑一次 `verify`（用新记录件）。绿 ⇒ 进 §六 判读；红 ⇒ §七。
6. **绿的情形也要留痕**：若 rebase **未改变内容**（`tested_tree == 落地树`，
   复核器绿），则门结论仍描述这棵树 ⇒ 可继续，但**基点变了这件事**必须写进
   合流报告/台账（内容没变 ≠ 什么都没发生）。

### 5.3 注：真合并形态的额外约束

真合并（`merge_mode=merge-commit`）时，落地树 = `ort` 机算合并树。**若解冲突时
人工改动过内容**，机算合并树与实际合并提交树会不同 ⇒ 步 8 的"合流后核"
（`git rev-parse <合并提交>^{tree}` == 复核输出的 `merge_tree`）会红 ⇒ **必须**
回到候选树重做（解冲突定稿 → 重跑门 → 重写记录件 → 再复核），不许带着
"机算树 ≠ 实际树"硬推。

---

## 六、★ 什么可以直接判过 / 什么必须人工看

> 前提：**复核器 exit 0**（机器侧未发现不符）。判过与否仍由人作（§〇）。

### 6.1 可按「自动判过路径」处理（人工只需读一行）

以下**全部**满足时，人工只需确认「四项逐项相符 + 落地树 == 门跑过的树」这一行：

- 候选树改动**只**落在不改门结论的面：`docs/**`、根级 `*.md`、`ci/out/**`；
- 合并方式 = `fast-forward`（无冲突解法）；
- 门摘要**无** `ADVISORY-WARN` 行、**无** `PASS-WITH-WARN` 行（工具缺席类 advisory /
  step14 机读一致性批的 advisory WARN：门仍绿，但须人眼看）；
- 候选头来源 = **远端头**（复核输出 `source_is_remote_head = true`、未用 `--allow-local-source`）；
- 无基点漂移（`GATE-BASE-DRIFT` 未命中）。

### 6.2 必须人工看（任一命中 ⇒ 人眼逐条看）

| 命中条件 | 人眼要看什么 |
|---|---|
| 候选树 diff 触及 `crates/**`、`ci/**`（除 `ci/out/`）、`Cargo.toml`/`Cargo.lock`、`deny.toml`、`deliverables.yaml`、`numbers.json`、`doc-claims.txt`、`strategies/**`、`proofs/**`、`.gitea/**`、`poc/**` | 该批 diff + 门结论摘要（`ci/out/SUMMARY.txt`）逐项对应 |
| 合并方式 = `merge-commit`（真合并） | 冲突解法有无夹带；步 8 的合并树逐位核对 |
| 门摘要含 `ADVISORY-WARN` | 缺席的是哪个工具、缺席原因是否可接受（advisory 非阻塞，但须登记） |
| 门摘要含 `PASS-WITH-WARN` | step14 机读一致性批的 advisory WARN：`ci/out/step14.log` 里哪些子检查是 WARN（门仍绿，但不许走 §6.1 自动判过路径） |
| 复核输出 `source_is_remote_head = false`（用了 `--allow-local-source`） | 候选头取自**本地提交** ⇒ 人工确认该提交**已推送**到候选分支（否则远端跑的不是被测的那个提交） |
| 发生过 rebase / 基点漂移（即便复核绿） | rebase 留痕：基点变了但内容未变的证据（`tested_tree` 相等） |
| 门执行面或判据正本被改（`suite_digest` 变化） | 门面 diff —— 改门即改判据 |
| 复核输出里 `review_points` 非空 | 按输出逐条看（复核器已把该看的点列出来） |

---

## 七、★ 失败时怎么办

### 7.1 红 ⇒ 停在哪

复核红 ⇒ **不许合流**：不许 push、不许开/合 PR、不许"先合了再说"、
**不许跳过复核**、**不许改记录件凑绿**。

### 7.2 回哪一步（按 reason_code）

| reason_code | 回哪一步 |
|---|---|
| `RECORD-ABSENT` / `RECORD-UNPARSEABLE` / `RECORD-SCHEMA` | 步 4：补写记录件（用 `emit`，禁手抄） |
| `KEY-MISSING` / `KEY-EMPTY` / `KEY-MALFORMED` | 步 4：重写记录件 |
| `VERDICT-NOT-PASS` / `GATE-VERDICT` / `GATE-EXIT-NONZERO` | 步 3：门红或门不可跑 ⇒ 修内容**重跑门**，再步 4 |
| `GATE-SUMMARY-DIGEST` | 步 3/4：**门摘要文件与记录件内嵌的摘要对不上**（三方比对不一致）⇒ 确认门摘要文件没被换、记录件没被手改 ⇒ 重跑门 + 重写记录件（新文件名 `-r2`，旧件不许删） |
| `GATE-SUMMARY-ABSENT` | 步 3/4：门摘要文件**读不出**（或记录件缺 `gate_summary_path`）⇒ 门不可跑 ⇒ 修内容**重跑门**，再步 4（回读不成立 ⇒ 门摘要自证这条路径不成立，不许"未回读也认"） |
| `LOCAL-SOURCE-NOT-ALLOWED` | 步 5：去掉 `--source`（复核器默认读**远端该分支头**）；确需以本地提交复核 ⇒ 显式加 `--allow-local-source` 并在合流报告里登记 `source_origin` |
| `BIND-MISMATCH(source_sha)` | 步 1：推上去的不是测过的那个提交 ⇒ 推对提交，或重跑门 |
| `BIND-MISMATCH(tested_tree)` / `BIND-MISMATCH(suite_digest)` | 步 3/4：门跑完后树或门面变了 ⇒ 重跑门 |
| `GATE-BASE-DRIFT` / `COVERAGE-TESTED-TREE` | 步 0 + 5：**rebase 条款**（§五）——重落基点 + 重跑门 + 重写记录件 + 再复核 |
| `MERGE-TREE-CONFLICT` | 步 0：解冲突、候选树定稿 ⇒ 重跑门（步 3） |
| `REMOTE-UNREACHABLE` / `REMOTE-HEAD-UNRESOLVED` / `REF-UNRESOLVED` | 环境错 ⇒ **复核仍然红**（不许因"读不到"就跳过）⇒ 修网络/分支后重跑复核 |
| `SUITE-DRIFT` | 门面在门跑完后变了 ⇒ 重跑门（步 3） |

### 7.3 记录件怎么处理（★ 不许删记录）

- **不许删、不许覆盖**：被拒的记录件**照样留档**。`emit` 默认拒绝覆盖既有记录件
  （`--force` 只用于同路径重写，且必须在工位留痕说明为什么重写）。
- 重写用**新文件名**（`-r2`/`-r3`…），原件保留 ⇒ "哪次被拒、为什么被拒"可回溯。
- 复核输出（stdout/stderr）**同样留档**（与记录件同目录）。
- 门产物（`ci/out/`）**不许删**。

---

## 八、★ 与现有合流事实的一致性（历史合流无记录面，如实登记）

### 8.1 现状

- 本仓近期合流**由主代理手工执行**（建设台账:148-149）；`product/main` 是唯一合流
  目标，且允许白名单直推（`docs/CI-BOUNDARY.md` §五）——**本仓不是强制 PR 合流模型**。
- 本批之前，**产品合流的门结论与具体内容之间没有任何机器可核的链**：
  四项绑定只在 PoC 样例仓验证过（PoC-E `codeadapter.py:144`）。

### 8.2 核证结果：★ 历史合流**无等价记录面**

- 全史是否有过 integrator 相关文件：`git log --all --oneline --name-only -- '*integrator*'`
  ⇒ **空**（全史无）；
- 仓内现存 `gate_result` / `record` 类跟踪件：`git ls-files | grep -iE 'integrator|gate_result'` ⇒ **空**；
- ⇒ **历史合流没有留下任何等价记录**。复核器对历史合流**必然拒绝**
  （`RECORD-ABSENT`）——这是设计意图，不是缺陷。
- ★ **不许事后伪造记录件**去"补齐"历史（§四 4.3）。历史只能如实登记。

### 8.3 逐笔真实读数（复核器自算；完整原文见
`poc/poc-t7-2-integrator/raw/history-readings.txt`）

| 合流 | 形态 | 落地树 vs 候选树 | 读数结论 |
|---|---|---|---|
| `83bb71c`（2026-11-04，集成批快进：product/main 由 `0bb3b14` 快进到集成分支头） | 快进 | **相等** | 落地内容 == 候选树 ⇒ **覆盖形态成立**（若门跑在集成分支头上，覆盖即成立） |
| `ae6869d`（2026-11-04，`feature/poc-e-codeadapter` 直并入 product/main） | 真合并提交 | **不等** | 落地树 ≠ 分支头树 ⇒ 落地内容**整体没过门**（历史形态如此） |
| `df7c6e3`（2026-11-04，`feat/l3-l3-independent-port` 直并入） | 真合并提交 | **不等** | 同上 |
| `e448453`（2026-11-04，`fix/gate-inputs-repin` 直并入） | 真合并提交 | **不等** | 同上 |

- 四笔的**机算 ort 合并树 == 真实合并提交树**（逐位相同）⇒ 历史合流都是干净的
  ort 合并（无 evil merge 夹带）。
- ★ `suite_digest` 的**历史限制**：历史时点**没有**任何记录侧摘要值（全史无记录件），
  且本批的集合定义（`af.gate_suite_digest.v1`）当时并不存在 ⇒
  **历史值不可从 git 反推为"当时记下的值"**。读数里给出的门集摘要只是
  **按本批定义在历史树上反算的重建值**，**不冒充**历史记录值。

---

## 九、复核器用法速查

```bash
# 记录侧（门绿后写；四项本器自算，禁手抄）
python3 -I ci/integrator_bind_check.py emit \
  --record ci/out/integrator-record-<batch>-<branch-slug>.json \
  --branch integrate/<batch> --remote origin --base-ref origin/product/main \
  --gate-id f2-full --gate-command "bash ci/run.sh" --gate-exit 0 \
  --gate-summary ci/out/SUMMARY.txt --recorded-by <人>

# 复核（合流动作之前；exit 0 = 机器侧未发现不符，≠ 准合流）
#   ★ 复核**强制回读**门摘要文件（磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文 三方比对；
#     无开关可绕；回读路径缺省＝记录件的 gate_summary_path）
python3 -I ci/integrator_bind_check.py verify \
  --record ci/out/integrator-record-<batch>-<branch-slug>.json \
  --branch integrate/<batch> --remote origin --base-ref origin/product/main

# 同上，但显式指定要回读的门摘要路径（内容仍须逐字节相符）
python3 -I ci/integrator_bind_check.py verify \
  --record ci/out/integrator-record-<batch>-<branch-slug>.json \
  --branch integrate/<batch> --remote origin --base-ref origin/product/main \
  --gate-summary ci/out/SUMMARY.txt

# ★ 候选头取自**本地提交**（默认读远端头）：必须显式承认，否则 LOCAL-SOURCE-NOT-ALLOWED
python3 -I ci/integrator_bind_check.py verify \
  --record ci/out/integrator-record-<batch>-<branch-slug>.json \
  --branch integrate/<batch> --remote origin --base-ref origin/product/main \
  --source <本地提交> --allow-local-source

# 只看实测侧读数（审计/历史核验；不做判定）
python3 -I ci/integrator_bind_check.py actual --base-ref origin/product/main --branch integrate/<batch>

# 门套件摘要（审计）
python3 -I ci/integrator_bind_check.py digest --tree <rev> --list-paths
```

★ 本复核器**不进 `ci/run.sh`**（禁新增 step 编号）：它是**独立脚本 + 本 SOP**，
由 §三 步 5 的 integrator 在合流前手动跑一次。

---

## 十、边界（如实登记）

1. **机器不判"记录件是不是真门产出的"**：复核器核的是「记录侧 ↔ 实测侧」的一致性
   ＋**回读**门摘要文件的三方自证（磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文）
   ＋门退出码 + step 词表（§一.3）。
   "这份门摘要文件确实出自某次真门"仍属**流程纪律**（SOP §四：谁在何时写、门产物同目录留证、
   门产物不许删）——文件与记录件一致**不等于**文件是某次真跑的门产出的；
   机器仲裁员首版不启用 ⇒ 机器不越权裁决。
2. **门套件集合只覆盖 F2 门执行面**（`ci/` ＋ `.gitea/workflows/`）。门的**数据输入**
   （仓根判据正本 `deliverables.yaml`/`numbers.json`/`doc-claims.txt`/`deny.toml`
   ＋ `strategies/` ＋ `poc/` 资产）在"候选树那一侧"由 `tested_tree` 绑定；
   "主线侧前移"由 `GATE-BASE-DRIFT` + `COVERAGE-TESTED-TREE` 兜住（⇒ 触发重跑门）。
3. **真合并形态的冲突解法由人工判读**（§六 b）；复核器机算的 `merge_tree` 只保证
   "无冲突时的 ort 结果"，人工解冲突改了内容 ⇒ 步 8 的合并树核对会红。
4. **本复核器不含任何自动合流动作**（不 push、不开 PR、不改 ref、不动
   `product/main` 指针）——合流动作仍在 §三 步 7-8，由人执行。
5. **`suite_digest` 的集合会随门面新增而变**：新增门脚本 ⇒ 成员数变 ⇒ 摘要变 ⇒
   正在合流的候选树必须**重跑门**（这是设计意图，不是缺陷）。

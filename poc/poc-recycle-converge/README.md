# poc-recycle-converge：worktree-recycle 删除动作收敛的证据面

分支 `feature/worktree-recycle-converge`，基线 = origin/product/main（66be37c）。
**纯 Python 批，零 cargo。**

## 改了什么

只动两件 + 本证据目录：

| 文件 | 动作 |
|---|---|
| `tools/worktree-recycle.py` | 收敛删除动作（114+/36-） |
| `tools/worktree-recycle-selfcheck.py` | 新增自检（非 `test-` 前缀，循仓内 selfcheck 命名） |
| `poc/poc-recycle-converge/` | 本目录：对照驱动器 + 读数 + 本 README |

三件收敛（详见提交 diff）：

1. **删分支收敛**：默认**不删任何分支**；确需删必须 `recycle --delete-merged-branch`
   显式开关，且先经 `_merged_into_protected` 真合入判定，再走 `git branch -d`
   （安全删，未合入即拒）——**全程无 `-D`**（grep 可证：脚本中已无 `"-D"` 字样）。
   docstring 铁律与 recycle 报告均写明「本工具默认保分支保提交」。
2. **强删目录兜底去除**：`git worktree remove` **去掉 `--force`**，且失败时
   **不再 rmtree 兜底**，改报「留检保现场」——报告含 git 拒绝原因（stderr 首行）
   与脏面摘要（`git status --porcelain` 前 10 行）；登记不销、记
   `mismatch=true` + `hold_reason="worktree-remove-failed"`。三路（merged/aborted/
   timeout）的 CLEAN 调用点都接住了这个失败分支：**失败 ⇒ KEEP 留检，绝不销登记悬空现场**。
   去 `--force` 的依据：实测（git 2.47.3）`git worktree remove` 不带 `--force` 时
   对脏树/含未跟踪文件的树自行拒绝（"contains modified or untracked files,
   use --force to delete it"）——git 自身的拒绝就是留检语义的实现位；带上
   `--force` 则连未提交改动一起抹掉（改前实测，见 before-after.md E1）。这使
   「要清脏树 ⇒ 由人先处理脏面」落到 git 原生行为上，而非自造检查。
3. **测试**：`tools/worktree-recycle-selfcheck.py`，`python3 -I` 可跑，20 项全绿
   （exit=0）。四条要求 + 三路语义回归都覆盖（文件名避开 `test-` 前缀，与外层
   L2 的 `tools/test-*.py` glob 不混淆）。

## 为什么

- 本项目纪律：**保分支保提交**（合入批硬约束：禁删分支、禁删提交对象）。
  改前脚本默认 `branch -D` 强删——**未合入也删**（before-after.md E2 实证），
  已迫使同机另一批弃用本脚本。
- F1 批自己的收卷口径是「merged 且 merge-base 真合入才清、自报不符记 mismatch
  留检」；改前的 `--force` + rmtree 兜底恰好绕过留检语义：脏树未提交改动会被
  无声抹掉，甚至误登记的**非 git 目录**也会被 rmtree 整目录删掉（E4 实证）。
  工具不得替人做不可逆决定。

## 读数文件

- `before-after.md`：改前（`git show 66be37c:` 取回原样跑）vs 改后，同夹具同命令
  四场景（E1 脏树 / E2 未合入分支 / E3 merged 默认 / E4 误登记路径）。
- `run-selfcheck.txt`：`python3 -I tools/worktree-recycle-selfcheck.py` 全量输出，
  20 项全绿，exit=0。
- `behavior-diff.py`：对照驱动器（证据采集用，非测试件）。

## 做不到什么（如实边界）

- `git branch -d` 的合入判定锚是**当前 HEAD/upstream**，本工具的真合入判定锚是
  **保护分支**（merge-base）。两者不一致时（如 HEAD 游离在别处），-d 可能对已
  真合入的分支拒绝——此时如实报告「被拒 → 分支保留」，**宁可少删不误删**；
  本批不为此引入任何绕过（绕过即回到 -D）。
- 误登记路径（E4 类）下脏面摘要的 porcelain 视角是**包含该路径的仓**（git -C
  向上找仓根），非该目录自身；摘要仅供人看，不影响留检判定。
- 三路语义、保护分支（PROTECTED_BRANCHES）、TTL≠删树等既有口径**原样保留**
  （selfcheck R1-R4 回归项守着）；未登记的树一律不动（原样）。
- 既有件的其余行为（mark/heartbeat/list、登记薄格式）未动；`_remove_worktree`
  签名变了（多了 `delete_branch`、返回值多了 ok 位），仓内无其他调用方
  （grep 全仓仅 docstring 自引）。
- F1 门（.gitea/workflows/f1.yml）无任何 python/tools 步骤（grep 无匹配），
  本批无仓级 pytest/自跑入口可跑；门读数即 `python3 -I` selfcheck（exit=0）。

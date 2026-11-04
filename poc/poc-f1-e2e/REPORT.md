# PoC · F1 快速门端到端绿跑（gitea Actions 真跑绿）

**日期**：2026-11-04　**执行机**：anolis-gpu-01　**工作树**：`wt/f1-e2e`（按 label 建的独立树，未触碰 `~/af-dev/af` 共享树）

## 结论（一行）

在 af 仓开专供验证分支 → 推送 → API 建 PR（base=`product/main`）→ gitea Actions
**真出现 run id=151**，`event=pull_request`、`head_sha` 与本地 HEAD 逐字一致，
**conclusion=success（绿）**，三件硬门步全部 success，run 全程 ≈54s。

## 链路各环（全部 API 原文可查）

| 环节 | 事实 |
|---|---|
| 基点 | `origin/product/main` = `288b0d9ecd4b0ceaaf5b55faae5a2411ae48cef3`（开工 fetch 核得；PR base.sha 同值） |
| 分支 | `poc/f1-e2e-verify` 自上述基点新建；唯一改动 = `docs/CI-BOUNDARY.md` §三 加一段说明性文字（+4 行，纯文档）；提交 `8dc06d4386533e0c29bae154fc85018880dcd860`；普通 push（`* [new branch]`，无 force） |
| PR | **#2**（内部 id=106），`http://100.64.0.6:3300/opchadmin/af/pulls/2`，base=`product/main`，head=`poc/f1-e2e-verify`，state=open、merged=false（验证用 PR，保持 open 不合） |
| Run | **id=151**，run_number=9，run_attempt=1，`display_title=F1 快速门 @ poc/f1-e2e-verify`，`path=f1.yml@refs/pull/2/head`，`event=pull_request`，`head_sha=8dc06d4…`（与 PR head / 本地 HEAD 一致） |
| 结论 | `status=completed`，**`conclusion=success`** |
| 耗时 | run created_at=`05:28:48Z` → updated_at=`05:29:42Z` ≈ **54s**（job started_at=`05:28:53Z`；job finished_at 字段 API 返回 null，以 run updated_at 计）。远快于 run148 的 5m16s：本分支代码面与主线零差异（纯文档 +4 行），持久 target 热缓存下三件步近乎只做校验 |

## 三件步结论（源：run151-jobs.json，job id=151 `f1-fast-gate`，runner label=`opch-ci`）

| # | step | conclusion |
|---|---|---|
| 0 | 分工声明（F1 快速门在此跑；F2 全量门不走 Actions） | success |
| 1 | checkout（PR 头树；零外部 action 依赖） | success |
| 2 | 工具链供给（runner 容器自举/复用钉住版） | success |
| 3 | **fmt --check（硬门）** | **success** |
| 4 | **clippy -D warnings（硬门）** | **success** |
| 5 | **快速测试（af-dsl + af-compiler）** | **success** |

## 它证明了什么 / 没证明什么

**证明了**：
1. `pull_request(base=product/main)` → `f1.yml` 触发链在 gitea 28 + 实例级
   opch-act-runner（label `opch-ci`）上**真实接通**：开 PR 即出 run，不再复现
   run143 式"标签不匹配 ⇒ 恒 queued"。
2. runner 环境能完整跑完 F1 三件套并**如实回传步级结论**（六步全 success，
   且是 Actions 上的真跑，非本地代跑）。
3. 私有仓 checkout（GITEA_TOKEN 注入）与工具链供给（复用持久 cargo）在
   稳态下工作正常，run 级耗时分钟内。
4. fmt 归零 + clippy 零 error 主线状态下，一条仅含文档变更的 PR 能拿到
   全绿 F1 结论——"主线绿 ⇒ PR 快速门绿"这条推断在此类变更上成立。

**没证明**：
1. **没证明代码变更下的跑绿**——本分支零代码差异；fmt/clippy/test 对本 PR
   实际检验的就是主线代码面。
2. 没证明 F2 全量门（设计上不走 Actions）；F1 绿也不是合流充分条件。
3. 没证明冷启动（target 空缓存首跑 20-30min）与双 runner 并发争用下的行为。
4. 没证明 Actions 产物（logs/artifacts API 面）之外的任何运营面。

## 红线核验

- `product/main` 未动：开工 `288b0d9…`，收尾 fetch 复核仍 `288b0d9…`（见下 REPORT）。
- 无 force push：分支首推输出 `* [new branch]`；后续证据提交普通 push。
- 未合流：PR #2 state=open、merged=false。
- 凭据来自 `~/.git-credentials`（urllib.parse 解析、basic auth、运行时取用），
  **未落盘**：`scripts` 与全部证据文件经反向扫描不含凭据串。
- 未改 workflow/门/代码：本分支相对 product/main 的 diff 仅
  `docs/CI-BOUNDARY.md` +4 行（首提交）与本 `poc/poc-f1-e2e/` 证据目录。

## 证据文件

- `pr2.json`：PR #2 API 原文（state/base/head/merged）。
- `run151-final.json`：run 151 终态 API 原文（status/conclusion/head_sha/时间戳）。
- `run151-jobs.json`：job + 全部 6 步结论 API 原文。
- `run-post-evidence.json`：证据提交（synchronize 事件）触发的追加 run 记录（若有）。
- `gitea_api.py` / `poll_run.py`：取证脚本（凭据运行时取自 `~/.git-credentials`，脚本本身无任何秘密材料）。

## 复核路径

1. `curl -u <opchadmin> http://100.64.0.6:3300/api/v1/repos/opchadmin/af/actions/runs/151`
   核 `head_sha=8dc06d4…`、`conclusion=success`。
2. 同端点 `/runs/151/jobs` 核三件步。
3. `git fetch origin && git rev-parse origin/product/main` 核仍为 `288b0d9…`。

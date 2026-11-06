# CI 边界声明——干净克隆下的 af/ci 外部依赖、跳过档与 F1/F2 分工

> 2026-11-04 · F1 快速门接线批落点（v0.3 T7 终裁④：af/ci 边界声明须写明外部依赖
> 或提供跳过档）。本文是**声明面**：只登记事实与口径，不改 `ci/run.sh` 任何断言
> 强度；run.sh 十四步（核心硬门为 1-7 步）语义以其自身文件为准。
> 事实核证方式：每行标注证据（`git ls-files` 亲证 / run.sh 行号 / 实测读数），
> 核证载体为干净 worktree（83bb71c，product/main 干净主线）。
> 主线前移注记（2026-11-04 F1 修补批）：product/main 已随 router-adjudication-leg
> 合流前移至 6ad146f（指针移动非本批所为，本地/远端双核一致）；本文「83bb71c」
> 一律指本批建支时点基线，§二 实测结论属 83bb71c 时点，不因主线前移失效。

## 一、核心七步的外部依赖与跳过档

| 步 | 门命令 | 外部依赖 | 干净克隆在位性（亲证） | 跳过档口径 |
|---|---|---|---|---|
| 1 | `cargo fmt --check --all` + `ci/k5-path-lint.py`（run.sh:194-195） | rustup rustfmt 组件（工具链钉 `rust-toolchain.toml` 1.99.1）；python3 标准库 | ✓（rustup 侧装；判分链清单 `ci/scorechain-manifest.txt` 入库） | **无跳过档**。工具缺席=红，不许 skip 入绿。 |
| 2 | `cargo clippy --workspace --all-targets -j4` + `ci/clippy_gate.py`（run.sh:199-200） | rustup clippy 组件；python3；基线 `ci/clippy-baseline.txt`（入库） | ✓ | **无跳过档**。 |
| 3 | `cargo test -j4 --workspace`（run.sh:223-224，含 provenance 门） | 纯 cargo；全量测试面含带 docker 执行面的 crate（af-bench 执行面跑容器，其单元测试不要求 docker 守护进程——本批实测见 §二） | ✓ | **无跳过档**；但全量测试耗时长，**属 F2**，不进 F1 快速门。 |
| 4 | cue vet/export/结构断言（run.sh:233-263） | `$AF_ROOT/poc/poc-cue/bin/cue`（run.sh:235） | **✓ 入库**（`git ls-files poc/poc-cue/bin/` → `poc/poc-cue/bin/cue`）；recipe 四件入库 | **无跳过档**。二进制随库，无需宿主供给。 |
| 5 | `ci/parse_scrub.py` × 7 真实日志（run.sh:265-288） | python3；输入 `poc/poc-cas/verify-logs/*.log`（run.sh:268） | **✓ 入库**（18 份日志 `git ls-files` 亲证在册） | **无跳过档**。 |
| 6 | cosign 断网 `verify-blob --trusted-root`（POC5 2026-11-05：revoked 链 fail-closed；在役 `tr-key.pub` 缺席；archived 公钥只作验证器活性；篡改例必拒；可读原因 `key revoked`） | ① `$AF_ROOT/bin/cosign`——**不入库**：`.gitignore` 行 2 `/bin`（亲证 `git ls-files bin/` 为空）；② 断网手段 `sudo unshare -n` 或回退 `unshare -Urn`（run.sh 内建）；③ 资产 `poc/poc-cas/verify-tr/` + `poc/poc-cas/revoked/tr-key.pub` **✓ 入库** | **✗ ②③在位，①缺席——干净克隆七步跑不齐的唯一步** | **跳过档（SKIP，environment_unavailable）**：宿主无 `bin/cosign` 时允许以 SKIP 入账并在 SUMMARY 标注缺因，等价验证=F2 独立 runner（已供给 cosign 的宿主）补跑 step6。**SKIP 不得计绿**；现状 run.sh 预检对缺件 fail-closed（run.sh:117-121 收 PREFLIGHT），此跳过档是**对账口径**而非预检改写。带外供给方案：把核过哈希的 cosign 二进制放宿主 clone 的 `$AF_ROOT/bin/cosign`（bin/ 不入库，供给属宿主操作，不触碰仓库内容）。判据未弱化：6b 篡改例仍必须被拒；在役公钥若被放回=红。 |
| 7 | `cargo deny --offline check`（run.sh:322） | ① `cargo-deny` 子命令二进制（PATH，仓外 `cargo install` 供给）；② advisory-db 本地库位 `~/.cargo/advisory-dbs/advisory-db-3157b0e258782691`（宿主状态，非仓内；刷新走 `ci/refresh-advisory-db.sh`，代理 `socks5h://<内网地址>:1080`，GitHub 不可达时沿用旧库 WARN 降级——deny.toml:5-6 与刷新脚本内建） | ✗ 两者皆宿主供给 | **跳过档（SKIP，environment_unavailable）**：cargo-deny 或本地 db 缺席时允许 SKIP 入账并标注缺因，等价验证=F2 独立 runner 补跑。SKIP 不得计绿。 |

step8-14 概览：python3/bash 自足面（neutrality/xcheck/branches/criteria/strategies/
对账三件套等 checker 全部入库），step9 的 coqc 与 Kani 是**声明可选**（缺席=WARN
非阻塞，run.sh:408 内建跳过档）；无 cosign 类仓外硬依赖。步 9 的 Kani 是 tmpfs
保留件（`/dev/shm/fv0/kani-0.68.0`，重启即失，run.sh:347-350）——本就是
run.sh 内建的缺席跳过档先例。

## 二、干净克隆可跑性结论（83bb71c 实测，2026-11-04）

- step1 fmt：**归零**（2026-11-04 fmt-zero-v2 批口径更新；时点=feature/mainline-fmt-zero-v2
  分支，基线 68be206 + 纯格式化提交 6c001f0，分支未动 product/main 指针）：
  `cargo fmt --check --all` exit=0（rustfmt 1.10.0-stable，钉住 1.99.1 生效）。改前
  同机实测 exit=1、99 diff 块/23 文件——与 83bb71c 时点预存红同源（漂移集中在
  af-order/af-workorder/af-execline/af-engine 等文件，`ci/REPORT.md` ①「预存未格式化
  代码」的 235 块/39 文件属更早树读数），非任一批新引入。**该读数时点=上述本批基线，
  不以本行直接充当主线读数**：主线 tip 会继续前进且前移段可能含新格式漂移，合流后
  fmt 状态须以合流树重验（rustfmt 幂等，重跑代价低）。
- step2 clippy：**绿**——`CARGO_BUILD_JOBS=4 cargo clippy --workspace
  --all-targets -- -D warnings` exit=0（45.2s 冷建 @ -j4）；基线已收紧至零
  （`ci/clippy-baseline.txt` 仅两行注释），`-D warnings` 与基线门均无既有警告可拦。
- F1 快速子集：**绿**——`cargo test -p af-dsl -p af-compiler` exit=0，
  375 个测试 0 败，单测试二进制 ≤0.25s（热缓存整机 <20s；冷建另计依赖编译）。
- 七步可跑性判定：**1/2/3/4/5 干净克隆可跑；6 缺 bin/cosign 必红（按 §一跳过档
  以 SKIP 入账或带外供给）；7 需宿主供给 cargo-deny+advisory-db**。

## 三、F1（Actions 快速门）与 F2（收件箱派单全量门）分工

v0.3 T7 终裁①：**CI 执行器 Gitea Actions 先行，Actions 只跑 F1 快速门；F2 全量
走收件箱派单+独立 runner，不许把全量门塞进 Actions。**

| | F1 快速门 | F2 全量门 |
|---|---|---|
| 载体 | `.gitea/workflows/f1.yml`（Gitea Actions，runner 标签 `opch-ci`） | `ci/run.sh` 十四步管道 |
| 触发 | pull_request → base=product/main | 收件箱派单 + 独立 runner 领单 |
| 内容 | fmt --check / clippy -D warnings / `cargo test -p af-dsl -p af-compiler` 快速子集 | 全量 test 面及 step4-14 全部硬门（含 step6 cosign、step7 deny） |
| 预算 | 稳态分钟级（runner 实测 run148=5m16s 全门；`timeout-minutes: 45` 止损，T7③——首次填充持久 target 约 20-30min，之后复用） | 小时级预算，独占 runner |
| 结论入账 | Actions run 结论（成功/失败，如实） | integrator 复核制：base_sha+source_sha+tested_tree+suite_digest 四元组（T7②） |
| 边界 | runner 上**无** cosign/netns/POC 资产供给义务——F1 不依赖也不执行 F2 门件 | 全量门不进 Actions |

两层的衔接：F1 绿不是合流充分条件——合流以 F2 全量门 + integrator 复核制
四元组为准；F1 红则是合流**充分**的暂缓理由（快速反馈拦在 PR）。

（2026-11-04）F1 端到端链路（开 PR → Actions 真跑 → 结论入账）另由专供验证分支
`poc/f1-e2e-verify` 的 open PR 实跑核验；该分支相对 product/main 仅含本段说明性
文字，不含任何代码/门/workflow 变更，验证完保持 PR open 不合流。

## 四、worktree 三路回收口径（T7④）

可执行脚本：`tools/worktree-recycle.py`（af 仓 tools/，非外层工作区 tools/）。
规则：**merged → 清**（worktree 目录与分支登记后移除）；**aborted → 留证 48h**
（保留现场与留痕文件，48h 后可清）；**超时 → 止损**（lease 过期即强制回收）。
**lease TTL 到期 ≠ 删树**：TTL 只重置认领，删树只走上述三路。详见脚本头注。

## 五、合流目标与 PR 基线口径（L1' 治理登记，2026-11-04）

产品仓 af 的**唯一合流目标是 `product/main`**。远端 `main` 是 **fix3 代际对照线**：
非默认分支、非合流目标、勿直接回合（merge/rebase/cherry-pick 回 `main` 一律禁止）。
本节只新增口径与事实，不改 §一—§四 任何既有语义。据此：

- **开 PR 的 base 必须显式写 `product/main`**。Gitea 的「默认 base」跟随仓设置
  default_branch——**该值已于 2026-11-04 由 `main` 改为 `product/main`**（L2 裁决执行；
  只读 GET `/api/v1/repos/opchadmin/af` 改前/改后双读取证，凭据指纹化、不落盘）。
  ⇒ **「默认 base 落在对照线 `main`」这一失门面已关闭**：不写 base 的 PR 现在默认
  对准 `product/main`，因而**默认就能拿到 F1 快速门**。
  ★ **但显式写 base 仍为硬要求**，理由有三：① 仓设置可被再次改动（保护规则的
  生效与撤销都是 API 一次调用），显式 base 不依赖仓设置；② **F1 快速门只认
  base=`product/main`**——base 落到 `main` 的 PR **拿不到任何门**（不红不绿就是不跑），
  这才是"显式写 base"的**首要**理由；③ `main` 虽已加保护，但**该保护只覆盖直推面**
  （见下条），PR 路径不在其拦截范围内 ⇒ 不能指望服务端替你拦住误开的 PR。
- **F1 快速门只在 base=product/main 时触发**（`.gitea/workflows/f1.yml` 实况：
  `on.pull_request.branches: [product/main]`）。base 落到 `main` 的 PR **拿不到
  F1 快速反馈**——静默失门，而非降级运行。
- **分支保护现状（L2'/L2'' 裁决执行，2026-11-04，经 pre-receive 实测）**：
  - `product/main`：`enable_push=true` + **白名单直推 `opchadmin`**（**保住现役
    「本地合流 → 直推」合流流程**）+ **禁 force push** + **禁分支删除**。
    实测四读数：白名单成员快进推**成功**；**有写权限但非白名单**的账号推**被拒**
    （`Not allowed to push to protected branch`）；**真非快进 force 推被拒**
    （`protected from force push`）；**删除被拒**（`protected from deletion`）。
    ⇒ F1/F2 门不再可被「直推」绕过（直推仅限白名单账号），且历史不可被 force 改写。
  - `main`：**直推面只读归档线**（`enable_push=false`、白名单为空、禁 force、禁删）
    ⇒ **连 owner 也推不上去**（实测 pre-receive 拒绝），`main` 上的 2 个独有提交
    与 `f373113` 指针**在直推／force／删除三面上就此冻结**，符合「对照线只读」的定位。
    ⚠ **口径限定（不可扩大解读）**：该保护**只覆盖直推面**。**PR 合流面不在其拦截范围内**
    ——上游 v28.0.0 源码实测：开 PR 路径无任何保护分支校验（`routers/api/v1/repo/pull.go`
    与 `services/pull/pull.go` 的创建路径均无 protect 检查）；合流路径 `doMergeAndPush`
    走真 git push 进 pre-receive，而 `IsUserMergeWhitelisted` 在 `enable_merge_whitelist=false`
    时**回落「写权限」**（`models/git/protected_branch.go:189-193`），仓主/admin 另经
    `CanBypassBranchProtection`（`:213-216`，本仓 `block_admin_merge_override=false`）
    **无条件放行**、且该放行在 `CheckPullBranchProtections` 之前返回
    ⇒ **有写权限者以 PR 方式合流 `main` 仍可能成功**。
    ⇒ **「main 只读」只对直推面成立**，**不等于「任何路径都写不进 main」**；
    防止误回合的**实际防线是流程纪律 + F1 门不覆盖 main 这一事实**（不是服务端拦截）。
  - **回滚方式**：两条规则各一条 API 删除即可复原（不影响任何 ref）。
  - ⚠ **与 §五 上一条的关系**：`product/main` 仍**允许**白名单直推 ⇒ 本仓**不是**
    「强制 PR 合流」模型；F1 快速门是 **PR 路径**的门，**直推路径不经 F1**
    （直推的正确性由合流前的本地全量门 F2 + integrator 复核承担）。
    本节登记的是**直推面的收窄**（仅白名单 + 禁 force + 禁删），**不是** PR 强制化。
- 关联缺陷（L1，同批修复）：`ci/fix_scan.py --diff-base` 缺省曾误取 `origin/main`
  （对照线），致 step13a diff 档路径过滤失效——本仓两线无 merge-base，基点恒
  不可解析 → 恒回落 full；若基点可解析亦属按对照线全树差集恒全族触发。缺省已改
  `HEAD~1`（与 mutations 门 `MUT_BASE` 同款，run.sh:167 先例），「基点不可解析
  时 fail-safe 回落 full」语义逐字保持。

## 六、GitHub 端 PR 反馈面（G1 批新增面，2026-11-04）

### 6.1 面与判据

- **面**：`.github/workflows/f1.yml` = GitHub 端 F1 快速门，与 Gitea 端
  `.gitea/workflows/f1.yml` **语义面 1:1 对等**（触发仅 `pull_request` + base 分支
  约束 / `cargo fmt --check --all` / `cargo clippy --workspace --all-targets -- -D warnings` /
  `cargo test -p af-dsl -p af-compiler` / `timeout-minutes: 45` / `concurrency`
  同分支只保最新 / clippy 与快速测试 step 带 `always()` 一次收全三路诊断）。
  环境面按 GitHub 侧现实改写（托管 runner、零 action 的 `git clone` checkout、
  无持久 target），逐条「改了什么 / 为什么 / 语义有无损失」登记在
  `poc/poc-g1-github-pr-surface/README.md` §二。
- **GitHub 侧 base 分支名 = `main`**（**实测非假设**）：
  `git ls-remote --symref github HEAD`（remote `github` = `https://github.com/Feasy898/af.git`）
  → `ref: refs/heads/main	HEAD`（**exit 0**）；
  同命令 `--heads` 实测 **24** 个分支中**无** `product/main` ⇒ Gitea 端
  `base=product/main` 在 GitHub 侧按实际写 `main`（映射语义 = 产品主线的 PR base 约束）。
  原始读数：`poc/poc-g1-github-pr-surface/readings/00-github-branch-probe.txt`
  §B「复跑（成功）」条（§A 的首次直连 URL `exit=124` 记录**保留不删**）。
- **判据**：`ci/gh_workflow_check.py`（G1 批新增件，R01–R22；R18–R22 为返修批加严）
  把上述约束逐条变成可执行判定（`pull_request_target` 缺席 / permissions 只读 /
  凭据面 / 零 action / 三个语义 step 齐备**且在命令位**（★ 返修批：R08/R09/R10 改
  命令位判定 + 拒吞红写法 `|| true` / `; true` / `set +e`）/ timeout / concurrency /
  F2 不进 Actions / always() 诊断面 / 工具链供给 / 解析 fail-closed + 双解析交叉核对
  + **重复键 fail-closed** / **语义 step 禁 `if:` 抑制** / **job 级 `if:` 禁恒假** /
  **触发面禁 `types`/`paths`/`paths-ignore` 收窄** / **判据面自检 step 在位且恒跑**）。
  **负向自证**：`python3 -I ci/gh_workflow_check.py --selftest` 造 **23** 个「被削弱的变体」
  逐个跑检查器 ⇒ 每个都必须红且命中预期规则（正例必须绿）。

### 6.2 门位（★ 本批不新增 `ci/run.sh` step 编号；`ci/` 下既有件零 diff）

| 层 | 触发面 | 性质 | 当前是否生效 |
|---|---|---|---|
| ① | GitHub 端 PR 门：`.github/workflows/f1.yml` 的「判据面自检」step（`if: always()`）恒跑 `python3 -I ci/gh_workflow_check.py` | 机器 | **否**——GitHub 通道当前 blocked（见 6.4） |
| ② | 门族面字节对账：`ci/gh_workflow_check.py` 落 `ci/` 门码面 ⇒ 既有跨树收口门 `ci/fix_verify.py` 机制③「门族全集单源」自动把它纳入逐树双核对账（`ci/fix_verify.py:199-231` 面派生 + `:268-300` 提交面 oid / 工作树面 sha256 双核）⇒ **检查器本体被删/被改弱 ⇒ 该既有门红** | 机器 | **是**（现役可跑）——但**有生效前提，见下** |
| ③ | SOP：①任何改动 `.github/**` 的 PR，合流前须 `python3 -I ci/gh_workflow_check.py`（exit 0）+ `--selftest`（exit 0）；②主线合流批可把它挂进 `ci/run.sh` 的**既有**槽位（**本批不挂**：禁新增 step 编号 + `ci/` 下既有件零 diff 硬约束） | 流程（人工触发） | 是（须被触发） |

> ★ **层 ② 的生效前提（返修批补，诚实登记）**：层 ② 的对账面 = **主干基线 ref 上的
> `ci/` 门码面**。本检查器本体是**新件、尚未并入任何基线 ref** ⇒
> 实测 `git ls-tree phase2 ci/ | grep -c gh_workflow` = **0**（另实测基线
> `origin/product/main` 同样为 0）⇒ **在合并进主干基线 ref 之前，基线门码面里根本没有
> 本件，机制③ 的逐树双核对账覆盖不到它 ⇒ 本体尚未受字节对账保护**。
> 与 `poc/poc-g1-github-pr-surface/README.md` §4.3 的「duty 树门族面补达」缺口对齐。
> ⇒ 层 ② 的现役可跑性**不覆盖本检查器本体**（覆盖的是既有 `ci/` 件）；本体入基线后
> 才被纳入。层 ①（GitHub 通道）当前 blocked ⇒ 三层里**当前没有任何一层机械地
> 保护本件**（层 ③ 是 SOP 面、须人工触发）。

> ★ 如实登记：层 ① 是「判据面被削弱 ⇒ 同一次 PR run 机器红」的机器面，
> 但**当前 GitHub 通道 blocked ⇒ 未生效**；层 ② 是现役机器面，保护的是
> **检查器本体**（★ 前提：本体入基线 ref，见上）；层 ③ 是 SOP 面。
> **GitHub 端 workflow 自身在通道恢复前无运行时权力**
> ⇒ 此期间「削弱 workflow」不产生运行时后果，但会在层 ① 生效后立刻被机器抓住，
> 且本批以 **23** 个削弱变体的负向读数预先证明「抓住」这一动作是机械的、非人工自觉。

### 6.3 本批不做什么

不动：GitHub 发布通道、镜像披露（`tools/mirror-disclose.sh` 与任何 allowlist/denylist）、
GitHub 公开仓已有 5 件 POC 私钥的处置（另一笔、需运营者决策）、Gitea 端 F1、
AF 产品的 F2 全量门路径、判分链、账本、协议、注册表、`crates/` 下任何件、
`ci/` 下既有件、`ci/run.sh` 的任何 step（含编号）。

### 6.4 GitHub 侧生效条件（当前**未生效**）

1. **披露通道恢复**：镜像终扫 `verdict=ok`（当前 `verdict=blocked`、无可发布物）⇒
   `30-工件/镜像披露-20261104/终审-allowlist与放行.md` 记的当前状态未变前，
   GitHub 侧拿不到本 workflow。
2. **镜像 allowlist 增补 `.github`**：`tools/mirror-allowlist.paths` 当前**无**
   `.github` 条目（实测 grep 零命中）⇒ 即使 ① 恢复，发布快照里的 `.github/` 会被
   **裁出** ⇒ 本 workflow 仍到不了 GitHub。**本批禁改该清单**（只登记，不动）。
3. **GitHub 仓侧 Actions 可用 + 仓内存在 `ci/gh_workflow_check.py`**：
   `ci` 在 allowlist 内（会被发布）⇒ 随快照到达；GitHub 仓的 Actions 权限需另行开通。
4. **恢复后须重验**：① `python3 -I ci/gh_workflow_check.py` 对**发布后的**仓面 exit 0；
   ② `--selftest` exit 0；③ 首个真实 PR 上 workflow 三个语义 step 实际跑起来
   （fmt/clippy/快速测试各绿）；④ 通道恢复与 allowlist 增补两件事都完成后，
   GitHub 侧分支保护把本 workflow 设为 required check（否则 PR 面无强制力）。

> ⚠ 已知边界（诚实登记）：GitHub 的 `pull_request` 事件用 **PR 头分支的 workflow 文件**
> ⇒ 判据面自检 step 跑的是 PR 头里的那份检查器；PR 若同时削弱 workflow 与检查器，
> 自检会一起失效（自证不可自证）。缓解：① 仓的 base 分支保护 + required check
> （base 侧文件不被 PR 头改写时判定生效）；② 本批把检查器本体交由层 ② 的既有门
> 做字节对账（跨树分叉即红）——★ **该缓解自身有生效前提：本体入基线 ref 之前不成立**
> （见 6.2 层 ② 的前提登记）；③ 合流前的 Gitea 端 F1 与产品 CI 走的是**主线**文件，
> 判据面改动必须经主线合流评审。**本批不解决「PR 头可同时改 workflow 与检查器」**
> （超出本批范围，另记）。

### 6.5 残余风险项（单列登记 · 返修批补）

**RR-01 ★ PR 头的 Rust 构建脚本会在 GitHub 托管 runner 上执行**（本批最需要登记的一条）

- **事实**：`pull_request` 事件下，本 workflow 检出并运行的是 **PR 头树**
  （`git clone --depth 1 --branch "$REF" "$URL" .`）。`cargo clippy` / `cargo test`
  会**编译 PR 头里的 crate**，而 Rust 的 `build.rs`（以及 proc-macro crate、
  `#[cfg(test)]` 辅助代码、构建脚本可触达的任意逻辑）**都是 PR 头作者可控的任意代码**
  ⇒ 它们**会在 GitHub 托管 runner（GitHub 侧机器）上原样执行**。
- **为什么必须单列**：这是本 workflow「检出 PR 头树」这一设计**固有的**执行面，
  与「谁能把代码送上 runner」无关——**只要本 workflow 对 PR 开放执行，就必然执行 PR 头的构建脚本**。
- **当前风险面（可量化的部分）**：本 workflow 的凭据面为**零 secret 引用**
  （不引 `secrets.GITHUB_TOKEN`、不引任何 `secrets.*`）、`permissions` 顶层与 job 级
  **双写 `contents: read`**、clone 走**匿名 HTTP**（零凭据落盘、零凭据进 argv）、
  **零 action 依赖** ⇒ 被执行代码拿不到仓库写权限、拿不到 secret。
- **残余风险（不可消除的部分）**：托管 runner 上的**任意代码执行**本身
  （CPU/内存占用、runner 内网可达面、GitHub 侧的滥用检测与账号级风控）——
  这部分**不因本 workflow 的凭据面为零而归零**；另 `cargo` 会从 crates.io 拉依赖
  （供应链面在 PR 头作者手里）。
- **处置**：**登记**，不声称已消除。可选的降险手段（**本批一律不做**、超出本批范围）：
  ①把 F1 面下沉到自建 runner；②对 PR 头加「首次贡献者需批准」；
  ③在 `pull_request` 之外另设一条不执行 PR 代码的只读面。
  另注：Gitea 端 F1 是**自托管 runner**（`runs-on: [opch-ci]`），同一执行面在那边
  落在**本厂自管容器**内 ⇒ GitHub 端把它挪到托管 runner 是**环境面改写带来的新执行面**，
  与 §6.1 的「环境面改写登记」同列。

## 七、工作区 all-gates L3 mes-ttl 审计消费腿（RT-E58，2026-11-05）

本节只新增口径，不改 §一—§六 任何既有语义。本仓 `ci/run.sh` **不**新增 step
编号；消费腿挂在工作区级在役发布面 `tools/all-gates.sh` 的 **L3 族**（与
`otel-reconcile-probe --check` / `obs-patrol-probe --check` 同构）。

- **债**：逃逸台账 RT-E58 / 阶段欠账台账:39。拦截①②③已落 `tools/mes-ttl-sweep.py`
  （①钉死清单数量下界红牌 `audit-shrink`；②审计失败/缩水 rc 上抛，现口径 4/5；
  ③水位行恒带「审计表数/承诺表数」）。机器消费者腿此前未施工
  （`grep -c ttl tools/all-gates.sh`=0）。
- **机读核**：`tools/mes-ttl-audit-consumer.py`（一次性只读进程，**零新增守护**）。
  `--check` 读近 N 行 `状态观测/mes-ttl-decision-point.jsonl` 与近 N 条
  `kind=otel-readonly-audit` 证据行：红牌 / 审计 rc 非零 / 数量下界破 ⇒ 门红；
  缺件/坏 JSON ⇒ rc=2 不可证不猜绿。主锚 5221 不可达（sweep 总 rc=1）**不是**
  otel 审计红牌，不把 OBS-D7 死锚误判成本门红。
- **门项**（工作区 `tools/all-gates.sh` `register_L3`，`add_item L3` 形态）：
  `--selftest`（夹具：往决策点注一条红牌必红、干净必绿）+ `--check`（实扫消费面）。
- **退出码对齐** `all-gates.sh` `run_item`（:360-365）：rc=0 ⇒ PASS；rc=2 ⇒ FAIL；
  其余非零 ⇒ FAIL。红文案含字面量 `FAIL`。
- **钉桩**：改 `tools/all-gates.sh` 后跑 `bash tools/gate-pins-refresh.sh` 重锚
  （登记串 `tools/gate-pins.list sha256=<64hex>；tools/all-gates.sh sha256=<64hex>`）；
  探针本体入 `tools/gate-pins.list` sha256 行。不改
  `tools/deploy-consistency-probe.sh` 的 `PIN_MD5`。
- **判据只增不减**：本门只新增消费面，不弱化 sweep 拦截①②③、不改
  `tools/mes-ttl-sweep.py`、不改 `tools/otel/otlp-assert.py`。

## 八、工厂 all-gates 门日志 per-run 隔离（RT-E73，2026-11-05）

> 本仓 `af/ci` **不拥有**工厂 `tools/all-gates.sh`（该件落 pandev-comp
> `$HOME/workspace/agent黑灯工厂/tools/`，工作区根与 `tools/` 均不在任何 git 仓内）。
> 本节只登记**门日志路径契约与消费者影响面**，供 af 侧读日志的路径不要按旧形态硬编码。
> 本批 af 树写入面＝本节；工厂落点与读数见
> `30-工件/rt-e73-perrun隔离-20261105/接线报告.md`（pandev-comp，不入库）。

### 7.1 债面（台账 :41 / 逃逸台账 RT-E73）

逃逸台账 RT-E73 拦截三件：①夹具路径 per-pid；②双实例同跑互不干扰自检；③门日志目录 per-run 隔离（同秒启动不共写）。

- ①已落工厂夹具（现势 file:line）：`tools/test-kvfix-verdicts-contract.py:83`
  `SCRATCH = … "kvfix-contract-scratch.%d" % os.getpid()`；
  `tools/publish-scrub-gate.py:343-344` `selftest-%d % os.getpid()`。
- ②③此前未见（台账 :41「部分」；逃逸台账修复对账末句「并行安全自检/日志目录隔离未见」）。本批只收这两件。

### 7.2 隔离形态（只收紧）

| 项 | 旧形态 | 新形态（RT-E73） |
|---|---|---|
| RUNDIR | `$HOME/all-gates-logs/<YYYYMMDD-HHMMSS>`（`mkdir -p`，同秒共写） | `$HOME/all-gates-logs/<YYYYMMDD-HHMMSS>-<pid>`（`mkdir` 排他；已存在则加 nsec 消歧） |
| 分配点 | 启动即建目录 | **flock 拿到之后**再 `alloc_rundir`（拒跑实例不再留下空时间戳目录） |
| 单例锁 | `$HOME/all-gates-logs/.all-gates.lock`（DEB-10 `flock -n`，第二实例 exit 2） | **不放宽**。`--selftest-perrun` / `--selftest-perrun-child` 在 flock 之前早退，以免 L3 常驻项在父持锁下再起本脚本被锁假红 |
| 父目录 / 外置锚 | `$HOME/all-gates-logs/`、`gate-pins.anchor` | 位置不变 |

落点（工厂现势 `all-gates.sh` sha256=`d44add1c9adf…`）：`alloc_rundir` :280；主流程锁 :1537-1541；锁后分配 :1546；L3 常驻项 :1165-1166
`bash tools/all-gates.sh --selftest-perrun`（L3 `add_item` 38→39，只增不减；同批合入 RT-E58 mes-ttl 两项，未摘门）。

### 7.3 消费者影响面

新目录名是旧形态的**真后缀**，既有读法继续成立：

- 汇总行仍打印 `日志目录：$RUNDIR`。L5/L6 自测 `grep -o '日志目录：[^ ]*'`（`l5-deploy-gate-selftest.sh:86` / `l6-deploy-gate-selftest.sh:72`）吃完整路径，含 pid 后缀。
- `ls -t ~/all-gates-logs`、glob `~/all-gates-logs/*/item-*.result` 仍匹配。
- 锁文件与 `gate-pins.anchor` 仍落父目录（`gate-pins-check.sh:42` / `gate-pins-refresh.sh:95`）。
- 历史时间戳目录原样保留，不作迁移、不删。

会断的写法：把 RUNDIR 钉死成「恰好 15 字符时间戳、无后缀」的路径拼接（本批工厂 `tools/` 扫描：生产代码无此拼接，仅注释里的历史目录名）。af/ci 不读 `~/all-gates-logs`，本仓无配套改动。

### 7.4 本仓边界

- **改**：本节（docs 新增一节）。
- **不改**：`crates/`、`ci/`、`Cargo.toml`/`Cargo.lock`、本仓 `tools/`（与工厂 `tools/` 不是同一棵树）。
- 工厂 `tools/gate-pins.list` 只许 `bash tools/gate-pins-refresh.sh` 机生成，禁手改。



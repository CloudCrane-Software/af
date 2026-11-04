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
| 6 | cosign 断网 `verify-blob --trusted-root`（run.sh:290-320） | ① `$AF_ROOT/bin/cosign`（run.sh:292）——**不入库**：`.gitignore` 行 2 `/bin`（亲证 `git ls-files bin/` 为空）；② 断网手段 `sudo unshare -n` 或回退 `unshare -Urn`（run.sh:300-309，run.sh 内建）；③ 资产 `poc/poc-cas/verify-tr/`（run.sh:293）**✓ 入库** | **✗ ②③在位，①缺席——干净克隆七步跑不齐的唯一步** | **跳过档（SKIP，environment_unavailable）**：宿主无 `bin/cosign` 时允许以 SKIP 入账并在 SUMMARY 标注缺因，等价验证=F2 独立 runner（已供给 cosign 的宿主）补跑 step6。**SKIP 不得计绿**；现状 run.sh 预检对缺件 fail-closed（run.sh:117-121 收 PREFLIGHT），此跳过档是**对账口径**而非预检改写。带外供给方案：把核过哈希的 cosign 二进制放宿主 clone 的 `$AF_ROOT/bin/cosign`（bin/ 不入库，供给属宿主操作，不触碰仓库内容）。 |
| 7 | `cargo deny --offline check`（run.sh:322） | ① `cargo-deny` 子命令二进制（PATH，仓外 `cargo install` 供给）；② advisory-db 本地库位 `~/.cargo/advisory-dbs/advisory-db-3157b0e258782691`（宿主状态，非仓内；刷新走 `ci/refresh-advisory-db.sh`，代理 `socks5h://100.64.0.3:1080`，GitHub 不可达时沿用旧库 WARN 降级——deny.toml:5-6 与刷新脚本内建） | ✗ 两者皆宿主供给 | **跳过档（SKIP，environment_unavailable）**：cargo-deny 或本地 db 缺席时允许 SKIP 入账并标注缺因，等价验证=F2 独立 runner 补跑。SKIP 不得计绿。 |

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

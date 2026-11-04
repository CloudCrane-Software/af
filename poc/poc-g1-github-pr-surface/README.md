# POC · G1 GitHub 端 PR 反馈面（`.github/` 缺席面补建）

> 批次：G1（欠账总表 §2「GitHub PR 端反馈面零落地」）
> 施工树：`/home/pan-ding/workspace/af-wt-g1github`（分支 `feat/g1-github-pr-surface`，
> 基线 `origin/product/main` = `56ff52a`；**未 commit、未 push**）
> 日期：2026-11-04
> 交付面：`af/.github/workflows/f1.yml`（新）、`af/ci/gh_workflow_check.py`（新）、
> `af/docs/CI-BOUNDARY.md`（仅追加 §六）、本证据目录（新）。
> 原始读数：本目录 `readings/`（00–06）。

---

## 一、Gitea 端 F1 快速门的「语义面 / 环境面」拆分表

正本：`af/.gitea/workflows/f1.yml`（sha256 `4002392f221aff4aa125d0d1df95600ba83e6190065422cc65ad5cac886292d2`，
与 `origin/product/main` 逐字一致——见 `readings/03-zero-change-and-fmt.txt` §3.3）。
行号锚见 `readings/04-gitea-f1-anchors.txt`。

### 1.1 语义面（判定内容：跑什么、判什么、什么算红）

| # | 语义件 | Gitea 端行号 | 判据内容（GitHub 端必须 1:1 对等） |
|---|---|---|---|
| S1 | 触发面 | `:29-32` | **仅** `pull_request`；base 分支约束 `product/main`（产品主线保护门） |
| S2 | 并发策略 | `:35-37` | `group: f1-<PR 头分支>`；`cancel-in-progress: true`（同分支重复推送只保最新一次） |
| S3 | 超时止损 | `:45` | `timeout-minutes: 45`（首次填 target 20–30min，稳态 <15min） |
| S4 | fmt 硬门 | `:106-109` | `cargo fmt --check --all`（非零即红） |
| S5 | clippy 硬门 | `:111-117` | `cargo clippy --workspace --all-targets -- -D warnings`（**全量警告零容忍**，比 F2 的基线门更严） |
| S6 | 快速测试硬门 | `:119-123` | `cargo test -p af-dsl -p af-compiler`（封闭子集：零 docker/netns/cosign/POC 资产依赖） |
| S7 | 诊断收集语义 | `:113-114`、`:120` | clippy 与快速测试 step 带 `if: always()` ⇒ **一次 run 收全三路诊断**，job 结论仍「任一红即红」 |
| S8 | F2 不进 Actions | `:7-10`、`:24` | 全量门（`cargo test --workspace` / step6 cosign 断网验签 / netns / POC 资产）**不进**本 workflow |
| S9 | 分工声明 | `:59-62` | step 内声明 F1/F2 分工（口径面，防止后人把全量门塞进来） |

### 1.2 环境面（环境依赖：跑在哪儿、用什么 checkout、凭据怎么来、target 放哪）

| # | 环境件 | Gitea 端行号 | 内容 |
|---|---|---|---|
| E1 | runner 标签 | `:42` | `runs-on: [opch-ci]`（自托管 OpenCloudOS 9.6 容器，标签须与实例注册一字不差） |
| E2 | 构建节流 | `:47-49` | `CARGO_BUILD_JOBS=4` / `CARGO_INCREMENTAL=0` / `CARGO_TERM_COLOR=never`（本厂内存纪律） |
| E3 | target 持久化 | `:51-54` | `CARGO_TARGET_DIR=/var/lib/opch-ci/.af-f1-target`（跨 run 复用 ⇒ 稳态分钟级） |
| E4 | 凭据注入 | `:55-57` | `AF_TOKEN=${{ secrets.GITEA_TOKEN }}`（每次运行自动注入的短期 token；仓为 private） |
| E5 | checkout 方式 | `:64-81` | `git clone --depth 1 --branch <head ref>`；**零外部 action 依赖、不用 `actions/checkout`** |
| E6 | 工具链供给 | `:83-104` | runner 容器无 rust ⇒ rustup 自举；按 `rust-toolchain.toml` 钉住 channel 装 `rustfmt`/`clippy` 组件 |
| E7 | 事件上下文 | `:27`、`:36`、`:66` | `gitea.event.pull_request.head.ref` / `gitea.ref_name` 命名 |

> 拆分的判据：**「跑什么/判什么/什么算红」= 语义面**；**「在哪儿跑/用什么 checkout/凭据从哪来/target 放哪」= 环境面**。
> 语义面 1:1 对等是硬要求；环境面可按 GitHub 侧现实改写，但**每处改写必须逐条登记**（见 §二）。

---

## 二、GitHub 端对等实现说明 + 逐条「改了什么 / 为什么 / 语义损失」

落点：`af/.github/workflows/f1.yml`（新目录 `.github/`；仓内此前 `.github/` 不存在 ⇒ 这就是 G1 债本体）。

### 2.1 语义面对等核（逐件，1:1）

| 语义件 | Gitea 端 | GitHub 端 | 对等性 |
|---|---|---|---|
| S1 触发 | `pull_request` + `branches: [product/main]` | `pull_request` + `branches: [main]` | ★ **分支名不同但已实测按实际写**（见 §2.3）；触发形态 1:1 |
| S2 并发 | `group: f1-${{ gitea... }}` + `cancel-in-progress: true` | `group: f1-${{ github... }}` + `cancel-in-progress: true` | 1:1（仅事件上下文命名不同） |
| S3 超时 | `timeout-minutes: 45` | `timeout-minutes: 45` | 1:1（检查器判 ≤45，不得放宽） |
| S4 fmt | `cargo fmt --check --all` | 同 | 1:1 |
| S5 clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 同 | 1:1（**未**降级为基线门） |
| S6 快速测试 | `cargo test -p af-dsl -p af-compiler` | 同 | 1:1（封闭子集未扩到 `--workspace`） |
| S7 诊断收集 | clippy/测试 step `if: always()` | 同 | 1:1 |
| S8 F2 不进 | 声明 + 实际不含全量门 | 同 | 1:1（检查器 R11 机械判定） |
| S9 分工声明 | step 内 echo 两行 | 同 | 1:1 |

### 2.2 环境面改写逐条登记（改了什么 / 为什么 / 语义有无损失）

| # | 环境件 | Gitea 端 | GitHub 端（改了什么） | 为什么 | **语义损失核** |
|---|---|---|---|---|---|
| E1 | runner | `runs-on: [opch-ci]`（自托管容器） | `runs-on: ubuntu-24.04`（托管 runner，钉版本不用浮动标签） | GitHub 侧无自托管 runner；托管镜像自带 git/rustup | **无**：判据内容（跑哪些门、什么阈值）一字未改；仅换宿主。**代价**：架构/OS 与自托管不同（OpenCloudOS 9.6 → Ubuntu 24.04），产物为公开披露镜像，风险面见 §五 |
| E5 | checkout | `git clone --depth 1 --branch <head ref>` + `secrets.GITEA_TOKEN` 注入 | 同款 `git clone --depth 1 --branch <head ref>`，**URL 优先取 `github.event.pull_request.head.repo.clone_url`（fork 感知），缺席回落本仓 `github.repository`**；**不注入任何 token** | GitHub 披露镜像为**公开仓** ⇒ 匿名 clone 即可；「零长期凭据」纪律 ⇒ 连 `secrets.GITHUB_TOKEN` 也不引 | **无判据损失**（检出的是同一棵 PR 头树）；**登记的差异**：① fork PR 的 clone 源仓不同（Gitea 端 `pull_request` 亦为同仓模型，GitHub 侧按平台现实补 fork 感知）；② 检出的是 **head 分支 tip**（与 Gitea 端同，非 merge commit）⇒ 存在「事件后分支又推进 ⇒ 跑的不是 PR 那次 sha」的竞态，**Gitea 端同款**，1:1 不改 |
| E4 | 凭据 | `AF_TOKEN=${{ secrets.GITEA_TOKEN }}` | **零 secret 引用**（公开仓匿名 clone） | 「不引用任何长期凭据，只许 GitHub 自动注入的 GITHUB_TOKEN」；公开仓用不着 | **无**；且凭据面**更紧**（连自动注入 token 都不用 ⇒ 零凭据落盘、零凭据进 argv） |
| E3 | target | `/var/lib/opch-ci/.af-f1-target`（跨 run 持久） | `${{ runner.temp }}/af-f1-target`（每 job 一次性） | 托管 runner 无跨 run 持久盘；且 `actions/cache` 属 action ⇒ 零 action 纪律下不可用 | **判据无损失**（同一批门、同一阈值）；**代价是速度**：每次 run 冷编译（更慢，**不是更松**）。Gitea 端靠自托管持久盘省时间，GitHub 端没有这个条件 ⇒ 如实登记为环境面差异 |
| E6 | 工具链 | 容器无 rust ⇒ rustup 自举 + 钉住 channel 装 rustfmt/clippy | 同款两段（`command -v cargo` 命中即复用，否则 rustup-init 自举）；随后 `rustup toolchain install "$CHANNEL" --profile minimal --component rustfmt --component clippy` + `rustup default` | 托管镜像自带 rustup，但**仓内 `rust-toolchain.toml` 钉住 channel=1.99.1** ⇒ 必须显式装钉住版与 rustfmt/clippy 组件，否则语义件跑的是非钉住工具链 | **无**（供给物与 Gitea 端同：钉住 channel + rustfmt + clippy 组件） |
| E2 | 构建节流 | `CARGO_BUILD_JOBS=4` / `CARGO_INCREMENTAL=0` / `CARGO_TERM_COLOR=never` | 同值逐字保留 | 本厂内存纪律（14G 机器禁高并发编译） | **无** |
| E7 | 事件上下文 | `gitea.*` | `github.*`（`github.event.pull_request.head.ref/sha/repo.clone_url`、`github.repository`、`runner.temp`、`github.workspace`） | 平台上下文变量名不同 | **无** |

### 2.3 ★ GitHub 侧 base 分支名＝`main`（实测，非假设）

- `git ls-remote --symref github HEAD`（remote `github` = `https://github.com/Feasy898/af.git`）
  → `ref: refs/heads/main  HEAD`，**exit 0** ⇒ GitHub 披露镜像的默认分支 = **`main`**。
- 同仓 `--heads` 全量 **24** 个分支中 **含 `product/main` 的 = 0** ⇒ GitHub 侧**不存在** `product/main`。
- 处置：按实际写 `branches: [main]`；**映射语义不变**（Gitea 端 `product/main` 是「产品主线」，GitHub 镜像侧对应「默认分支 `main`」）。
- 原始读数：`readings/00-github-branch-probe.txt`
  - **§B「复跑（成功）」条** = symref 的 **exit 0 成功原始输出**（返修批追加，§七）——
    `.github/workflows/f1.yml:21-27` 的注释引用指向该条；
  - §A = 首次直连 URL 形态的 `exit=124`（超时、无输出）记录，**保留不删**（如实留痕）。

### 2.4 ★ 安全面（硬写死，检查器逐条判定）

| 面 | 处置 |
|---|---|
| `pull_request_target` | **禁用**（拿 base 仓上下文跑不可信 PR 代码 = 典型提权面）。本 workflow 只用 `pull_request` |
| 触发面 | **只许 `pull_request`**；无 `push` / `workflow_dispatch` / `schedule` / `workflow_run` / `issue_comment` |
| `permissions` | 顶层 + 每个 job 双写 `contents: read`（只读；job 级不继承假定） |
| 凭据 | **零 `secrets.*` 引用**；零硬编码凭据；匿名 clone |
| 第三方 action | **零 `uses:`**（连 GitHub 第一方 `actions/checkout` 也不引 —— 对等 Gitea 端「零外部 action 依赖」纪律的最严解读）。**本批未引入任何第三方 action，也未因此停手**（判定：零 action 可行，checkout 用裸 `git clone` 实现） |
| F2 全量门 | 不进本 workflow（检查器 R11 机械判定命令位） |

---

## 三、★ 检查器设计 + 削弱变体自证读数

落点：`af/ci/gh_workflow_check.py`（新件；**零第三方依赖**：内建 YAML 子集解析器，PyYAML 在场时双解析交叉核对，结构不一致即红）。
用法：`python3 -I ci/gh_workflow_check.py`（检现役面）/ `--selftest`（负向自证）/ `--list-rules` / `--json`。

### 3.1 规则表（R01–R22；★ R18–R22 为返修批新增，见 §七）

| 规则 | 判定内容 | 对应纪律/语义件 |
|---|---|---|
| R01 | `.github/workflows/` 存在且非空 | G1 债本体不得再欠 |
| R02 | YAML 解析成功（解析失败 = fail-closed 红）+ 双解析交叉核对一致 + **重复键判红** | 判据不可判 = 红 |
| R03 | 生效文本无 `pull_request_target` | 禁提权触发 |
| R04 | 触发面 **仅** `pull_request` 且带 base 分支约束 | S1 |
| R05 | 顶层 + 每个 job 的 `permissions` 均无 write、含 `contents: read`、job 级不得缺席 | 只读收窄 |
| R06 | 只许自动注入 token；禁其它 `secrets.*`、禁硬编码凭据形态、禁凭据字面量 env | 零长期凭据 |
| R07 | 零 `uses:` | 零 action 依赖 |
| R08 | `cargo fmt --check --all` **在命令位**在位（★ 返修批：改命令位判定 + 拒吞红） | S4 |
| R09 | `cargo clippy --workspace --all-targets -- -D warnings` **在命令位**在位，且**不降级**（禁 `clippy_gate.py` / `--cap-lints` / `-A warnings`） | S5 |
| R10 | `cargo test -p af-dsl -p af-compiler` 封闭子集**在命令位**在位 | S6 |
| R11 | F2 全量门不得进（命令位判定：`ci/run.sh` / `cargo test --workspace|--all` / `cargo deny` / `cosign` / `unshare` / `netns`） | S8 |
| R12 | 每个 job 有 `timeout-minutes` 且 ≤ 45 | S3（不得放宽） |
| R13 | 顶层 `concurrency` 在位（`group` 非空 + `cancel-in-progress: true`） | S2 |
| R14 | clippy 与快速测试 step 带 `always()` | S7 |
| R15 | 每个 job 有 `runs-on` | 环境面最小面 |
| R16 | 工具链供给在位（`rustup` + `rustfmt` + `clippy`） | E6（否则语义件跑不了） |
| R17 | 每个 step 都有 `run` 体（零 action 形态） | 形态面 |
| **R18** | **语义 step（fmt/clippy/快速测试）不得带 `if:` 抑制条件**（只许 `always()` 恒跑形态；`if: false` / `success()` 等一律红） | ★ 返修批新增 |
| **R19** | **job 级 `if:` 不得恒假**（`false` / `${{ false }}` / `false && …`） | ★ 返修批新增 |
| **R20** | **`on.pull_request` 不得带 `types` / `paths` / `paths-ignore`**（收窄触发面 = 削掉「同分支重复推送即重跑」） | ★ 返修批新增 |
| **R21** | **吞红写法一律红**（`|| true` / `|| :` / `; true` / `set +e`，任何 step 的命令位） | ★ 返修批新增 |
| **R22** | **判据面自检 step 在位且恒跑**（F1 面不得删掉自检 step；被调用处须带 `always()`） | ★ 返修批新增 |

### 3.2 ★ 削弱变体自证（23 个变体，逐个须红且命中预期规则）

命令：`python3 -I ci/gh_workflow_check.py --selftest`（每个变体 = 现役 workflow 的「判据面被削弱」形态；
检查器以**子进程实跑**，断言 `exit≠0` **且** 命中预期规则 Rxx ⇒ 不是「碰巧红」）。
原始读数：`readings/02-selftest-variants.txt`。

| 变体 | 削弱形态 | exit | 命中规则 |
|---|---|---|---|
| V01 | 引入 `pull_request_target` | 1 | R03 ✓ |
| V02 | 顶层 `permissions` 放宽为 `write` | 1 | R05 ✓ |
| V03 | job 级 `permissions` 放宽为 `write` | 1 | R05 ✓ |
| V04 | 引入 `actions/checkout@v4` | 1 | R07 ✓ |
| V05 | clippy 去掉 `-D warnings`、改接 `ci/clippy_gate.py`（降级成基线门） | 1 | R09 ✓ |
| V06 | 删掉 fmt 语义件 | 1 | R08 ✓ |
| V07 | 快速测试换成 `cargo test --workspace` | 1 | R10 ✓ |
| V08 | 删掉 `timeout-minutes` | 1 | R12 ✓ |
| V09 | 删掉 `concurrency` | 1 | R13 ✓ |
| V10 | 引用 `secrets.MY_PAT` | 1 | R06 ✓ |
| V11 | 触发面放宽到 `push` | 1 | R04 ✓ |
| V12 | 去掉 base 分支约束 | 1 | R04 ✓ |
| V13 | clippy step 去掉 `always()` | 1 | R14 ✓ |
| V14 | 快速测试封闭子集缩水（漏 `af-compiler`） | 1 | R10 ✓ |
| V15 | 把 F2 全量门（`bash ci/run.sh`）塞进 Actions | 1 | R11 ✓ |
| V16 | fmt 门降级：`cargo fmt --check --all` → `cargo fmt` | 1 | R08 ✓ |
| V17 | clippy 门降级：`-D warnings` → `-D clippy::all` | 1 | R09 ✓ |
| V18 | 超时放宽 45 → 120 | 1 | R12 ✓ |
| V19 | `cancel-in-progress: true` → `false` | 1 | R13 ✓ |
| V20 | job 级 `permissions` 整块删掉（依赖继承） | 1 | R05 ✓ |
| V21 | env 硬编码 PAT 字面量 | 1 | R06 ✓ |
| **V22** | **整块删掉 clippy -D warnings 语义 step**（★ 返修批新增：覆盖 R09 fail-open 路径） | 1 | R09 ✓ |
| **V23** | **整块删掉快速测试（af-dsl + af-compiler）语义 step**（★ 返修批新增：覆盖 R10 fail-open 路径） | 1 | R10 ✓ |

**汇总读数**：`POSITIVE … exit=0（期望 0）` / **23** 个变体 `exit=1` 且 `hit=yes` /
`SELFTEST SUMMARY: variants=23 unexpected=0` / 自检整体 `exit=0`。
（返修批新增 V22/V23 的逐条读数见 §七 与 `readings/08-fix-selftest-23-variants.txt`。）

> ★ **不许为了让某个变体仍绿而放宽判定**：本批未出现「变体仍绿」的情形；
> 若出现，实现方式是**加严规则**（如 V16/V17/V18/V19/V20/V21 即是加严后的补强），
> 不是放宽。
> ★ 两条「设计取舍」如实登记（为避免假红，判据按**生效面**判，不按注释字面量判）：
> ① R03 只扫**注释剥离后**的生效文本（注释里写「禁用 pull_request_target」是文档，不是触发面）；
> ② R11 判**命令位**（step 里 echo 一句「F2 全量门：ci/run.sh 不进 Actions」是声明不是调用）。
> 两者都只放宽「注释/声明」，**不放宽任何生效面判定**。

### 3.3 ★ 门位（触发面）—— 一个从不被任何门跑的检查器＝摆设

本批**不新增 `ci/run.sh` step 编号**、**`ci/` 下既有件零 diff**（硬约束）⇒ 门位取
「独立脚本 + SOP」路线，并叠两层机器面：

| 层 | 触发面 | 性质 | 现役是否生效 |
|---|---|---|---|
| ① | GitHub 端 PR 门：`.github/workflows/f1.yml` 的「判据面自检」step（`if: ${{ always() }}`）恒跑 `python3 -I ci/gh_workflow_check.py` | 机器 | **否**——GitHub 通道 blocked（§五） |
| ② | 门族面字节对账：`ci/gh_workflow_check.py` 落 `ci/` 门码面 ⇒ 既有跨树收口门 `ci/fix_verify.py` 机制③「门族全集单源」自动把它纳入逐树双核对账（提交面 oid + 工作树面 sha256）⇒ **检查器本体被删/被改弱 ⇒ 该既有门红** | 机器 | **是**（现役可跑） |
| ③ | SOP：①改动 `.github/**` 的 PR 合流前须 `python3 -I ci/gh_workflow_check.py`（exit 0）+ `--selftest`（exit 0）；②主线合流批可把它挂进 `ci/run.sh` **既有**槽位（**本批不挂**） | 流程（人工触发） | 是（须被触发） |

- 层 ② 的机器依据：`ci/fix_verify.py:199-206`（`surface_maps`：门族面三段＝`ci/` 全集 + 判据/证据正本 + 数据面）
  与 `:210-231`（`collect_surface`/`canonical_surface`：排除面＝`*.bak` 与 `ci/out/`；正本面取自主干基线 ref）、
  `:268-300`（逐树双核：①提交面 blob oid + ②工作树面 sha256 + ③数据面盘面；分叉/缺件/多余件即红）。
- ★ 如实登记：**层 ① 当前未生效**；层 ② 保护的是**检查器本体**；层 ③ 是 SOP 面。
  「削弱 workflow ⇒ 机器红」这一动作由层 ① 承担，**待 GitHub 通道恢复后生效**；
  本批以 **23** 个削弱变体的负向读数**预先证明**该动作是机械判定的（非人工自觉）。
- ★ 已知边界（诚实登记）：GitHub `pull_request` 事件用 **PR 头分支的 workflow 文件**
  ⇒ 判据面自检跑的是 PR 头里的那份检查器；PR 若同时削弱 workflow 与检查器，自检会一起失效
  （自证不可自证）。缓解：base 分支保护 + required check（base 侧文件不被 PR 头改写时判定生效）、
  层 ② 的跨树字节对账、主线合流评审。**本批不解决该问题**（另记）。

---

## 四、门读数与边界

### 4.1 本批实跑的门（逐门给读数）

| 门 | 命令 | 读数 |
|---|---|---|
| fmt | `cargo fmt --check --all`（工作树 `af-wt-g1github`） | **exit 0**（cargo 1.99.1 / rustc 1.99.1；耗时 0.94s；不产生 target、不占构建盘） |
| 面自检（正例） | `python3 -I ci/gh_workflow_check.py` | **exit 0**；`SUMMARY: files=1 rules=22 fail=0 failed_rules=-`（★ 返修批：R01–R17 ⇒ R01–R22，见 §七） |
| 面自检（负向自证） | `python3 -I ci/gh_workflow_check.py --selftest` | **exit 0**；`variants=23 unexpected=0`（正例 exit=0；**23** 个削弱变体逐个 exit=1 且命中预期规则；★ 返修批新增 V22/V23，见 §七） |
| af 仓零改动核 | `git status --porcelain` / `git diff --stat origin/product/main` | 仅 4 项：`M docs/CI-BOUNDARY.md`、`?? .github/`、`?? ci/gh_workflow_check.py`、`?? poc/poc-g1-github-pr-surface/` |
| `.gitea/workflows/f1.yml` 逐字未动 | `git diff origin/product/main -- .gitea/`（空）+ sha256 对账 | 空 diff；工作树面 sha256 = 基线面 sha256 = `4002392f…92d2` |
| `ci/clippy-baseline.txt` 未变 | sha256 对账 | 工作树面 = 基线面 = `a48639e75b5203fef7e1ffc7026b8f0d9de20f3ce862fa3988a1ec6b49326ba4` |
| `ci/` 既有件零 diff | `git diff --name-only origin/product/main -- ci/`（空）+ `git status --porcelain -- ci/` | 空 diff；状态面只有新增件 `?? ci/gh_workflow_check.py` |
| `crates/` 零新增 crate | `git diff --name-only origin/product/main -- crates/` + `git status --porcelain -- crates/` | 两项均空 |
| `Cargo.toml`/`Cargo.lock`/`rust-toolchain.toml` 零改动 | `git diff --name-only origin/product/main -- …` | 空 |
| refs 未动 | `git -C <主检出树> rev-parse HEAD product/main origin/product/main` | 三者同为 `56ff52a57c022983d74d994fd4e1c63e98f45235`（`product/main` 指针未动） |
| 主检出树未被本批改动 | `git -C <主检出树> status --porcelain` | 仅开工前已有的 `?? redteam/scans/2026-11-04-e4484537/`（本批未碰） |
| 在飞登记面（防本批新增假红） | `python3 -I ci/fix_verify.py --repo <主检出树>` | 本树 `[PASS] 在飞登记背书成立`；该门整体 **红 7**，7 项全部是**其它批次**的在飞工作树漏登记（`af-wt-fmtfix`/`af-wt-l3l3`/`af-wt-l3l3-base`/`af-wt-review-mainline`/`af-wt-satcast`/`af-wt-t5t6`/`af-wt-t7-2`）⇒ **本批未新增红** |

### 4.2 ★ 未跑的门（如实登记「未跑 + 理由」）

| 门 | 状态 | 理由 |
|---|---|---|
| `cargo build` | **未跑** | 本批**禁改 `crates/`**（硬约束），且任务书明令不为此在本地跑重编译；产物由主线全量门复验批承担（本机 14G 内存） |
| `cargo test` | **未跑** | 同上；本批无 Rust 改动，测试结论与主线同 |
| `cargo clippy` | **未跑** | 同上；任务书指派由主线全量门复验批（anolis 在跑，tip `56ff52a`）承担 |
| GitHub 侧实跑（PR 上真跑本 workflow） | **未跑** | GitHub 通道 blocked + 镜像无可发布物 ⇒ 本 workflow 在 GitHub 侧尚不存在（§五） |
| 镜像披露终扫 | **未跑** | 本批禁碰镜像披露（`tools/mirror-disclose.sh` 与任何 allowlist/denylist）；当前 `verdict=blocked` 的状态是既有事实，本批未重跑 |

### 4.3 其它边界（如实登记）

- **零 commit、零 push**：本批 diff 全部以工作树面呈现，留给主代理；分支 `feat/g1-github-pr-surface`
  仅创建（`git worktree add -b`，基线 `origin/product/main`），未提交、未推送、未改 `product/main` 指针。
- **未动共享主检出树**：所有改动在 `/home/pan-ding/workspace/af-wt-g1github`；主检出树只追加了
  `af-state/resources/inflight-worktrees.list` 一行与 `af-state/resources/leases.db` 一条租约
  （gitignored 运行态面，用于避免既有跨树收口门因本批新增工作树而假红）。
- **证据零凭据正文**：本目录与本批产物中不含任何凭据形态；租约/登记面只出现
  `holder`/`resource`/`work_interval` 等逻辑标识与 sha256 指纹。
- **duty 树门族面补达**：本批新增 1 件 `ci/` 门码面文件（`ci/gh_workflow_check.py`）⇒ 合流到主线后，
  在役 duty 树须走既有的「门族面补达」流程（先例：`30-工件/duty树门族面补达-20261104/`），
  否则 `ci/fix_verify.py` 机制③ 会对 duty 树报「门族多余件」。**本批未做该补达**（超出本批范围）。
- **本批未引入任何第三方 action**：若后续有人判定「必须引 action 才做得成」，按纪律应停手报告，
  本批不自行引入。

---

## 五、本批不做什么 + GitHub 侧生效条件

### 5.1 本批**不**做什么

1. **不**动 GitHub 发布通道、**不**发布镜像、**不**改 `tools/mirror-disclose.sh`、**不**改任何
   allowlist/denylist（`tools/mirror-allowlist.paths` / `mirror-denylist.paths` / `mirror-scan-allowlist.json`）。
2. **不**处置 GitHub 公开仓已有的 5 件 POC 私钥（`30-工件/凭据轮换盘点-20261104/` 的 #71）——
   那是另一笔、且处置权在运营者；本批产物与证据零凭据正文。
3. **不**改 Gitea 端 F1（`.gitea/workflows/f1.yml` 逐字未动，sha256 已对账）。
4. **不**动 AF 产品的 F2 全量门路径（`ci/run.sh` 十四步管道、`ci/out/` 产物面、收件箱派单与
   integrator 复核制）、**不**动判分链、账本、协议、注册表。
5. **不**改 `crates/` 下任何件、**不**改 `ci/` 下既有件（含 `ci/run.sh` 的任何 step 与编号）、
   **不**改 `Cargo.toml`/`Cargo.lock`/`rust-toolchain.toml`、**不**改既有测试断言。
6. **不**新增 `ci/run.sh` step 编号；**不** push、**不** commit、**不** amend/rebase、
   **不** force push、**不** `git reset --hard`、**不** sudo、**不**改 `product/main` 指针。

### 5.2 ★ GitHub 侧何时生效（当前 **未生效**）

**生效条件（四条，缺一不可）**：

1. **披露通道恢复**：镜像终扫 `verdict=ok`（当前 `verdict=blocked`、`hits=1205`、无可发布物——
   见 `30-工件/镜像披露-20261104/终审-allowlist与放行.md:274`）⇒ GitHub 侧才拿得到本 workflow。
2. **镜像 allowlist 增补 `.github`**：`tools/mirror-allowlist.paths` 当前**无** `.github` 条目
   （实测 grep 零命中）⇒ 即使 ① 恢复，发布快照里的 `.github/` 会被**裁出**，workflow 仍到不了
   GitHub。**本批禁改该清单**（只登记，不动）⇒ 这条是**另一批/运营者**的动作。
3. **GitHub 仓侧 Actions 可用**（仓设置开 Actions；本 workflow 需托管 runner 额度）。
   另：`ci` 在 allowlist 内 ⇒ `ci/gh_workflow_check.py`（自检 step 的被测脚本）会随快照到达。
4. **仓保护接线**：把该 workflow 设为 required check（否则 PR 面无强制力）。

**恢复后须重验（哪几步）**：

- ① 对**发布后的**仓面跑 `python3 -I ci/gh_workflow_check.py` ⇒ exit 0；
- ② `python3 -I ci/gh_workflow_check.py --selftest` ⇒ exit 0（**23** 变体仍逐个红）；
- ③ 首个真实 PR 上三个语义 step 实际跑起来并各绿（fmt / clippy -D warnings / af-dsl+af-compiler）
  ——注意 GitHub 托管 runner 冷编译可能逼近 `timeout-minutes: 45`（**本批无法实测**：通道 blocked），
  若超时，须**先复核判据**再议（判据面不得为「跑起来」而放松）；
- ④ 通道恢复 + allowlist 增补两件事都完成后，再把 required check 接上。

### 5.3 本批**未生效**的诚实声明

- 本 workflow 在 GitHub 侧**尚未存在**（镜像无可发布物 + allowlist 未含 `.github`）；
- 本批交付的是**面（workflow 文件 + 校验器 + 证据）**，**不是**「在 GitHub 上启用它」；
- 本批**未**尝试 push 到 GitHub、**未**尝试发布镜像、**未**触碰任何凭据面处置。

---

## 七、G1 返修批（2026-11-05）：fail-open 关闭 + 判据加严 + 文书/证据修好

> 本段为**返修批追加**（原 §一–§六 的读数与结论一字未删；§二.3/§三.1/§三.2/§四.1/§五.2
> 仅为「与现役产物一致」而做的**事实性订正**，逐处已标「★ 返修批」）。
> 新增原始读数：`readings/07-fix-failopen-closed.txt`、`08-fix-selftest-23-variants.txt`、
> `09-fix-probe-p01-p14.txt`、`10-fix-gates-and-zero-change.txt`、`11-fix-fingerprints.txt`；
> `readings/00-github-branch-probe.txt` **追加** §B（symref 成功原始输出；§A 原 exit=124 记录保留）。

### 7.1 ★ 否决项：R09/R10 缺 `else` ⇒ fail-open（已关闭）

- **根因**（复核员指出，本批复核属实）：`ci/gh_workflow_check.py` 里 R08 有
  `else: report.fail(...)`（`:632-635`），**R09/R10 没有** ⇒ 「语义件齐备」的判定是
  **fail-open**：语义 step 被整块删掉时，既不报 `[OK]` 也不报 `[FAIL]`，检查器照样 **exit 0**。
- **修法**（照抄 R08 写法补 else；失败信息写明「语义件缺失 = 判据弱化」+ 规则号 + 文件相对路径）：

  ```python
  if clippy_ok:
      report.ok("R09", "%s: clippy -D warnings 在位" % rel)
  else:
      report.fail("R09", "R09: %s: 未见 clippy 语义件 `cargo clippy --workspace --all-targets -- -D warnings`"
                        "（语义件缺失 = 判据弱化：删掉该 step = 硬门缺席且检查器放行）" % rel)
  if test_ok:
      report.ok("R10", "%s: af-dsl+af-compiler 快速测试在位" % rel)
  else:
      report.fail("R10", "R10: %s: 未见快速测试语义件 `cargo test -p af-dsl -p af-compiler`"
                        "（语义件缺失 = 判据弱化：删掉该 step = 硬门缺席且检查器放行）" % rel)
  ```

  （R08 的 else 同步补上 `R08:` 前缀，三条消息口径一致。）
- **复现与验证**（`readings/07-fix-failopen-closed.txt`）：
  - 改前：把现役脚本复制一份、**只**摘掉 R09/R10 两段 else ⇒ 对 M05/M06 **exit 0 / fail=0**，
    且 **R09、R10 连 `[OK]` 都不出现** ⇒ **复现复核员的 fail-open**；
  - 改后（现役）：M05 ⇒ **exit 1 / failed_rules=R10**；M06 ⇒ **exit 1 / failed_rules=R09**。
- **判据只能变严**：补 else **只**让「缺失」变红；正例仍 **exit 0**
  （`SUMMARY: files=1 rules=22 fail=0 failed_rules=-`）。

### 7.2 变体表 21 → 23（V22/V23 覆盖 fail-open 路径）

- 旧 21 变体里**只有 V06（删 fmt step）**覆盖到「语义件缺失」路径 —— 因为只有 R08 带 else
  ⇒ 覆盖不到本 fail-open 路径。
- 新增：**V22 = 整块删 clippy 语义 step（期望 R09）**；**V23 = 整块删快速测试语义 step（期望 R10）**。
- 复跑 `--selftest`：**POSITIVE exit 0**；**23/23 逐个 exit=1 且命中预期规则**；
  `SELFTEST SUMMARY: variants=23 unexpected=0`；自检整体 **exit 0**（逐条读数见 `readings/08-…`）。

### 7.3 ★ 判据面加严（建议项，全部做；**只增不减检出力**）

| # | 加严项 | 实现 | 覆盖的探针 |
|---|---|---|---|
| a | **R08/R09/R10 改命令位判定** | 复用 R11 的口径（逐行剥注释、只判行首命令；新 helper `_command_lines()`）：写在 `echo`/注释里的命令**不算**语义件在位 | P01–P03 |
| b | **拒吞红写法**（`\|\| true` / `\|\| :` / `; true` / `set +e`） | 新规则 **R21**（任何 step 的命令位）；且吞红写法**不算**该语义件「在位」（R08/R09/R10 同时判红） | P04/P05/P10/P11/P13 |
| c | **语义 step 不得带 `if:` 抑制条件** | 新规则 **R18**：fmt/clippy/快速测试 step 的 `if:` 只许 `always()` 恒跑形态；`if: false` / `success()` 等一律红 | P06 |
| d | **job 级 `if:` 不得恒假** | 新规则 **R19**：`false` / `${{ false }}` / `false && …` 一律红 | P07 |
| e | **`on.pull_request` 不得带 `types`/`paths-ignore` 收窄触发面** | 新规则 **R20**；★ **加判 `paths`**（与 `types`/`paths-ignore` 同形的收窄） | P08/P09 |
| f | **判据面自检 step 在位且恒跑** | 新规则 **R22**：F1 面（workflow `name` 含 F1）必须存在调用 `gh_workflow_check.py` 的 step，且带 `always()` | P12 |
| g | **重复键 fail-closed** | 内建 YAML 子集解析器在 `_parse_mapping` 里检出重复键 ⇒ `ParseError` ⇒ R02 判红（重复键 = 解析歧义，不放行） | P14 |

- **规则数 17 → 22**（R01–R17 + R18–R22）；`SUMMARY` 里的 `rules=` 随之由 17 变 22。
- ★ **一条如实留痕的修正**：P06 初版写成「在 clippy step 里**再插一行** `if: false`」⇒ 实测**仍绿**；
  原因是同映射内**重复键**被 YAML 解析器**后者覆盖前者** ⇒ 该变异在语义上不生效。
  处置：① P06 改成复核员本意（把 `if: ${{ always() }}` **替换**为 `if: false`）⇒ 转红（R18+R14）；
  ② 另立 P14 覆盖「重复键夹带」⇒ 在解析层 fail-closed（R02）。
  ⇒ 两处修法都是**判据变严**，不是放宽。

### 7.4 复核员 P01–P09 九个探针的逐条读数（＋ 自加 P10–P14）

命令：`python3 -I poc/poc-g1-github-pr-surface/probe-p01-p09.py`（探针本体入库，可复跑）。
原始输出见 `readings/09-fix-probe-p01-p14.txt`。

| 探针 | 削弱形态 | 读数 | 命中规则 |
|---|---|---|---|
| P01 | 注释掉 fmt 命令行 | **红**（exit 1） | R08 |
| P02 | 注释掉 clippy 命令行 | **红**（exit 1） | R09 |
| P03 | 注释掉快速测试命令行 | **红**（exit 1） | R10 |
| P04 | fmt 命令行 `\|\| true` | **红**（exit 1） | R21 + R08 |
| P05 | clippy 命令行 `\|\| true` | **红**（exit 1） | R21 + R09 |
| P06 | 语义 step `if: false`（替换 always()） | **红**（exit 1） | R18 + R14 |
| P07 | job 级 `if: false` | **红**（exit 1） | R19 |
| P08 | `on.pull_request` 加 `types: [opened]` | **红**（exit 1） | R20 |
| P09 | `on.pull_request` 加 `paths-ignore` | **红**（exit 1） | R20 |
| P10 | 【自加】fmt 命令行 `; true` | **红**（exit 1） | R21 + R08 |
| P11 | 【自加】clippy step 里 `set +e` | **红**（exit 1） | R21（R09 仍判在位：命令本身未被改） |
| P12 | 【自加】整块删掉「判据面自检」step | **红**（exit 1） | R22 |
| P13 | 【自加】自检命令 `|| true` | **红**（exit 1） | R21 |
| P14 | 【自加】重复键夹带第二个 `if:` | **红**（exit 1） | R02 |

`PROBE SUMMARY: probes=14 still_green=0` ⇒ **复核员九项全部转红，无「仍绿」项**。

### 7.5 文书 / 证据修好（逐处）

| # | 位置 | 改前 | 改后 |
|---|---|---|---|
| a | `.github/workflows/f1.yml:25` | 「ls-remote --heads 实测 **23** 个分支中无 product/main」 | 「实测 **24** 个分支」（复核员亲跑 `git ls-remote --heads github \| wc -l` = 24；本批复跑同值） |
| b | `readings/00-github-branch-probe.txt` | symref 记的是 **exit=124**（超时、无输出） | **追加 §B**：symref **exit 0** 的原始输出（`ref: refs/heads/main	HEAD`）+ `--heads` 全量 24 行 + `wc -l`=24 + `grep -c product/main`=0；**§A 原 exit=124 记录一字未删** |
| c | README §2.3 | 引用笼统指向 `readings/00` | 明指 **§B「复跑（成功）」条**（并说明 §A 的 exit=124 保留） |
| d | `.github/workflows/f1.yml:21-27` | 注释里的 symref 命令没指向成功那条 | 注释改指 **§B**，并登记「首次直连 URL exit=124、原记录保留」 |
| e | `docs/CI-BOUNDARY.md` §6.1 | 「R01–R17」/「21 个变体」/「23 个分支」 | 「R01–R22」/「23 个变体」/「**24** 个分支」+ symref 命令改指 §B |

### 7.6 `docs/CI-BOUNDARY.md` §6.2 层② 生效前提 + §6.5 残余风险（建议项，全部做）

- **层 ② 的生效前提（已补进 §6.2）**：层 ② 的对账面 = **主干基线 ref 上的 `ci/` 门码面**；
  本检查器本体是**新件、尚未并入任何基线 ref** ⇒ 实测
  `git ls-tree phase2 ci/ | grep -c gh_workflow` = **0**（`origin/product/main`、`product/main`
  同为 0）⇒ **合并进主干基线 ref 之前，基线门码面里没有本件，机制③ 的逐树双核对账覆盖不到它
  ⇒ 本体尚未受字节对账保护**（与 §4.3 的「duty 树门族面补达」缺口对齐）。
  ⇒ 如实登记：三层门位里**当前没有任何一层机械地保护本件**（层 ① 通道 blocked、层 ② 前提未成立、
  层 ③ 是 SOP 面须人工触发）。
- **残余风险 RR-01（已单列为 §6.5）**：**PR 头的 Rust 构建脚本（`build.rs` 等任意代码）
  会在 GitHub 托管 runner 上执行** —— 只要本 workflow 对 PR 开放执行就必然发生；
  当前凭据面为零 secret + 只读 token ⇒ 风险可控，但「托管 runner 上的任意代码执行」本身
  **不因凭据面为零而归零**（另含 crates.io 依赖的供应链面）。
  另注：Gitea 端 F1 跑在**自托管 runner**（`runs-on: [opch-ci]`）⇒ 同一执行面在那边落在
  **本厂自管容器**内 ⇒ **把它挪到托管 runner 是环境面改写带来的新执行面**，与 §二 的
  「环境面改写登记」同列。

### 7.7 返修批门读数（逐条；原始输出见 `readings/10-…`）

| 门 | 命令 | 读数 |
|---|---|---|
| fmt | `cargo fmt --check --all` | **exit 0**（cargo 1.99.1 / rustc 1.99.1；**不产生 target 目录**，实测 `ls -d target` = 不存在） |
| 面自检（正例） | `python3 -I ci/gh_workflow_check.py` | **exit 0**；`SUMMARY: files=1 rules=22 fail=0 failed_rules=-` |
| 面自检（负向自证） | `python3 -I ci/gh_workflow_check.py --selftest` | **exit 0**；`variants=23 unexpected=0` |
| 否决项复现（改前复原件） | `python3 -I <仅摘 R09/R10 else 的复原件> --root <M05/M06>` | M05/M06 均 **exit 0**（= 复现 fail-open，证明根因定位正确） |
| 否决项验证（改后现役） | `python3 -I ci/gh_workflow_check.py --root <M05/M06>` | M05 **exit 1 / failed_rules=R10**；M06 **exit 1 / failed_rules=R09** |
| 探针面 | `python3 -I poc/poc-g1-github-pr-surface/probe-p01-p09.py` | `probes=14 still_green=0` |
| af 仓零改动核 | `git status --porcelain -uall` / `git diff --stat origin/product/main` | 仅 `M docs/CI-BOUNDARY.md`（**纯插入 112 行、0 删除**）+ 新增件 `.github/workflows/f1.yml`、`ci/gh_workflow_check.py`、`poc/poc-g1-github-pr-surface/**` |
| `crates/` 零 diff | `git diff --name-only origin/product/main -- crates/` | **空**；crate 数 `ls crates/ \| wc -l` = **22 → 22** |
| `.gitea/` 零 diff | `git diff --name-only origin/product/main -- .gitea/` | **空**；`sha256sum .gitea/workflows/f1.yml` = 工作树面 = 基线面 = `4002392f…92d2` |
| `ci/` 既有件零 diff | `git diff --name-only origin/product/main -- ci/` | **空**；`git status --porcelain -- ci/` 只有新增件 `?? ci/gh_workflow_check.py` |
| `ci/clippy-baseline.txt` 未变 | sha256 对账 | 工作树面 = 基线面 = `a48639e7…26ba4` |
| refs 未动 | `git rev-parse HEAD product/main origin/product/main` | 三者同为 `56ff52a57c022983d74d994fd4e1c63e98f45235`（**`product/main` 指针未动**） |

### 7.8 返修批的边界（如实登记）

- **零 commit、零 push**：本批全部改动以工作树面呈现（未提交、未推送、未 amend/rebase、
  未 `git reset --hard`、未 sudo）；分支仍是 `feat/g1-github-pr-surface`。
- **`cargo build` / `cargo test` / `cargo clippy` 三门：未跑** —— 禁改 `crates/` ⇒ 本批无 Rust
  改动可验，且任务书明令本机 14G 内存不为此跑重编译；产物由 anolis 在跑的主线全量门复验批承担。
  **不写成已跑、不写假绿。**
- **GitHub 侧实跑：未跑** —— 通道 blocked + 镜像无可发布物 ⇒ 本 workflow 在 GitHub 侧尚不存在。
- **本批未做**：GitHub 发布通道、镜像披露面（`tools/mirror-disclose.sh` 与任何 allowlist/denylist）、
  Gitea 端 F1（`.gitea/workflows/f1.yml` 逐字未动）、`ci/` 下既有件、`ci/run.sh` 任何 step（含编号）、
  `crates/` 下任何件、`product/main` 指针。
- **本批未解决的既有边界（沿用 §3.3 / §6.4 的登记）**：GitHub `pull_request` 用 **PR 头分支的
  workflow 文件** ⇒ PR 若同时削弱 workflow 与检查器，自检会一起失效（自证不可自证）。
- **证据零凭据正文**：本批新增产物（`ci/gh_workflow_check.py`、`probe-p01-p09.py`、
  `readings/07–10`）中不含任何凭据形态字面量（V21 的 PAT 夹具仍是运行时拼装形态）。

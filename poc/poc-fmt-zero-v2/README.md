# poc-fmt-zero-v2 — 主线 fmt 归零·第二版（以当前主线为基）

批次：feature/mainline-fmt-zero-v2（基线 = origin/product/main @ **68be206**）· 执行机 anolis-gpu-01 · 2026-11-04
提交：`6c001f0`（纯格式化产物，23 文件 +316/−255）+ 本证据与 CI-BOUNDARY §二 step1 口径更新（随批第二笔提交）。

## 为什么有第二版

第一版 `feature/mainline-fmt-zero`（afa3e4c + 5747ae0）基于旧基线 dfea0c0，独立复核虽判可收卷，
但其后主线又合入三支带**新代码**的分支（router 消费腿 / 在飞树登记面 / admit_seal 补面 /
worktree-recycle 收敛 / 编译器诊断产物面），新代码未格式化，直接 merge 会在同文件上撞车。
rustfmt 输出只取决于文件内容 ⇒ 以当前主线 68be206 为基重跑一次 `cargo fmt --all`，结果等价且零冲突。
本批全部读数为**本机在 68be206/6c001f0 上亲跑**，未沿用第一版任何读数（`poc/poc-fmt-zero/`
不在主线上，仅经 `git show feature/mainline-fmt-zero:...` 作方法参照）。

## 门读数对照（改前 / 改后，全部本机亲跑，独占 target=/data/bench/targets/fmt-zero-v2）

| 门 | 改前（68be206 原树） | 改后（6c001f0） |
|---|---|---|
| `cargo fmt --check --all` | **exit=1**，99 diff 块 / 23 文件（输出 1258 行） | **exit=0，输出 0 行——归零** |
| `cargo test --workspace --no-fail-fast` | exit=0；152 套件；**1510 passed / 0 failed / 9 ignored** | exit=0；152 套件；**1510 / 0 / 9**；**逐套件三元组 diff 零差异**（152/152，剥墙钟计时后全同） |
| `cargo clippy --workspace --all-targets --message-format=json` | exit=0；626 条消息；**error 级 0 条** | exit=0；626 条消息；**error 级 0 条** |
| `python3 -I ci/clippy_gate.py <jsonl>` | PASS（0 警告 / 0 签名，基线 0 签名） | **PASS**（0 警告 / 0 签名） |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit=0 | exit=0 |

改前红面构成（99 块）：af-order/src/router.rs 16、admit.rs 14、adjudicate.rs 12（近批合入的新代码），
tests/adjudicate_loop.rs 8、examples/router_consume_leg.rs 6、execline/ledger.rs 6，其余 43 块散布
af-workorder/af-execline/af-engine/af-mes/release-gate 旧漂移——与 83bb71c 时点在案预存红同源。
（对照：第一版改前同为 99 块/23 文件但构成不同——旧漂移为主；数字巧合，读数各自独立采集。）

## 纯格式化证明（零语义变更，最强机械判据）

1. **逐字节同源证明**：`git archive 68be206` 抽原始树到 /tmp/fmt-fresh-v2（不经手本工作树），
   在彼处**独立**跑 `cargo fmt --all`（同版 rustfmt 1.10.0-stable，仓库钉 1.99.1，无 rustfmt.toml
   → 全默认配置），与分支格式化后的工作树做全树 `diff -r -x .git -x .offload`——
   **exit 0、零差异**（raw/diff-r-fresh-fmt-vs-branch.txt 为空）。另：全仓 **642 个 .rs 文件逐文件
   sha256 前后树全同**（raw/rs-sha256-*.txt，对差文件数 0）；fresh 树上 `cargo fmt --check --all`
   亦 exit=0（幂等性自证，raw/fresh-fmt-check-after.*）。分支树每一字节都等于 rustfmt 对原始
   68be206 的机械输出，手改无处藏身。
2. **人工逐块核对**：全量 diff（raw/fmt.diff，1336 行）23 文件逐块核过（raw/per-file-nature.tsv
   逐文件归类）。性质仅四类：A 拆行/合行（表达式与方法链重排）、B 多行参数表补尾逗号、
   C 导入/模块/重导出重排（rustfmt 默认 reorder_imports/reorder_modules）、D 纯缩进修正
   （identity.rs，`git diff -w` 残差=0）。
3. **字符串字面量字节零变化**：23 个改动文件**单行字符串字面量多重集前后全等**；driver/schema/router
   三文件再以「全文去空白后抽取引号段序列」复核全等（117/38/240 段）——含中文错误消息、
   `#[error(...)]` 属性串、带 `\n` 的 DDL 串（af-mes/schema.rs）、STEWARD_SEED_HEX，引号内字节零触碰。
4. **断言/测试守恒**（raw/assertion-counts.txt）：
   - 抽样 2 个改动过的测试文件（af-order/tests/adjudicate_loop.rs、af-workorder/tests/lifecycle.rs）：
     `#[test]` 5/3、`#[ignore]` 0/0、assert* 34/22 条、**测试函数名清单 md5 前后一致**——逐项相同。
   - 全部 23 个改动文件合计：`#[test]` 126→126、`#[ignore]` 0→0、assert* 579→579；diff 中零
     `#[test]`/`#[ignore]` 增删行。无测试函数删除、无 `#[ignore]` 新增、golden 快照零触碰。
5. **判据面零触碰**（raw/criteria-blobs.txt）：68be206→HEAD 非 .rs 改动文件数=0（本笔 fmt 提交）；
   `ci/clippy-baseline.txt`、根 `Cargo.toml`（members 未动，diff 0 行）、`.gitea/workflows/f1.yml`、
   `ci/baselines/clippy-baseline.sha256` 的 git blob 哈希前后逐一相同；crates/ 目录条目集合一致，
   零新增 crate、零机制、零注释掉检查、零 fmt 豁免、调用形态未动。

## CI-BOUNDARY §二 step1 口径改动（仅此一处）

主线现文该条为「step1 fmt：**红**——主线预存状态…」（83bb71c 时点在案）；第一版曾在**其未合入分支**上
改为「归零…合流后本行即主线真读数」——末句被独立复核判**过度承诺（P1）**。**该句从未落在主线**，
故本批在新基线上直接落成收窄后的终态：红→归零 + **条件式时点表述**——
「**该读数时点=上述本批基线，不以本行直接充当主线读数**：主线 tip 会继续前进且前移段可能含新格式
漂移，合流后 fmt 状态须以合流树重验（rustfmt 幂等，重跑代价低）」。
改前改后原文与 diff 见 ci-boundary-change.diff；§一七步外部依赖表、§三 F1/F2 分工表、§四三路回收
口径一字未动（git diff 单一 hunk，仅 §二 step1 一条）。

## 如实登记的边界（不遮丑）

- **任务书 ③ 的前提与主线实况有出入**：任务书称 §二 step1 末句「现写『合流后本行即主线真读数』」，
  实况是主线该条仍为旧「红」版（第一版含该句的 5747ae0 未合入）。本批按其意图落成收窄终态
  （红→归零 + 条件式时点句），非「改一句措辞」而是「在正确基线上一次性落成」——如实登记。
- **test 前后两次跑均用同一独占 target**（/data/bench/targets/fmt-zero-v2，执行器注入），无共享 target
  并发撞车问题；两次均为冷建后热跑，全程无外部干扰迹象（152 套件两跑全绿）。
- **改前/改后 test 墙钟计时不同**（如 adjudicate_loop 26.74s→23.74s）：逐套件对照剥计时后零差异，
  计时本身不作判定读数。
- **主线此后仍会前进**：本批归零读数时点=68be206+6c001f0；若主线在前移段引入新 .rs，合流树 fmt
  状态须重验（已在 CI-BOUNDARY §二 step1 条件句中写明）。
- **做不到的**：本批只能让 F1 的 fmt 步在**本分支树**上绿；gitea Actions 端到端绿须待分支合入
  product/main 后的 run 实证（F1 只在 pull_request→product/main 触发）。
- rustfmt 版本敏感性：读数绑定 rustfmt 1.10.0-stable（rust-toolchain.toml 钉 1.99.1 生效），
  升版 rustfmt 可能引入新漂移（届时须重跑，非本批范围）。

## raw/ 底账清单

fmt-check-before/after（exit+输出+逐文件块数）、fmt.diff（全量 1336 行）、per-file.tsv（hunk/残差）、
per-file-nature.tsv（逐文件性质归类）、diff-r-fresh-fmt-vs-branch.txt（空=全树逐字节全同）、
rs-sha256-{fresh,worktree,diff}.txt（642 文件逐文件 sha256）、fresh-fmt-check-after.*（幂等自证）、
clippy-{before,after}.jsonl(+.stderr/.exit)、clippy-D-{before,after}.*、clippy-gate-{before,after}.txt、
test-{before,after}.log/.exit/.suites.txt、test-suite-diff-timing-stripped.txt（空=152 套件全同）、
assertion-counts.txt（抽样+合计）、assertion-counts-23files.txt、criteria-blobs.txt（判据件 blob 对账）。

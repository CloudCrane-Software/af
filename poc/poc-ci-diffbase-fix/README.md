# poc-ci-diffbase-fix —— L1 CI 门基点缺陷修正（--diff-base 缺省取错分支）+ L1' PR 基线治理登记

- 批次：外派任务书「施工面：L1」（30-工件/远端main分叉治理盘点-20261104 判出）
- 工作树：`~/af-dev/af/wt/ci-diffbase-fix/`（执行器建，基线 = `origin/product/main` @
  `25429293b0671460d394cb217290f5cebf024a52`；构建盘 `/data/bench/targets/ci-diffbase-fix`）
- 分支：`fix/ci-diffbase-correct`（自上述基线建支）
- 环境：`PATH=$HOME/.cargo/bin`、`CARGO_BUILD_JOBS=8`、`LANG=C.UTF-8`、
  `CARGO_TARGET_DIR=/data/bench/targets/ci-diffbase-fix`（执行器注入，未改）

## 一、改动清单（恰好三件，与任务书硬约束一致）

| 文件 | diff 规模 | 性质 |
|---|---|---|
| `ci/fix_scan.py` | +8/−2 | 缺省值改 `HEAD~1`（:564 + 头注 :9 同步）+ 4 行注释（说明缺省依据与 fail-safe 语义不变）；**族定义/白名单/fail-loud 判据零触及**（白名单正本在 `ci/fix-scan.yaml`，与基线 0 行 diff） |
| `ci/run.sh` | +1/−1 | 仅 step13a 那一行的 `FIXSCAN_DIFF_BASE` 缺省 `origin/main` → `HEAD~1` |
| `docs/CI-BOUNDARY.md` | +20/−0 | 纯新增 §五「合流目标与 PR 基线口径」（既有 §一—§四 逐字节未动，diff 全为 + 行） |

## 二、★核心读数：step13a 改前/改后对照

缺陷缺省 `origin/main` 在本仓的显形（**与任务书预测的 4101 文件形态互补，同为路径过滤失效**）：
本仓 `product/main`（根 83bb71c，42 commits）与 `main`（根 8b07a7d，182 commits）是
**无共同祖先的两条孤儿线**——`git diff origin/main...HEAD` 恒 `fatal: no merge base`
（两点点 diff `origin/main..HEAD` = 4101 文件，复现任务书与旁证 `ci/out/step13a.log:2`
的 3656→4101「一直涨」指纹；两线一旦可解析即按对照线全树差集恒全族触发）。

| 读数 | 改前（缺省 origin/main） | 改后（缺省 HEAD~1，裸调证缺省已生效） |
|---|---|---|
| 实际看到文件数 | diff 失败（no merge base）→ **不可解析** | **18**（`git diff --name-only HEAD~1...HEAD` = 18） |
| 模式走向 | WARN 回落 `mode=full`（diff 档路径过滤**死**） | `mode=diff` 正常路径过滤 |
| 触发族 | 2/2（全族，非按本批改动面） | 2/2（本批 18 文件真实触及 `crates/**/*.rs` 与 `crates/af-execline/src/**`，**合法触发**） |
| sat-cast 命中 | **1 白名单外命中**（graph_unit.rs:1167，exit 1） | **1 白名单外命中**（同一条，exit 1） |
| pyc-shadow 命中 | 0（双法一致） | 0（双法一致） |
| exit | **1（红）** | **1（红）** |

**修后仍红，如实报告（不消红）**：红源=**既有白名单锚漂移**，与本批基点修正无关——
`ci/fix-scan.yaml` sat-cast 白名单锚 `graph_unit.rs:1068`（D-50），该站点随文件增长
漂移到 **:1167**（`turns: stage.turns as u32,`，模式仍命中），白名单锚过期 ⇒ 命中落在
白名单外（工具建议号 D-53）；两条 WL-STALE 提示（1068 / shadow.rs:392）同证漂移。
改前回落 full、改后正常 diff **扫到同一条**（族扫描面=full_scan_globs，本就不随基点变），
即此红在改前同样在场，非「修后才红」。按任务书：**未改白名单、未放宽判据**；处置建议=
白名单锚随 D-50 站点行号更新（1068→1167 类修订须走 gate_inputs 同批评审，非本批动作）。

证据文件：`readings/step13a-before.txt`（改前实跑）、`readings/step13a-before-repro.txt`
（修后代码显式传旧基点的复现跑，stdout 与改前逐行一致）、
`readings/step13a-after-default.txt`（缺省裸调）、`readings/step13a-after-explicit.txt`
（任务模板显式形态）、`readings/{before,after}-batch/`（两形态产物 JSON+sha256，
`mode` 字段 full↔diff 对照）。

## 三、五门读数

| 门 | 读数 |
|---|---|
| 1. `python3 -I ci/fix_scan.py --validate` | **环境不可跑**：`-I` 隔离态排除用户 site（PyYAML 装于 `~/.local/lib/python3.11/site-packages`）→ `[fix-scan] FATAL: 需要 PyYAML` exit 2（**改前改后同因同码**，非本批改动引入）。**替代核验**（run.sh step13a 实际调用形态 `python3 ci/fix_scan.py --validate`）：改前 exit 0 / 改后 exit 0，schema 通过（2 族 sat-cast, pyc-shadow；债册 50 号） |
| 2. step13a 实跑（改前/改后） | 见 §二对照表：改前 exit=1（回落 full，1 命中）/ 改后 exit=1（diff 18 文件，1 命中，同源） |
| 3. `cargo fmt --check --all` | **exit 0**（`readings/gate-fmt.txt`） |
| 4. `cargo test --workspace --no-fail-fast` | **exit 0**；三元组 **passed=1552 / failed=0 / ignored=9**（156 个测试二进制；touch 后全量重建 5m07s；log 内 2 处 "panicked" 字样均为测试名 `…_is_reported_not_panicked`，实跑 ok） |
| 5. clippy + clippy_gate | `cargo clippy --workspace --all-targets --message-format=json` exit 0，**error 级 0 条**（587 条 compiler-artifact = touch 后全量重 lint 非空跑证据）；`python3 -I ci/clippy_gate.py <jsonl>` → **CLIPPY GATE: PASS**（0 警告 / 0 签名，基线 0 签名内） |

## 四、判据零弱化证明

- `ci/clippy-baseline.txt` sha256 `a48639e75b52…` == 哈希钉 `ci/baselines/clippy-baseline.sha256`（0 数据行），与 `origin/product/main` **0 行 diff**；
- `crates/` Cargo.toml 集合与基线 **diff 空**（零新增 crate）；
- `ci/fix-scan.yaml`（族定义/白名单正本）、`ci/budget-cast-whitelist.json`、`ci/debt-register.md` 与基线 **0 行 diff**；
- fail-safe 分支逐字在场：`changed_files()` 基点不可解析 → 返 `None` → `cmd_run` 回落 `mode=full`（「宁多扫不漏扫」），本次 diff 未触及该路径（`readings/diffs-detail.txt` 全文 diff）；
- `git status --porcelain` 逐件：跟踪件改动**恰好** `M ci/fix_scan.py`、`M ci/run.sh`、`M docs/CI-BOUNDARY.md` 三件；untracked 三项分类：`?? .offload/`（执行器留）、`?? poc/poc-ci-diffbase-fix/`（本批证据）、`?? redteam/scans/2026-11-04-25429293/`（step13a 门产物，历史批次均入库但本批按「只许改三件」不提交，读数已拷贝进本证据目录）；
- `docs/CI-BOUNDARY.md` diff 全为 `+` 行（§五新增段），既有七步依赖表/F1-F2 分工表/三路回收口径逐字节未动。

## 五、gitea 只读读数（GET only，零写操作）

- `default_branch` = **`main`**（`GET /api/v1/repos/opchadmin/af` → 200；full_name opchadmin/af）；
- `GET …/branch_protections` → 200，**count = 0**；
- `branches/main` → 200，protected=**false**，sha f3731134；`branches/product/main` → 200，protected=**false**，sha 2542929；
- 凭据：`~/.git-credentials` 以 `urllib.parse` 解析（主机段 `<内网地址>%3a3300` unquote 后比对），凭据仅以指纹 `sha256(cred)[:12]=5c5df08f27c9` 出现，**明文零落盘**；
- 脚本：`scripts/gitea_readonly_check.py`（`python3 -I` 运行，读数 `readings/gitea-readonly.json`）。
- 登记：改 default_branch（→product/main）属 **L2 运营者签认项**、建分支保护属 **L2'' 签认项**，本批均未动。

## 六、边界与如实登记

1. **证据瑕疵（如实）**：改前实跑的产物 JSON 原件被本人一次误操作 `cp` 覆写（前后产物同
   batch 名 `2026-11-04-25429293`）。未伪造：`readings/before-batch/` 为修后代码显式传旧
   基点 `--diff-base origin/main` 的**真实复现跑**产物（该代码路径未被本批 diff 触及），
   stdout 与改前实跑逐行一致（`step13a-before.txt` ↔ `step13a-before-repro.txt`）；改前
   原件 sha256（sat-cast.json=4c0e04de10fb…）已在过程留痕中记录于本 README 之前，可对拍。
2. **refs 布局操作（如实）**：为复现缺陷条件，曾 `git fetch origin refs/heads/main:
   refs/remotes/origin/main`（对远端只读；执行器原布局无此跟踪 ref），改前读数取毕已
   `git update-ref -d` 恢复原布局，`git ls-remote` 复核远端 `main`/`product/main` 指针
   前后一致（f3731134 / 2542929）。**未改远端任何分支**。
3. **touch 全部 .rs 的边界情况（如实）**：`git ls-files` 默认 quotepath 把非 ASCII 路径
   转义致 xargs touch 打不到真名（9 件 `proofs/` 中文路径）；已用 `core.quotepath=off`
   重 touch，盘上在位 .rs 全部刷到新 mtime。其中 `proofs/Kani锁像剂-INV组/` 5 件
   索引在、盘上无（porcelain 不显 D），且 `proofs/` 整体被 workspace `exclude`、
   不参与 clippy/fmt/test——对「防缓存平凡绿」目的无影响（workspace `crates/*` 全部
   ASCII、全部 touch 成功、587 artifacts 证全量重 lint）。
4. `python3 -I` 下 PyYAML 不可达（见 §三门 1）——按任务书「门不可跑就如实登记替代核验
   与原因」处置，替代形态即 run.sh 实际调用形态。
5. 共享树 `~/af-dev/af` 未触碰（本批全部操作在本 worktree + 对远端只读 API/只读 fetch）。
6. `origin/main` 分支本身未动（不改/不回合/不 force/不删）；未用 sudo；无
   `git reset --hard`；无 force push；未动 `product/main` 指针。

## 七、提交与推送

- 提交范围：三件改动 + 本证据目录（`poc/poc-ci-diffbase-fix/`）。
- 推送：`git push origin fix/ci-diffbase-correct`（普通推送）。
- 提交号与远端核验结果登记于交付回执（push 后 `git ls-remote` 复核）。

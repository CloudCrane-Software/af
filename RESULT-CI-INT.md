# RESULT-CI-INT —— CI 七门首跑（phase2 主线，任务二）

- 工作树：`/home/pan-ding/workspace/af-wt-phase2-execline`（分支 phase2）
- 管道：`CARGO_TARGET_DIR=~/afbuild-phase2i bash ci/run.sh`（run.sh：七步硬门，产物落 `ci/out/`）
- 结论：**6 绿 1 红**。step7 `cargo deny --offline check` 红 = 依赖类发现
  RUSTSEC-2024-0449（unsafe-libyaml unmaintained），按纪律不改依赖、不加豁免，
  登记 blocker 交编排者；其余六门全绿。无未决破坏性改动，已 push。

## 提交图（本任务收尾时）

```
9d3a62a fix(af-compiler): 消 clippy 新增警告 doc_lazy_continuation——golden_test 注释行首 + 改词
<本文件与 ci/out 证据提交>
2fe80c1 style: rustfmt 全仓归零——19 文件纯重排（含 registry_decl_test.rs 在制重排）
1bb14df fix(af-compiler): 透传 Value 键序规范化——cedar preserve_order 特性渗漏修复
e64c39f fix(af-order): 补齐 DriverConfig P4/P5 扩展字段——合并语义残留
28ba57a merge: phase2-g2b-hyg（fmt 归零+文档清偿）并入 phase2 主线
3fccc70 merge: phase2-g3-l4（af-packages 组件声明 7→57 + 闸门矩阵 v2 15/15）并入 phase2 主线
```

## 冲突与解法（承接任务一，均非文本冲突，两合并由编排者完成）

| 残留 | 现象 | 解法 |
| --- | --- | --- |
| `crates/af-order/src/cli.rs` | DriverConfig 缺 P4/P5 六字段，workspace 编译断 E0063 | 按 af-execline `lifecycle.rs` 桩单元占位先例补齐（commit e64c39f） |
| `crates/af-compiler/src/ir.rs`+`lower.rs` | workspace feature 统一经 af-credential→cedar-policy→cedar-policy-core 开 `serde_json/preserve_order`，Value::Object 变插入序，golden 逐字节仅在 --workspace 构建下挂 4/11 | `canonical_value` 递归键序规范化 + lower.rs 三处透传入点；golden/断言零改动（commit 1bb14df） |
| `crates/af-compiler/tests/golden_test.rs:309` | clippy 基线门拦唯一新增警告 doc_lazy_continuation（注释行首 `+ ` 被 markdown 当列表标记） | 纯注释改词（commit 9d3a62a） |

## fmt 零语义证据（任务一收尾，本任务 step1 复验）

- `cargo fmt --all --check` rc=0（任务一）；本任务管道 step1 `cargo fmt --check --all` PASS 568ms。
- 零语义：`cargo test -p af-compiler -p af-dsl` 58+244=302 全绿；
  `cargo test --workspace` 118 套全 ok（1089 通过 / 0 失败）。
- golden 快照零改动：`git status crates/af-compiler/testdata/` 全程干净
  （任务一期间篡改测试触碰过 sample1/GPTSwarm 两 golden 的 mtime，内容经
  git 核实与提交态一致）。

## 七门逐门证据（第二轮全量跑，`ci/out/SUMMARY.txt`）

| # | 门 | 结果 | 命令 | 关键输出 |
| --- | --- | --- | --- | --- |
| 1 | fmt | **PASS** 568ms | `cargo fmt --check --all` | 零输出退出 0 |
| 2 | clippy | **PASS** 390ms | `cargo clippy --workspace --all-targets -j4 --message-format=json` + `ci/clippy_gate.py` | `CLIPPY GATE: PASS —— 现有 0 条警告 / 0 签名，均在基线内（基线 0 签名）` |
| 3 | test | **PASS** 9718ms | `cargo test -j4 --workspace` | 118 suites, passed=1089 failed=0 |
| 4 | cue | **PASS** 33ms | cue vet/export + 结构断言 + 反例拒 | `recipe-merged.json 结构断言: 9 值全中，键集闭合`；反例 bad-override 被 vet 拒 |
| 5 | scrub | **PASS** 240ms | `ci/parse_scrub.py` × 7 真实日志 | 2 正例 exit=0、5 反例 exit=1，`PASS-EXPECTED×2 / FAIL-EXPECTED×5` 全中 |
| 6 | cosign | **PASS** 65ms | `sudo unshare -n bin/cosign verify-blob --trusted-root …`（断网 netns） | `正例: Verified OK；篡改例: 已拒绝（exit 1）；netns: sudo unshare -n` |
| 7 | deny | **FAIL** 601ms | `cargo deny --offline check` | `error[unmaintained]: unsafe-libyaml is unmaintained`（RUSTSEC-2024-0449）→ `advisories FAILED, bans ok, licenses ok, sources ok` |

step7 红项归因（依赖类，非环境性）：
- 唯一 error：**RUSTSEC-2024-0449**，链路 `unsafe-libyaml 0.2.11 ← serde_yaml
  0.9.34+deprecated ← af-dsl ← af-compiler`（Cargo.lock:333）。上游仓库 2024-03-25
  归档、无安全升级路径（官方建议换 saphyr 等 safe-Rust 解析器 = 依赖变更）。
- deny.toml 既有豁免名单（RUSTSEC-2026-0173 / RUSTSEC-2024-0436）不含本条；
  新增豁免或换解析器均超本角色权限 → **blocker 交编排者**。
- bans 的 21 条 duplicate-versions 与 licenses 的 1 条 not-encountered 均为
  warn 级（deny.toml `multiple-versions = "warn"`），不拦门。

## advisory-db 处置（第 7 门离线机制）

- 机制（读 ci/run.sh:181 + ci/refresh-advisory-db.sh）：`cargo deny --offline check`
  用 cargo-deny 默认库位 `~/.cargo/advisory-dbs/advisory-db-<url哈希>`；
  `ci/refresh-advisory-db.sh` 负责从 GitHub（hk 代理）刷新 `ci/advisory-db/`
  再同步到该哈希库位。
- 本机现状：`~/.cargo/advisory-dbs/advisory-db-3157b0e258782691` 已存在且有
  效（HEAD 999ec89，2026-10-17「Assigned RUSTSEC-2026-0373 to ammonia」）。
  **db 不缺 → 未克隆、未刷新**，`--offline` 直接命中本地库；step7 日志无任何
  网络/db 缺失报错，红项纯为 advisories 实质发现。
- 工作树 `ci/advisory-db/` 不存在（.gitignore 忽略该路径，本就非入库件）。

## 环境件处置（预检依赖）

- `bin/cosign` 本工作树缺失（环境件，git 不跟踪，.gitignore `/bin`）：从主仓
  `/home/pan-ding/workspace/agent黑灯工厂/af/bin/cosign` 只读复制（sha256
  `4629c757b7618056f8ddd7e2625ae9fdd94c0372a65049520bc7d9df9efc7f71` 两端一致），
  未动主仓。缺它则 run.sh 预检 exit 2 一门都跑不了。
- sudo 免密可用 → step6 走真 root netns（`sudo unshare -n`）。
- 构建全部落 `CARGO_TARGET_DIR=~/afbuild-phase2i`，工作树无 target/。

## df 前后

- 开工：`/dev/nvme0n1p2 468G 217G 227G 49% /`
- 收工：`/dev/nvme0n1p2 468G 219G 226G 50% /`（全程 ≥15G 红线之上）

## 首跑 → 复跑差异

- 首跑：step1/3/4/5/6 绿，step2 红（1 条 doc_lazy_continuation 新增警告）、
  step7 红（同上 RUSTSEC-2024-0449）。
- 修 step2（纯注释改词）后全量复跑：step2 转绿，其余不变。两轮完整日志：
  `/tmp/af-ci-run1.log`、`/tmp/af-ci-run2.log`；门级产物 `ci/out/`。

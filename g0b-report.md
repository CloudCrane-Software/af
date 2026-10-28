# G0-B 组施工报告 · 真伪门三件套（B1 provenance 门 / B2 基线机算+三态化 / B3 mutations 元探针）

- **批次**：G0-CI 机制批 · 子代理 B（真伪门三件套）
- **工作树/分支**：`/home/pan-ding/workspace/af-wt-g0b`（branch `g0b`，base=`e6e05a0`）
- **规格**：`计划/研究组/G0-CI机制批-实施规格-20261027/`（总对账 + B/ 三份 SPEC）
- **施工日期**：2026-10-28
- **验收口径**：三份 SPEC ⑤「可机判判据」逐条实跑；命令与输出摘录见下文；未跑过的检查不写成已通过

---

## 一、交付清单与落点纪律自检

### 1.1 落点文件

| 文件 | 属性 | 行数 | 机制 |
|---|---|---|---|
| `ci/provenance_check.sh` | 新增 | 225 | B1 |
| `ci/baseline_calc.sh` | 新增 | 214 | B2 |
| `ci/baselines/kani-inv-harness.manifest` | 新增（9b 正本） | 41 | B2 |
| `ci/mutation_runner.sh` | 新增 | 286 | B3 |
| `mutations/registry.yaml` | 新增（探针注册表） | 36 | B3 |
| `ci/baselines/mutations-registry.sha256` | 新增（注册表基线钉） | 1 | B3 |
| `numbers.json` | 新增（B 批入键贡献件） | 27 | B2/B3 跨机制 |
| `ci/run.sh` | 改写（step0.x / step3 / 9b / a_verdict 四段，编号不变） | +82/-17 内 | B1/B2/B3 |
| `crates/af-order/scripts/af-order-test.sh` | 改写（ensure 改调 ci 级入口） | +31/-4 内 | B1 ⑧.3 裁决 |

未触碰：step10-14 区域（A/C/D 组落点）、AGENT.md、`af`（旧分支局部副本）、`af-wt-phase2-execline` 共享树、其他代理工作树。全部脚本 `--root/--target` 参数化，零硬编码他树绝对路径（root 钉死纪律）。

### 1.2 `git diff --stat` 自检（落点纪律判据，交卷前实跑）

```
$ git -C /home/pan-ding/workspace/af-wt-g0b diff --stat
 ci/run.sh                                | 82 +++++++++++++++++++++++++++-----
 crates/af-order/scripts/af-order-test.sh | 31 ++++++++++++--
 2 files changed, 96 insertions(+), 17 deletions(-)
$ git -C /home/pan-ding/workspace/af-wt-g0b status --porcelain
 M ci/run.sh
 M crates/af-order/scripts/af-order-test.sh
?? ci/baseline_calc.sh
?? ci/baselines/
?? ci/mutation_runner.sh
?? ci/provenance_check.sh
?? mutations/
?? numbers.json
```

提交后终态（10 files changed, 1318 insertions(+), 17 deletions(-)，`git status --porcelain` 为空=工作树 clean；本批 commit 见 `git log --oneline -1`，branch `g0b` tip）：

```
 ci/baseline_calc.sh                      | 214 ++++++++++++++++++
 ci/baselines/kani-inv-harness.manifest   |  41 ++++
 ci/baselines/mutations-registry.sha256   |   1 +
 ci/mutation_runner.sh                    | 286 +++++++++++++++++++++++
 ci/provenance_check.sh                   | 225 ++++++++++++++++++
 ci/run.sh                                |  82 +++++--
 crates/af-order/scripts/af-order-test.sh |  31 ++-
 g0b-report.md                            | 376 +++++++++++++++++++++++++++++
 mutations/registry.yaml                  |  36 +++
 numbers.json                             |  27 +++
```
（三个 .sh 新文件入库即带可执行位 100755；`git status --porcelain` 提交后为空。）

改动面=两处既有文件（run.sh step3/step9 段 + af-order 包装脚本）+ 本组新文件，无越界。

### 1.3 到货状态说明（如实）

本工作树到我手时已带一批未提交改动（文件 mtime 15:17-15:28，为本批早前attempt 遗留，未提交未报告）。我逐文件对照 SPEC 复核后沿用其主体，并做三处修正/新增：① `provenance_check.sh` 头注活例计数「58 处」与 SPEC ①.1 及我今日复跑实测（64 处 af-wt-f6v）不符，已改为实测口径并注明复跑证据；② 补 `numbers.json` B 批三键入账（§六）；③ 全部判据在本机重跑（早前 attempt 无任何实跑记录可继承，一切以本次实跑为准）。

---

## 二、B1 · step3 pre-step provenance 门（SPEC-B1 ⑤ 判据 1-7 全过）

### 判据 1：`bash -n ci/provenance_check.sh` 通过 ✅

```
$ bash -n ci/provenance_check.sh && echo OK
OK
（连带 bash -n ci/run.sh、bash -n crates/af-order/scripts/af-order-test.sh 均 OK）
```

### 判据 2：正例——干净 target check exit=0 ✅

构造 `.g0b-tmp/b1/fakeworktree` + `faketarget-clean`（env-dep 分别指向本树 / CARGO_HOME registry / rustc sysroot）：

```
$ bash ci/provenance_check.sh check --root .../fakeworktree --target .../faketarget-clean
provenance_check: check OK: target=... 无外来来源（本树=...）
exit=0
```

### 判据 3：反例——外来工作树 env-dep → exit=1 且 stderr 点名 ✅

```
$ bash ci/provenance_check.sh check --root .../fakeworktree --target .../faketarget-dirty
provenance_check: 外来产物来源: CARGO_MANIFEST_DIR=/home/pan-ding/workspace/af-wt-f6v/crates/af-execline（非本树 ... / CARGO_HOME / sysroot）
provenance_check: check FAIL: 共享 target 存在外来产物（1 类）——先 clean 再跑，否则绿属于别的分支（W3-C-25）
exit=1
```
（W3-C-25 原案情同款路径形态；stderr 含该路径，grep 可断言。）

### 判据 4：ensure 污染清理+盖章；危险路径一律 exit 2 且 target 未改动 ✅

```
$ bash ci/provenance_check.sh ensure --root .../fakeworktree --target .../faketarget-dirty
provenance_check: ensure: 判脏 → 清理 target 构建产物（等价 cargo clean）……
provenance_check: check OK: target=... 无外来来源
provenance_check: ensure OK: target 已清并盖章（本树=...）——此后他树复用即红
exit=0
→ 外来 .d 已清；复检 exit=0；守卫章 ci-step3-provenance-stamp.json 写入且 worktree_root=本树；
  换一棵树 check → exit=1「守卫章异树——target 上次验证树根为 …（跨树复用铁证）」。
```
危险路径逐一实跑（配对 root 与 target 同属本树的正确形态）：

| --target | exit | target 改动 |
|---|---|---|
| `/` | 2 | 无 |
| `$HOME` | 2 | 无（未落章） |
| `--root` 自身（af-wt-g0b） | 2 | 无（未落章） |
| `CARGO_HOME`（~/.cargo） | 2 | 无 |
| rustc sysroot | 2 | 无 |

另：target 不存在 → 判干净、建目录+盖章 exit 0（fail-open 于「无物可脏」）。

### 判据 5：record 三要素 JSON ✅

```
$ bash ci/provenance_check.sh record --root /home/pan-ding/workspace/af-wt-g0b \
    --target .../rectarget --out .../step3-provenance.json --test-log .../fake-step3.log
{"tool":"ci/provenance_check.sh","worktree_root":"/home/pan-ding/workspace/af-wt-g0b",
 "git_commit":"e6e05a0ceda0475a93d1bb78a97f825eed482b06","cargo_target_dir":".../rectarget",
 "manifest_dirs_count":1,"manifest_dirs":["/home/pan-ding/workspace/af-wt-g0b/crates/af-order"],
 "foreign_env_deps":"see_check_log","tests":{"passed":12,"failed":0},"generated_at":"..."}
```
`python3 -m json.tool` 可解析；机校验断 `worktree_root==AF_ROOT` 且 `git_commit==git -C $AF_ROOT rev-parse HEAD` 且 tests 汇总正确 → `PROVENANCE-CHECK: OK`。

### 判据 6：今日活例（真实共享 target 只读复跑）必须红 ✅

```
$ bash ci/provenance_check.sh check --root /home/pan-ding/workspace/af-wt-g0b --target ~/afbuild-phase2i
provenance_check: 外来产物来源: CARGO_MANIFEST_DIR=/home/pan-ding/workspace/af-wt-f6v/crates/af-compiler（非本树 ...）
…（af-credential/af-dsl/af-execline/af-graph/af-meter/af-order/af-workorder 各一条）
provenance_check: 外来产物来源: CARGO_MANIFEST_DIR=/home/pan-ding/workspace/af-wt-phase2-execline/crates/af-compiler（非本树 ...）
provenance_check: check FAIL: 共享 target 存在外来产物（9 类）——先 clean 再跑，否则绿属于别的分支（W3-C-25）
exit=1
$ grep -rho 'CARGO_MANIFEST_DIR=[^[:space:]]*' ~/afbuild-phase2i | grep -c 'af-wt-f6v'
64
```
只读操作（find+grep），未对共享 target 做任何写入。raw 计数 64 处 af-wt-f6v，与 SPEC ①.1 亲测口径一致（总数 67=64 外来+2 他树+1 registry 合法项）；去重后 9 类外来 manifest 目录（8 类 af-wt-f6v + 1 类 af-wt-phase2-execline——后者对主线 root 合法、对本树 root 属外来，故本树口径 9 类、主线口径 8 类，SPEC ④ 的「8 类」是主线 root 口径）。

### 判据 7：域守卫夹具 26 断言回归不破 ✅

```
$ TMPDIR=... bash crates/af-order/scripts/test-af-order-target-guard.sh
…（26 条 PASS 全列）
pass=26 fail=0
exit=0
```
（本机制未改动 af-order-target-guard.sh；夹具含包装脚本 --dry-run 三断言亦全绿。）

### 附加验证 A：run.sh step3 段接线三场景实跑（抽取真代码块 + stub cargo）

| 场景 | 结果 |
|---|---|
| A 干净/不存在 target | ensure 绿 → cargo 跑 → record 落档（三要素机校验 OK）→ step3 exit=0；复跑（有章同树）→ `check OK` 直接放行无误清 |
| B 危险 target（$HOME） | ensure exit=2 → step3 exit=2，**cargo 未跑、record 未跑、HOME 未落章**（门红即停，无假绿记录） |
| C 外来污染 target | ensure 判脏→清→复检干净→盖章 → cargo 跑 → record 落档 → exit=0 |

### 附加验证 B：af-order 包装端到端（⑧.3 裁决接线，真跑）

```
$ bash crates/af-order/scripts/af-order-test.sh        # TMPDIR=~/af-ci-tmp（环境配额，见 §八）
[af-order-test] CARGO_TARGET_DIR: /home/pan-ding/afbuild-af-wt-g0b-af-order（按工作树隔离派生，W3-C-25）
provenance_check: check OK: target=... 无外来来源（本树=/home/pan-ding/workspace/af-wt-g0b）
…cargo test -p af-order 全绿（7 个 test result 行全 ok）…
[af-order-target-guard] record: 验收记录 → .../af-order-evidence/af-order-provenance-20261028T093008Z-257778.json
[af-order-test] 测试退出码: 0
wrapper-rc=0
```
域证据 JSON 三要素：worktree_root=af-wt-g0b / branch=g0b / commit=e6e05a0 / manifest_dirs=2 / test_list=59。ci 级 ensure 经 `guard_ensure` 实跑生效；退出码透传正确（首轮因 /tmp 配额 cold build 失败时 wrapper-rc=101 如实透传，非 0 伪装）。

---

## 三、B2 · 9a/9b 基线机算 + 显式退出码（SPEC-B2 ⑤ 判据 1-6 全过）

### 判据 1：`bash -n ci/baseline_calc.sh` 通过 ✅

### 判据 2：正例——真实树只读对账一致，count=32/定理 21 条 ✅

```
$ bash ci/baseline_calc.sh verify-kani --root /home/pan-ding/workspace/af-wt-g0b
baseline_calc: 9b 基线对账一致: count=32, names-sha256=6ee08a95cf92…
exit=0
$ bash ci/baseline_calc.sh verify-rocq --root /home/pan-ding/workspace/af-wt-g0b
baseline_calc: 9a 清单对账一致: 21 条定理名 + statements-sha256=57f6393869e9…
exit=0
```
与 run.sh 原手写 32、9a-2 现状口径逐位一致（机算可复现正本，替换零语义漂移）。

### 判据 3：反例——正本任一数据行/count/sha 被改 → exit=1 + 漂移诊断 ✅

| 篡改面 | exit | stderr 诊断 |
|---|---|---|
| `count 32→31` | 1 | `9b FAIL: 基线漂移——期望 count=31/sha=6ee08a95…，源机算 count=32/…` |
| `names-sha256` 行改零 | 1 | `9b FAIL: 基线漂移——期望 count=32/sha=000000000000…` |
| 数据行改一个 harness 名（count/sha 行不动） | 1 | `9b FAIL: 基线数据行漂移（count/sha 相同但逐行不一致——查重复名）`+diff |
| 正本缺席 | 1 | `9b FAIL: 基线正本缺席（...）` |

### 判据 4：手写常数清除 + 正本 count 行=32 ✅

```
$ grep -n '\-ne 32' ci/run.sh        # 零命中（exit 1）
$ awk -F'\t' '$1=="count"{print $2}' ci/baselines/kani-inv-harness.manifest
32
```
9b 期望值改从正本 count 行读入（`exp_k`），正本缺席/漂移时 `verify-kani` 先行红（run.sh 9b 段内 `bash "$CI_DIR/baseline_calc.sh" verify-kani --root "$AF_ROOT"`，路径参数化到本树）。

### 判据 5：ADVISORY-FAIL 注入演练（抽取 run.sh 真代码块 `T9_0=$(now_ns)`→`printf '  [9]'`，非复刻）✅

| 态 | fv_advisory | step9.log | a_verdict | step9-failures.json |
|---|---|---|---|---|
| 1 | exit 0 + 全绿行 | 无 FAIL | `ADVISORY-PASS` | 不落 |
| 2 | exit 1 + 仅工具缺席 WARN 行 | 无 FAIL | `ADVISORY-WARN` | 不落 |
| 3 | exit 1 + `9a FAIL: 定理名集合漂移…`/`9b FAIL: 基线对账不过…` | 有 FAIL | `ADVISORY-FAIL` | 落 |

FAIL 件内容（`python3 -m json.tool` 可解析）：
```json
{"verdict": "ADVISORY-FAIL", "step9_failures": ["9a FAIL: 定理名集合漂移（期望 21 条 ↔ 实得 20 条）", "9b FAIL: 基线对账不过（kani-inv-harness.manifest 缺席或漂移）"], "log": "ci/out/step9.log"}
```

### 判据 6：step10 消费形状（对 A 的 step10_check.py 喂 FAIL 件，三向）✅

夹具 deliverables.yaml 将 FAIL 件登记为 done 证据（evidence_path=ci/out/step9-failures.json + sha256 + mtime 窗口），跑 A 树 `ci/step10_check.py`：

| 场景 | step10 结果 |
|---|---|
| 件在场 + sha 正确 | `step10 PASS: 承诺-交付对账闭合`（`OK [G0-M05] done evidence/sha256/mtime 窗口 0.0d ≤ 7d`） |
| 件缺席 | `step10 FAIL`（`RED [G0-M05] evidence 缺席：ci/out/step9-failures.json`）——未消化/未落档即红 |
| 件被改（sha 不符） | `step10 FAIL`（`RED [G0-M05] sha256 不符：登记=06c8b9bc… 实算=05fa6b55…`） |

消费形状闭合：FAIL 件可被 step10 以 done 证据形式登记，缺席/篡改均红。step9 退出码不变（仍非阻塞，九门口径）。

---

## 四、B3 · mutations/ 元探针框架（SPEC-B3 ⑤ 判据 1-6 全过）

### 判据 1：语法 + 注册表 JSON 子集可解析 ✅

```
$ bash -n ci/mutation_runner.sh          → OK
$ python3 -c "import json;json.load(open('mutations/registry.yaml'))"
registry OK: probes= ['MUT-001', 'MUT-002']
```

### 判据 2：注册表完整性三态 ✅

| 态 | exit | 诊断 |
|---|---|---|
| 未动（真树） | 0 | `注册表完整: 2 探针, sha256=46b08df4ef0b…` |
| 删任一探针条目（fake root 同代码路径） | 1 | `探针清单漂移：期望 2 条/哈希 46b08df4…，实得 1 条/哈希 ed011462…` |
| 删基线文件（fail-closed） | 1 | `探针清单漂移：注册表基线缺席（...）——防护未钉，fail-closed 红` |

### 判据 3：MUT-001 语义（真树全量演练实跑）✅

前置：真树 `cargo fmt --check --all` 基线干净（保证探针红因唯一=门脚本被删）。全量演练（scratch 硬链接隔离，`--scratch-dir` 指同设备非树内路径）：

```
$ bash ci/mutation_runner.sh run --root /home/pan-ding/workspace/af-wt-g0b --mode full --scratch-dir .../b3/scratch
mutation_runner: 注册表完整: 2 探针, sha256=46b08df4ef0b…
  [probe MUT-001] run exit=2（期望 nonzero）
  [probe MUT-001] stderr 子串命中: k5-path-lint.py
  [probe MUT-001] PASS        ← 删门脚本 k5-path-lint.py → 门红（exit≠0 + stderr 含被删文件名）
  [probe MUT-002] run exit=1（期望 1）
  [probe MUT-002] stderr 子串命中: 探针清单漂移
  [probe MUT-002] PASS        ← 删探针 MUT-001 → check-registry 红（自指防护）
mutation_runner: 全探针 PASS（模式=full）
run-exit=0
```

### 判据 4：mirror 审计（门命令演化而探针没跟上 → 探针漂移红）✅

fake root 内把 run.sh step1 命令 `python3 ci/k5-path-lint.py` 改为 `...-v2.py`（原串彻底消失，非后缀追加）：

```
$ bash ci/mutation_runner.sh run --root .../fakeroot2b --probe MUT-001 --scratch-dir .../b3/scratch
  [probe MUT-001] FAIL: 探针命令与 run.sh step1 漂移（run.sh 内不再包含: cargo fmt --check --all && python3 ci/k5-path-lint.py）——先修探针镜像
mutation_runner: 存在红探针——被测门未失败于删除/弱化=门失效或探针漂移（复盘 C3：mutation 不过不得合流）
run exit=1
```
正向对照：真树 run.sh 含探针命令逐字原串（`grep -qF` 命中）→ mirror 放行（B3-05 全绿即隐含证明）。

### 判据 5：should-run 触发规则 ✅

| 场景 | 输出 |
|---|---|
| git 仓 diff 触及 `ci/`（新增未跟踪门脚本） | `full` |
| git 仓 diff 不触及（只动 docs/） | `nightly-only` |
| 非 git 目录（fail-safe） | `full` |
| 真树（未提交改动含 ci/run.sh + 未跟踪 ci/*.sh） | `full` |

### 判据 6：隔离断言——演练后真树零变化 ✅

```
git status --porcelain 演练前后 diff → 零差异（仍只有本批预期改动 6 项）
真树 mutations/registry.yaml sha256 不变（46b08df4ef0b…）
真树 ci/baselines/mutations-registry.sha256 仍在
scratch 目录已自清（无残留）
```
（`cp -al` 硬链接 + scratch 内 unlink/os.replace 不伤正本 inode——实测证明。）

---

## 五、⑧.3 裁决落地：域守卫 ensure 改调 ci 级单一入口

`crates/af-order/scripts/af-order-test.sh` 新增 `guard_ensure()`：本树有 `ci/provenance_check.sh` 时 ensure 走 ci 级入口（`--root $ROOT --target $TARGET`），缺席时回退域守卫并 WARN（不静默失效）。`af-order-target-guard.sh` 本体零改动（26 断言夹具回归不受影响，见判据 7）；`record` 仍走域守卫（域证据 JSON 带 branch/test_list，与 ci 级三要素口径不同、互不覆盖）。端到端实跑证据见 §二 附加验证 B（wrapper-rc=0，域证据三要素齐）。

---

## 六、跨机制入键：numbers.json（总对账 §三.3 口径）

新增 `numbers.json`（B 批贡献件，只含 B2/B3 三键——本树 run.sh 无 step10-12，A 的 gates.* 键不可在本树复算故不占位；合流批与 A 的 numbers.json 取并集）：

| 键（终稿原文拼写） | 登记值 | source 命令（本树复跑一致） |
|---|---|---|
| `fv.kani_inv_harness.count` | 32 | `bash ci/baseline_calc.sh gen-kani \| awk -F'\t' '$1=="count"{print $2}'` |
| `fv.kani_inv_harness.manifest_sha256` | `dd67716e…c85ab` | `sha256sum ci/baselines/kani-inv-harness.manifest \| cut -d' ' -f1` |
| `mutations.probes` | 2 | `python3 -c "import json;print(len(json.load(open('mutations/registry.yaml', encoding='utf-8'))['probes']))"` |

自检：`json.tool` 可解析；三键 source 在本树复算==登记值（`ALL-SOURCE-RECOMPUTE-OK`）；mutations registry sha 现值 46b08df4ef0b… 记入 note（机读绑定在 `ci/baselines/mutations-registry.sha256`，step0 check-registry 每次对账）。

**已知 schema 冲突（合流批须裁决，勿绕过）**：A 已合流的 `ci/step12_check.py:23` `KEY_RE=^[a-z0-9_]+\.[a-z0-9_]+$` 只收**单点**键名——`fv.kani_inv_harness.count` / `.manifest_sha256` 两个双点键名会被判「非限定名」；同文件 :69-72 要求来源命令 stdout 必须 **int** 可解析——manifest_sha256 的十六进制值不满足。`mutations.probes`（单点+整数）无冲突。键名按任务书与 A 的 _note「键名拼法照终稿原文，勿改」沿用终稿拼写（A _note / SPEC-B2 ①.2 / SPEC-B3 ①.3 / 总对账 §三.3 四方钉死，改键名即制造新的跨文书漂移），冲突与建议（按终稿键名放宽 step12：KEY_RE 允许多点域路径 + value 允许十六进制串比对）已写入 numbers.json `_note` 与本报告；放宽前，sha 的机读面由 `baseline_calc.sh verify-kani`（run.sh 9b 每次实跑）与 `mutations-registry.sha256`（step0 每次实跑）承担，不失控。

---

## 七、判据汇总与已知限制

### 7.1 判据汇总（全部本机实跑，2026-10-28）

| 机制 | 判据数 | 结果 |
|---|---|---|
| B1 | 7 + 附加 2（step3 接线三场景、af-order 端到端） | 全过 |
| B2 | 6 | 全过 |
| B3 | 6 | 全过 |
| 跨机制 numbers.json | 3 键复算 | 值一致；KEY_RE/整数值 schema 冲突已登记待合流批裁决 |

实跑脚本与原始输出留痕于 `/home/pan-ding/workspace/.g0b-tmp/`（树外，git 不可见）：`b1-checks.sh`/`b1-fix-checks.sh`/`step3-sim2.sh`/`b2-checks.sh`/`b2-fix-checks.sh`/`b3-checks.sh`/`b3-07-fix.sh`/`numbers-calc.sh`/`numbers-verify.sh` 及各自 `*.txt` 输出。

### 7.2 已知限制（承接 SPEC ⑧，未擅自扩大）

1. B1 ⑧.1：env-dep 扫描为主面，.d 内字面依赖路径未逐一分类（他树盖章由守卫章补面）；⑧.2 空白路径按空白截断（承域守卫同款声明）。
2. B1 ⑧.5：step9 9b 的独立 target `~/afbuild-phase2fv-kani` 不受本门覆盖（规格明示不扩）。
3. B2 ⑧.1：kani::proof 提取按行状态机，单行多 harness/宏生成不识别；⑧.3：gen-rocq 与 run.sh 9a-2 两处提取正则须同步演化（最小 diff 纪律未合轨）；⑧.4：两正本物理位置不同（ci/baselines/ vs proofs/）。
4. B3 ⑧.1：探针库现仅 2 例（G0 合流最小集），其余随「新门上线同批带探针」纪律沉淀；⑧.3：nightly 定时挂载属运营者裁决点（不新增宿主级服务）；⑧.4：mirror 审计只证子串在场，语义等价由 MUT-001 实跑兜底。
5. B2 ⑧.2：ADVISORY-FAIL 未升硬红（维持 advisory 定位），以「结构化产物+step10 消费」闭环——运营者改一行 `rc` 接线即可升格。
6. 合流线现状（如实）：`af-wt-g0merge@53b1212` 已将 G0-M04/05/06 descope 保持 open（due=2026-10-31 复核锚）；本分支交付+本报告即翻判 done 的输入（evidence_path 计划=ci/out/step3-provenance.json、ci/out/step9-failures.json，按 A 的 deliverables.yaml ⑥-⑨ 行口径）。

### 7.3 环境事实（影响验证方式，记录在案）

- 本次会话 `/tmp` 用户配额耗尽（`os error 122 / Disk quota exceeded`）：Bash 工具的标准输出捕获与 cargo cc 临时文件均曾因此失败。验证改用「输出全部重定向到 `/home/pan-ding/workspace/.g0b-tmp/` 下文件」方式完成（工具调用本身零 stdout）；af-order cold build 以 `TMPDIR=~/af-ci-tmp`（run.sh step0 护栏同款磁盘路径）跑通。这与 run.sh 已固化的 /tmp 配额护栏史实一致，非本批代码问题。
- 首轮验证中我有 3+6+1 处**测试脚本自身 bug**（grep 模式空格、不存在的 root 目录、root/target 配对错误、stub 注入破坏词法、harness `$@` 作用域等），均已定位修正并重跑全绿——产品代码零返工；唯一一处产品侧修订=provenance_check.sh 头注活例计数 58→64（实测口径，§1.3）。

---

## 八、CI 时长增量（承接 SPEC ⑥）

- B1 常态：check 扫 .d 秒级 ≈ +2~5s、record ≈ +0.5s；污染 target 首次=一次性冷重建（正确性强制成本，本树 `~/afbuild-af-wt-g0b-af-order` 已实跑验证）。
- B2：verify-kani/verify-rocq 毫秒级，增量 <1s。
- B3：step0 check-registry <0.1s；full 演练仅 diff 触发（should-run=full 时）+10~20s。
- 合计常跑稳态 +3~6s，≤20 分钟红线无影响（与总对账 §二 时长账一致）。

---

## 九、复跑指南

```bash
cd /home/pan-ding/workspace/af-wt-g0b
# B1：bash -n + 正反例 + ensure/拒清/record + 活例只读 + 域守卫夹具
bash /home/pan-ding/workspace/.g0b-tmp/b1-checks.sh        # 结果: .g0b-tmp/b1/b1-results.txt
bash /home/pan-ding/workspace/.g0b-tmp/b1-fix-checks.sh    # 结果: .g0b-tmp/b1/b1-fix.txt
bash /home/pan-ding/workspace/.g0b-tmp/step3-sim2.sh       # step3 接线三场景
# B2
bash /home/pan-ding/workspace/.g0b-tmp/b2-checks.sh        # 结果: .g0b-tmp/b2/b2-results.txt
bash /home/pan-ding/workspace/.g0b-tmp/b2-fix-checks.sh    # 结果: .g0b-tmp/b2/b2-fix.txt
# B3
bash /home/pan-ding/workspace/.g0b-tmp/b3-checks.sh        # 结果: .g0b-tmp/b3/b3-results.txt
bash /home/pan-ding/workspace/.g0b-tmp/b3-07-fix.sh        # mirror 审计
# numbers.json 入键复算
bash /home/pan-ding/workspace/.g0b-tmp/numbers-verify.sh   # 结果: .g0b-tmp/numbers/verify.txt
# af-order 端到端（⑧.3 接线）
TMPDIR=~/af-ci-tmp bash crates/af-order/scripts/af-order-test.sh
```

*报告完 · 2026-10-28 · G0-B 施工员（af-wt-g0b/g0b）*

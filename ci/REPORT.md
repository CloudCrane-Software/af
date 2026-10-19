# REPORT.md — ci-skeleton：验收管道骨架固化

任务：把已定案的验收固化成一条可跑的管道，产出物全部在 `AF/ci/` 下。
执行日：2026-10-19。所有结论均来自本任务内真实执行的命令（命令原文与输出见③）。

---

## ① 结论

**部分通过**。管道骨架已建成并全链路真实跑通：六步门全部实现、逐步计时、产物落 `ci/out/`；其中 **5/6 门绿**（clippy 基线门 / cargo test / CUE 预编译 / scrub 解析门 / cosign 断网验签），**step1 `cargo fmt --check` 在当前树上红**（exit 1，235 个格式 diff 块、39 个文件）——这是树 的预存状态，纪律禁止我改 crate 代码（波次 1 的活），管道如实亮红并继续跑完全部步骤收集完整诊断。

"干净 checkout 可重复执行"已**实证**（非推测）：把仓库源码拷到仓库外临时目录（**不含 target/、不含 ci/out/**），全管道在该副本上跑完，六门判定与原树完全一致（fmt 同红、其余同绿；冷 clippy 95.2s、冷 test 127.3s @ -j4，含同机并发 agent 的负载）。临时副本已删除。

同一棵树上管道复跑 3 轮（其中 1 轮从 /tmp 以绝对路径调用）判定逐一相同——结果确定性成立。

| 步骤 | 门 | 结果（当前树） |
|---|---|---|
| 1 | cargo fmt --check | **红**（预存未格式化代码，235 块/39 文件） |
| 2 | clippy 基线门（29 条既有警告全在基线内） | 绿 |
| 3 | cargo test -j4 --workspace（174 过 / 0 败 / 1 忽略） | 绿 |
| 4 | CUE recipe vet + export + 结构断言 | 绿 |
| 5 | parse_scrub.py × 7 份真实日志 | 绿（判定逐一符合预期） |
| 6 | cosign 断网验签（sudo unshare -n + --trusted-root） | 绿（正例过/篡改例拒） |

## ② 步骤（做了什么）

产出物清单（全部在 `AF/ci/`）：

```
ci/
├── run.sh                 # 管道入口：预检 + 六步 + 汇总；产物写 ci/out/；exit 0/1
├── clippy_gate.py         # clippy 警告基线门：签名=(lint, 文件, 次数)，新增才 fail；--update 固化基线
├── clippy-baseline.txt    # 基线（15 签名 / 29 警告，--update 生成）
├── parse_scrub.py         # zot scrub 表格解析门：STATUS=affected → exit 1；解析失败 → exit 2
├── out/                   # 每轮产物（SUMMARY.txt、step1..6 日志、recipe-merged.json 等，可整删重跑）
└── REPORT.md              # 本报告
```

各步实现要点与决策：

- **预检（step0）**：`rustup component add rustfmt clippy`（本机已装，幂等）、工具存在性检查（cargo/python3/sudo/unshare/`bin/cosign`/`poc-cue/bin/cue`）、`cargo metadata --no-deps` 校验 workspace manifest 完整性。
- **step1 fmt**：`cargo fmt --check --all`，退出码直通硬门。任务未给 fmt 类似 clippy 的基线豁免，故不做基线——现状红即红（详见④-1）。
- **step2 clippy 基线门**：`cargo clippy --workspace --all-targets -j4 --message-format=json` → `clippy_gate.py` 比对基线。策略实现：签名 = `(lint 名, 相对路径, 出现次数)`；本次任一签名次数**超过**基线 → exit 1（新增警告）；只减不增 → 绿并提示 `--update` 收紧；行号漂移不影响判定；基线只松不紧，修好警告后人工 `--update` 收紧。已实测 cargo 在热缓存下会**回放**全部 29 条既有警告（jsonl 344 行），基线门在增量跑时不是空转。
- **step3 test**：`cargo test -j4 --workspace`，退出码硬门。
- **step4 CUE**：`poc/poc-cue/bin/cue` 对 recipe 产线样例：4a `vet recipe-default.cue + operator-override.cue` 必须过；4b `export` 落 `out/step4-recipe-merged.json`；4c Python 结构断言（L4 覆盖生效 15/0.05 + L3 默认保留 4/3/5/120/85 + 顶层键集闭合，9 值逐一核对）；4d 反例 `operator-override-bad.cue`（maxAttempts=99）必须被 vet 拒绝（非零才过）。
- **step5 scrub 门**：`parse_scrub.py` 按 ask 指定测试集逐一判定：`scrub-baseline.log`、`scrub-after-gc.log` 应 exit 0；`scrub-corrupt1/2/3a/3b-*.log`（= corrupt3b-consistent + corrupt3b-inplace）应 exit 1；任一不符 → 步骤红。退出码语义：0 全 ok / 1 检出 affected / 2 解析失败（无表头、坏行、文件不可读——**宁可误红不可漏报**，空输入、垃圾输入、未知 STATUS 都按 2 处理）。表头前的 zot debug JSON 行忽略（真实 corrupt1.log 即此形状）；表头后非空行必须是数据行。
- **step6 cosign**：在 `poc/poc-cas/verify-tr/` 内：6a `sudo unshare -n bin/cosign verify-blob --key tr-key.pub --bundle tr-bundle.json --trusted-root trusted-root.json artifact-tr.txt` 必须过（sudo 下 HOME=/root、无 `~/.sigstore` 缓存，顺带证明自包含）；6b 篡改工件 `artifact-tampered.txt` 同命令必须 exit 1（证明门真的在验）。
- **管道语义**：步骤失败**不中断**（一次跑完拿全量诊断），汇总表打 `out/SUMMARY.txt`，任一硬门红 → 总 exit 1。路径全部相对脚本定位仓库根，与调用 cwd 无关（已从 /tmp 调用实测）。

过程记录：第一版 run.sh 的步 4/5/6 内层 `bash -c` 未继承 `AF_ROOT/OUT`（首轮红，退出码/报错如实留痕），加 `export` 修复后全部绿——管道自身的一次真实调试。

## ③ 证据（命令原文与输出摘录）

### 环境与组件

```
$ source ~/.cargo/env && rustup component add rustfmt clippy
info: component rustfmt is up to date
info: component clippy is up to date          # exit=0；rustfmt 1.10.0-stable / clippy 0.1.99（rustc 1.99.1）
```

### step1 fmt（红）

```
$ cargo fmt --check; echo $?
1                                              # 完整 diff 落 ci/out/step1.log（154 KB）
$ grep -c '^Diff in' ci/out/step1.log
235                                            # 39 个文件；按 crate 分布：
45 af-calendar / 44 af-release-gate / 31 af-workorder / 25 af-stop / 19 af-sampling /
18 af-effect-gateway / 13 af-trace / 12 af-meter / 11 af-credential / 9 af-station / 8 af-graph
样例 diff（af-workorder/tests/lifecycle.rs:136）：
-    let tampered = DecisionArtifact::seal("user-approver", ..., "k-approver");
+    let tampered = DecisionArtifact::seal(
+        "user-approver", ..., "k-approver",
+    );
```

### step2 clippy 基线门（绿）

```
$ cargo clippy --workspace --all-targets -j4 --message-format=json > clippy.jsonl   # 首次采集 68.07 s
$ python3 ci/clippy_gate.py clippy.jsonl --update
clippy-gate: 基线已更新 → ci/clippy-baseline.txt（15 签名 / 29 警告）
$ python3 ci/clippy_gate.py ci/out/step2-clippy.jsonl
CLIPPY GATE: PASS —— 现有 29 条警告 / 15 签名，均在基线内（基线 15 签名）
基线 15 签名全文：
1 clippy::assertions_on_constants crates/af-sampling/src/csp1.rs
1 clippy::assign_op_pattern     crates/af-calendar/tests/dst_night_shift.rs
2 clippy::doc_lazy_continuation crates/af-station/src/lib.rs
4 clippy::drain_collect         crates/af-release-gate/src/sim.rs
1 clippy::manual_is_multiple_of crates/af-release-gate/tests/day_sim.rs
1 clippy::map_flatten           crates/af-station/src/lib.rs
2 clippy::needless_question_mark crates/af-station/src/lib.rs
2 clippy::needless_range_loop   crates/af-calendar/src/calendar.rs
3 clippy::needless_range_loop   crates/af-calendar/tests/dst_night_shift.rs
1 clippy::needless_range_loop   crates/af-calendar/tests/sla.rs
2 clippy::neg_cmp_op_on_partial_ord crates/af-release-gate/src/gate.rs
1 clippy::print_literal         crates/af-meter/examples/routing.rs
2 clippy::result_large_err      crates/af-trace/src/trace.rs
2 clippy::useless_format        crates/af-graph/src/validate.rs
4 clippy::while_let_loop        crates/af-release-gate/src/sim.rs

fail 路径实测（把基线中 drain_collect 一行删掉模拟"新增"）：
$ python3 ci/clippy_gate.py ci/out/clippy.jsonl --baseline /tmp/base-reduced.txt; echo $?
CLIPPY GATE: FAIL —— 1 个签名出现新增警告（新增 4 条）:
  +4 clippy::drain_collect crates/af-release-gate/src/sim.rs（现 4 / 基线 0）
1
输入不像 JSON 时 exit 2（拒绝拿错文件当基线输入）；基线为空时全量判新增 → FAIL（安全方向）。
```

### step3 cargo test（绿）

```
$ cargo test -j4 --workspace; echo $?          # 热缓存 5.94 s；真·干净 checkout 127.3 s
0
$ grep -E "^test result:" step3.log | awk '{ok+=$4; f+=$6} END {print ok, f, NR}'
174 0 35                                       # 35 个测试目标，174 过 / 0 败 / 1 忽略
```

### step4 CUE（绿）

```
$ poc/poc-cue/bin/cue vet poc/poc-cue/recipe/recipe-default.cue poc/poc-cue/recipe/operator-override.cue; echo $?
0
$ cue export ...（同上两文件）                   # 落 out/step4-recipe-merged.json
{ "scheduling": { "batchIntervalSec": 15, "maxConcurrentJobs": 4,
    "retry": { "maxAttempts": 3, "backoffSec": 5 } },
  "thresholds": { "cycleTimeWarnSec": 120, "defectRatioWarn": 0.05, "temperatureCritC": 85 } }
$ python3 断言（run.sh 内联）
recipe-merged.json 结构断言: 9 值全中，键集闭合
$ cue vet recipe-default.cue operator-override-bad.cue; echo $?   # 反例必须拒
scheduling.retry.maxAttempts: invalid value 99 (out of bound <=10): ...
1
```

### step5 scrub 解析门（绿）

```
$ for f in scrub-baseline scrub-after-gc scrub-corrupt1 scrub-corrupt2 scrub-corrupt3a \
         scrub-corrupt3b-consistent scrub-corrupt3b-inplace; do
    python3 ci/parse_scrub.py poc/poc-cas/verify-logs/$f.log >/dev/null 2>&1; echo "$f exit=$?"; done
scrub-baseline exit=0          # 应通过 ✓
scrub-after-gc exit=0          # 应通过 ✓
scrub-corrupt1 exit=1          # 应失败 ✓（blob not found，且日志前有 zot debug JSON 行，正确忽略）
scrub-corrupt2 exit=1          # 应失败 ✓（bad blob digest）
scrub-corrupt3a exit=1         # 应失败 ✓
scrub-corrupt3b-consistent exit=1   # 应失败 ✓（invalid server config）
scrub-corrupt3b-inplace exit=1      # 应失败 ✓
边界（均按"宁可误红"设计）：
  空输入 → exit 2；无表头垃圾 → exit 2；表头后未知 STATUS 行 → exit 2；
  多文件模式下一个 affected → exit 1；--json 机器可读输出可用。
（注：scrub-corrupt3c.log 不在 ask 指定测试集——zot scrub 对"结构合法但身份错误"的
  config 本来就输出 ok（poc-cas/VERIFY.md 已记录的盲区），解析器如实回显 exit 0。）
```

### step6 cosign 断网验签（绿）

```
$ cd poc/poc-cas/verify-tr && sudo -n true; echo $?     # 免密 sudo 可用
0
$ sudo unshare -n ../../bin/cosign verify-blob --key tr-key.pub --bundle tr-bundle.json \
      --trusted-root trusted-root.json artifact-tr.txt; echo $?
Verified OK
0                                                       # 无 --insecure-ignore-tlog，断网命名空间内过
$ sudo unshare -n ../../bin/cosign verify-blob ... artifact-tampered.txt; echo $?
Error: failed to verify signature: ... invalid signature when validating ASN.1 encoded signature
1
（另：--trusted-root bad-root.json → proto-json unmarshal 报错 exit 1，手工验证过一次。）
```

### 全管道与可重复性

```
轮次            树/条件                    fmt  clippy     test       cue   scrub  cosign
RUN1 11:10     原树, 首轮(子进程变量bug)   FAIL  PASS       PASS       FAIL  FAIL   FAIL   → 修复 export
RUN2 11:11     原树, 热缓存               FAIL  PASS 387ms PASS 5.4s  PASS  PASS   PASS
RUN3 11:12     原树, 从 /tmp 调用         FAIL  PASS 480ms PASS 5.4s  PASS  PASS   PASS
CLEAN 11:13    源码副本, 无target/无out/   FAIL  PASS 95.2s PASS 127.3s PASS PASS   PASS
判定三轮全同；总退出码 1（fmt 红所致，符合管道语义）。
```

### 环境事实（与共享仓库相关，如实记录）

- 本任务**未修改** AF 根 `Cargo.toml`、未创建/改动任何 crate、未 git commit。执行期间另一并发任务（poc-otel）曾于 11:01 把 `members` 改为含 `poc/poc-otel/otel-probe/otel-probe`（目录当时不存在，`cargo test` 全树报 `failed to load manifest` exit 101），11:0x 由该任务自行还原为 `members = ["crates/*"]`。窗口期事实已用 `cargo metadata` 预检固化进 run.sh（step0），此类损坏现在会让管道以 exit 2 + 指向日志的明确报错快速失败，而不是六步里莫名炸开。
- 常驻进程核查：本任务未启动任何 server/worker/db。核查时在跑的 `zot serve`、两个 `temporal server`、两个 temporal worker 均属并发 agent（命令行带 `poc-bridge` 等路径/参数），未触碰。

## ④ 对方案的反馈 / 决策输入

1. **fmt 红是当前最大的决策点**：定案验收把 `cargo fmt --check` 放进管道，但树上有 235 个 diff 块/39 文件，且明令不许我改 crate 代码。管道按字面实现为硬门（与 clippy 不同，任务没给 fmt 基线豁免），结果就是**当前每轮必红**。给开发形态的输入：要么波次 1 第一步先跑一遍全仓 `cargo fmt`（机械改动、零逻辑风险，一次消红），要么明示给 fmt 也配"基线豁免"策略——两者选一，管道不用改（fmt 命令与退出码语义都已就位）。红项清单在 `ci/out/step1.log`，按 crate 分布见③。
2. **clippy 基线门是"只拦新增"的可行实现**：签名取 (lint, 文件, 次数) 而非行号——对行号漂移免疫，对同文件同 lint 的增量也能查；基线文件随仓库走（本次干净 checkout 实证：冷 clippy 29 条与基线精确吻合）。给波次 1 的纪律：修掉现有 29 条后应跑 `--update` 收紧基线（工具会打印提示）；新代码引入任何 clippy 警告都进不了主干。
3. **cargo 热缓存会回放警告**（实测 344 行 JSONL/29 条），基线门在增量跑时真实有效，不需要 `cargo clean`；同时冷跑数据（95s/127s @ -j4，同机有并发负载）可作 CI 机器的预算参考。
4. **scrub 门解析的是表格而非退出码**（scrub 检出损坏也 exit 0，poc-cas/VERIFY.md ④-2），`parse_scrub.py` 把它补成了真门；对"解析失败"按 exit 2 误红处理是刻意取向——完整性门宁可误报不可漏报。`scrub-corrupt3c`（合法但身份错）是 scrub 的固有盲区，**不属于管道能拦的损坏类别**，那类要靠 step6 的签名/验签手段补位——两步在管道里正好互补。
5. **cosign 断网步骤完全自包含**：sudo 下 HOME=/root（无任何 TUF 缓存）仍 Verified OK，验证机不需要网络也不需要预置缓存，trusted-root.json + 公钥两个小文件随仓分发即可。管道顺带验了篡改负例，防"验证命令坏了永远 exit 0"的门失效模式。运维前提（继承 poc-cas 结论）：sigstore 轮换密钥时 trusted-root.json 要重刷重发。
6. **共享仓库需要预检**：本次亲历 workspace manifest 被并发任务改坏（cargo 全树不可用数分钟）。`run.sh` step0 的 `cargo metadata` 预检把这类环境损坏与代码问题区分开（exit 2 vs exit 1）。建议所有在 AF 根跑 cargo 的任务都带这一步。

## ⑤ 遗留

- **fmt 红待波次 1 消**：`cargo fmt`（写模式）全仓跑一遍即可，管道侧无需改动；红项明细在 `ci/out/step1.log`。
- **冷跑计时含并发负载**（95s/127s 是同机 16 核上有其它 agent 构建时测得），空载 CI 机会更快；未做隔离基准。
- `rustup component add` 在真·无网新机上的首次安装路径未测（本机组件已就位，管道里该步幂等）。
- parse_scrub.py 目前针对 `zot scrub` **表格模式**；在线 scrub 的 warn 日志模式（`"blobs/manifest affected" image=...`）未纳入解析器（当前管道测试集均为表格日志）。
- clippy 基线是"只松不紧"策略：文件删除/大改导致的基线残留要靠人工 `--update` 收紧，未做自动收紧提议机制。
- cosign 步骤用的是 blob 验签（与 poc 范围一致）；registry 内镜像签名（`cosign verify` + zot referrer）离线化仍是 poc-cas 遗留，未进本管道。
- 执行期间观察到的根 Cargo.toml 并发改动事件已记录于③，未做跨任务协调机制（超出本任务范围）。

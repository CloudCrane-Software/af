# REPORT — 分层 cargo 反馈流水线（poc-cargo-pipeline）

日期：2026-10-19。环境：rustc/cargo 1.99.1（stable，58cf8f9c3 2026-10-13）+ nightly-x86_64-unknown-linux-gnu（miri/fuzz 用）、16 核、`-j4`、`TMPDIR=/home/pan-ding/tmp`（/tmp 是 tmpfs，配额见 ③.0）。
所有结论均来自本机真实执行：命令原文与输出摘录在 ③ 节。凡未实测的，在 ⑤ 节点名。

---

## ① 结论

**裁决：引入。** 九个候选全部真实装、真实跑通，没有一个是"纸面工具"。落成四层阶梯：

| 层 | 时机 | 工具与命令 | 预期时长（本机实测） |
|---|---|---|---|
| **L0 每轮写码** | agent 每次改完码 | `cargo check -p <改动crate>` | 秒级（demo 增量 0.18s；AF workspace 增量 0.98s） |
| **L1 提交前** | 每个 commit/PR 前 | `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test -p <crate>` | 十秒级（AF clippy 2.5s；demo 全测试增量 0.12s） |
| **L2 合流/CI** | PR 合流 | `cargo test --workspace` + `cargo semver-checks check-release --baseline-root main -Z unstable-options` + `cargo deny check` + `cargo audit --json` + criterion bench + fuzz 短档（`-max_total_time=60`） | 分钟级（semver-checks 小 crate 0.55s、真 crate 冷启 ~1-2min；deny 2.75s；audit 0.9s；bench ~10s/项） |
| **L3 周期/夜间** | 定时 | 长 fuzz（30min/target）+ `cargo +nightly miri test` + advisory-db 刷新 + deny/audit 全量 + criterion 历史对比 | 分钟~小时级 |

三条核心取舍（全部有实测支撑）：

1. **L0 只放 `cargo check`，不放 clippy/test**：check 是纯类型检查无代码生成，demo 上 0.18s、AF 上 0.98s；而带 dev-deps 的首次 `cargo test` 冷编译 24.4s、clippy 冷启 16.2s。agent 每轮都跑得起、愿意跑的只有 check。clippy 上移到 L1（增量同样 0.18s，但首次贵）。
2. **miri/fuzz 必须沉到 L2/L3，且只能覆盖纯 Rust 路径**：miri 首跑 80s（sysroot 初始化）；它的增量价值在原生 debug 抓不到的 UB——实测 Rust 1.99 的 unsafe 前置条件检查已经能在原生测试里抓住 slice 越界（`get_unchecked_mut` 越界→非展开 panic→SIGABRT），而 `transmute::<u8,bool>(2)` 这种非法值原生静默通过、miri 精确报错到行。AF 的 rusqlite(bundled) crate 带 C 代码，预计无法整体 miri（未实测，见 ⑤）。
3. **cargo-semver-checks 的用法是"minor/patch 回归防线"，不是 major 发布器**：实测 0.51.0 在 major  bump 下把全部 260 个 lint 判 unnecessary 直接跳过；在"声称 minor 实际混入破坏"的场景一次抓出 6 条 major 违例（删函数/改签名/字段转私有/枚举加变体），每条带官方 semver 参考链接和 file:line。AF 的公开 API 面（af-workorder 门控类型、af-graph schema）正是它的目标。

---

## ② 步骤

1. 建 `AF/poc/poc-cargo-pipeline/`，`cargo new demo --lib`；发现 cargo new 把 demo 自动挂进 AF 根 Cargo.toml 的 members（根文件被改）→ `git checkout -- Cargo.toml` 还原 + demo/Cargo.toml 加空 `[workspace]` 表脱钩。
2. `rustup component add clippy rustfmt`；`rustup toolchain install nightly --profile minimal` + `rustup component add miri --toolchain nightly`（验证 `cargo +nightly miri --version` → miri 0.1.0）。
3. `cargo install -j4 cargo-fuzz cargo-insta cargo-semver-checks cargo-deny cargo-audit`：第一批 /tmp 配额爆掉 3 个（cargo install 的临时构建目录在 /tmp）→ `TMPDIR=/home/pan-ding/tmp` 后第二批装齐。
4. demo 工程（`AF/poc/poc-cargo-pipeline/demo/`，Cargo.toml 带空 `[workspace]` 表）：
   - `src/lib.rs`：clamp_pct / apply_markup_cents（纯函数，proptest 主角）+ oob_write（UB 例子 A）+ invalid_bool（UB 例子 B）+ parse_order（fuzz 主角）；UB 测试标 `#[ignore]`。
   - `tests/proptest_props.rs`：2 个属性（clamp 值域+恒等；加价不 panic/不减/单调）。
   - `tests/insta_snap.rs` + `tests/snapshots/insta_snap__clamp_pct_table.snap`：1 个快照。
   - `benches/bench_markup.rs`（`harness = false`）：1 个 criterion 基准。
   - `fuzz/`（`cargo fuzz init` 生成，手动加空 `[workspace]` 表、bin 条目改 parse_order、corpus 种子 "1999:13"）：1 个 libFuzzer target。
5. 逐个实跑并计时：check / clippy（文本+JSON+-D warnings 门）/ fmt / test / proptest（含故意失败看收缩）/ insta（新建+失配两种形态）/ criterion（默认+压参两档）/ fuzz run / miri（两个 UB 例子+原生对照）。
6. semver-checks：`semver-demo/` 下手写基线对（semdemo 0.1.0→0.2.0 含 4 处破坏），另做两组对照实验定位"0 checks 全 skip"的原因（见 ③.8）。
7. cargo-deny：写 `deny.toml`（licenses/bans/advisories/sources 四类），踩了两个 0.20 版配置格式坑后跑通；advisory-db 从 GitHub 克隆到 `/home/pan-ding/advisory-db` 后用 `db-path` 离线全量。cargo-audit 用 `--db ... --no-fetch` 跑通（含 `--json`）。
8. AF 仓库实测规模耗时：`CARGO_TARGET_DIR` 指到任务目录内（遵守"只在任务目录写文件"）+ `--locked`（保证不动 AF 的 Cargo.lock），跑 workspace 级 check/clippy；跑完 `git status` 确认 AF 无改动。
9. 无常驻进程需要清理（本任务未起 server/worker/db；`ps` 确认无我启动的进程）。

---

## ③ 证据

### ③.0 环境与两个基建坑（影响接入清单）

/tmp 配额爆掉 cargo install：
```
$ cargo install -j4 cargo-fuzz cargo-insta cargo-semver-checks cargo-deny cargo-audit
error: failed to write /tmp/cargo-install0xlBMS/release/deps/rustc6wcETU/lib.rmeta: Disk quota exceeded (os error 122)
   Summary Successfully installed cargo-fuzz, cargo-insta! Failed to install cargo-semver-checks, cargo-deny, cargo-audit
EXIT=101
```
→ `TMPDIR=/home/pan-ding/tmp` 后第二批成功（975s，757 crates）：
```
   Installed package `cargo-audit v0.22.2` (executable `cargo-audit`)
   Summary Successfully installed cargo-semver-checks, cargo-deny, cargo-audit!
EXIT=0
```
装成版本：cargo-fuzz 0.13.2 / cargo-insta 1.49.0 / cargo-semver-checks 0.51.0 / cargo-deny 0.20.2 / cargo-audit 0.22.2；clippy/rustfmt 为 stable 内置 component；miri 0.1.0（b6eeb93c99 2026-10-18，nightly）。

cargo new 自动改 AF 根 Cargo.toml（已还原，git status 干净）：
```
$ cargo new demo --lib
    Adding `demo` as member of workspace at `/home/pan-ding/workspace/临时工作区/temp2/af`
$ cd af && git status --short
 M Cargo.toml            # members 被加上 poc/poc-cargo-pipeline/demo
$ git checkout -- Cargo.toml && head -4 Cargo.toml
[workspace]
resolver = "2"
members = ["crates/*"]     # 已还原
```
且空 `[workspace]` 表是硬需求——拿掉后 cargo metadata 直接报错（semver-checks 实测触发）：
```
error: current package believes it's in a workspace when it's not:
current:   .../semver-demo/current/Cargo.toml
workspace: .../af/Cargo.toml
```

### ③.1 cargo check / clippy / fmt（L0/L1 主力）

demo（lib 无依赖）：
```
$ time cargo check -j4          → Finished in 1.72s   real 1.77s
$ time cargo clippy -j4 --all-targets   （冷，含 dev-deps 编译 16.2s；语法错误那次也照编了依赖）
$ time cargo clippy -j4 --all-targets   → Finished in 0.18s   real 0.246s   （增量）
$ cargo fmt --check             → 无输出（干净）
```
clippy 失败形态（人类文本）与机器形态（JSON）：
```
warning: clamp-like pattern without using clamp function
 --> tests/lint_demo.rs:3:5
  help: replace with clamp: `x.clamp(0, 100)`
warning: `demo` (test "lint_demo") generated 3 warnings (run `cargo clippy --fix --test "lint_demo" -p demo -- ` to apply 1 suggestion)
```
```
$ cargo clippy -j4 --all-targets --message-format=json | grep '"level":"warning"' | head -1
{"reason":"compiler-message","message":{"rendered":"warning: clamp-like pattern ... help: replace with clamp: `x.clamp(0, 100)` ...","spans":[...]}}
```
→ 每行一个 JSON 对象：`reason=compiler-message`、`message.message`（短消息）、`message.spans[]`（file_name/line_start/column_start/suggested_replacement）、并有 `cargo clippy --fix` 一键应用建议。**机器可解析+可自动修复，两者都成立。**

`-D warnings` 门（L1 门禁形态）：
```
$ cargo clippy -j4 --all-targets -- -D warnings
error: transmute from a `u8` to a `bool`
   = note: `-D clippy::transmute-int-to-bool` implied by `-D warnings`
```
→ 这条正是我故意留的 UB demo B：**clippy 静态 lint 先拦、miri 运行时后抓，双层防御**。加 `#[allow(clippy::transmute_int_to_bool)]` 后门通过（GATE-PASS）。

AF workspace 真实规模（11 crate，各 2-5 个依赖，CARGO_TARGET_DIR 指到任务目录、--locked）：
```
$ time cargo check -j4 --locked        → Finished in 1m 44s   real 1m45s   （冷，全依赖编译）
$ time cargo check -j4 --locked        → real 0.978s                        （增量）
$ time cargo clippy -j4 --locked --all-targets → Finished in 2.40s  real 2.478s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
```

### ③.2 cargo test / proptest / insta（L1）

全量测试（冷编译 24.37s 含 proptest/insta/criterion 全部 dev-deps）：
```
$ time cargo test -j4   （UB 测试已 #[ignore]）
running 3 tests: clamp_basic ok; miri_invalid_bool_value ignored; miri_ub_write_oob ignored
test result: ok. 1 passed; 0 failed; 2 ignored
test clamp_pct_snapshot ... ok            （insta）
test clamp_pct_stays_in_range ... ok      （proptest）
test markup_monotonic_and_saturating ... ok
$ time cargo test -j4   → real 0.118s     （增量全绿）
```
libtest 的 `--format json` **稳定版被拒**，nightly 才可以（这决定 L1 的机器可读路线）：
```
$ cargo test --lib -- --format json
error: The "json" format is only accepted on the nightly compiler with -Z unstable-options
$ cargo +nightly test --lib -- --format json -Zunstable-options
{ "type": "suite", "event": "started", "test_count": 3 }
{ "type": "test", "event": "started", "name": "tests::clamp_basic" }
```

proptest（1.12.0）：默认每属性 256 例，0.01s。**抓到真实 bug**——我 demo 函数里 `(u128 中间值) as u64` 是截断不是饱和：
```
thread 'markup_monotonic_and_saturating' panicked at tests/proptest_props.rs:4:1:
Test failed: assertion failed: out >= lower at tests/proptest_props.rs:20.
minimal failing input: base = 7718302959711109463, pct = 239
	successes: 147
```
→ 最小反例直接给出（base=7718302959711109463, pct=239 时 u128 中间值越过 u64::MAX 被截断）。修复（`.min(u64::MAX as u128) as u64`）后全绿。失败时还会落 `.proptest-regressions` 文件固化种子。**输出形态：最小反例行 + 回归种子文件，机器可解析。**

insta（1.49.0 + cargo-insta 1.49.0）。首次无快照 → 生成 `.snap.new` 并在终端给结构化 diff：
```
Expression: results
+new results
────────────┬───────────────────────────────────────────────────────────────────
          1 │+[
          2 │+    0,
...
To update snapshots run `cargo insta review` or set `INSTA_UPDATE=always`
```
接受（`INSTA_UPDATE=always cargo test --test insta_snap`）后绿。人为改坏快照值再跑，失配 diff（行号对齐 old/new）：
```
-old snapshot
+new results
────────────┬───────────────────────────────────────────────────────────────────
    6       │-    51,
          6 │+    50,
```
`cargo insta test` 子命令亦可用（"no snapshots to review"）。**输出形态：终端 diff + .snap.new 待接受文件；对 agent 的价值是"改输出=显式接受或显式失败"，不会静默漂移。**

### ③.3 criterion（0.5.1，bench 性能面）

```
$ time cargo bench -j4
Gnuplot not found, using plotters backend
Benchmarking apply_markup_cents/1999+13: Warming up for 3.0000 s
Benchmarking apply_markup_cents/1999+13: Collecting 100 samples in estimated 5.0000 s (1.4B iterations)
apply_markup_cents/1999+13
                        time:   [3.4435 ns 3.4990 ns 3.5618 ns]
Found 7 outliers among 100 measurements (7.00%)
real 0m40.776s          （30.5s 冷编译 bench profile + ~10s 测量）
```
压参快档（0.5.1 没有 `--quick`，用原生参数；且必须指定 `--bench` 否则 libtest 二进制会因不认识参数报错）：
```
$ time cargo bench -j4 --bench bench_markup -- --warm-up-time 0.5 --measurement-time 1 --sample-size 10
apply_markup_cents/1999+13
                        time:   [3.2518 ns 3.3446 ns 3.4891 ns]
                        change: [-3.6774% -0.9863% +1.9744%] (p = 0.72 > 0.05)
                        No change in performance detected.
real 0m2.0s
```
**输出形态：time: [lo, median, hi] + 与上次的 change% + p 值判定（"No change in performance detected"）——性能回归判定是现成的，machine-parsable 程度中（文本表格），适合 L2 而非 agent 每轮。**

### ③.4 cargo-fuzz（0.13.2）

`cargo fuzz init` 生成 fuzz/；两个坑：① fuzz 构建脚本调默认 stable cargo 报 "1 nightly option were parsed" → 必须 `cargo +nightly fuzz ...`；② 默认 target 文件删了但 Cargo.toml 的 `[[bin]]` 条目还在 → 改条目。构建 ~21s（sanitizer+lib）。抓崩：
```
$ cargo +nightly fuzz run parse_order -- -max_total_time=60
...
SUMMARY: libFuzzer: deadly signal
0x31,0x39,0x39,0x3a,0x31,0x39,0x32,      → "199:192"（pct=192>100 触发 assert）
artifact_prefix='.../fuzz/artifacts/parse_order/'; Test unit written to .../crash-862e821d740199a13f74c9cebd4f301dc8e7d86f
Base64: MTk5OjE5Mg==
Failing input: fuzz/artifacts/parse_order/crash-862e821d740199a13f74c9cebd4f301dc8e7d86f
Reproduce with: cargo fuzz run parse_order fuzz/artifacts/parse_order/crash-...
Minimize test case with: cargo fuzz tmin parse_order fuzz/artifacts/...
Error: Fuzz target exited with exit status: 77
```
计时重跑（删 artifact 后）：同 1 秒内复现抓崩（`#16 INITED cov: 68` → `#85 REDUCE` → crash，exec/s 都来不及显示）。**输出形态：artifact 文件（hex/base64 输入）+ 复现命令 + 最小化命令——对 agent 极友好：一个路径+一条命令即可重放。简单 panic 秒级必中。**

### ③.5 miri（nightly component）

首跑 80s（sysroot 初始化+编译）：
```
$ time cargo +nightly miri test --lib
test tests::clamp_basic ... ok
miri green secs: 80
```
UB 例子 A（slice 越界，`get_unchecked_mut(idx=4)`）：
```
$ cargo +nightly miri test --lib -- --ignored miri_ub_write_oob
error: abnormal termination: the program aborted execution
$ cargo test --lib -- --ignored miri_ub_write_oob        （原生对照）
thread 'tests::miri_ub_write_oob' panicked at src/lib.rs:19:14:
unsafe precondition(s) violated: slice::get_unchecked_mut requires that the index is within the slice
This indicates a bug in the program. This Undefined Behavior check is optional...
thread caused non-unwinding panic. aborting.   （signal: 6, SIGABRT）
```
**关键实测：这一类 UB 原生 debug 测试就已经抓住（Rust 1.99 unsafe 前置条件检查）。**

UB 例子 B（非法 bool 值）——miri 的独占价值：
```
$ cargo +nightly miri test --lib -- --ignored miri_invalid_bool_value
test tests::miri_invalid_bool_value ... error: Undefined Behavior: constructing invalid value of type bool: encountered 0x02, but expected a boolean
  --> src/lib.rs:32:14
32 |     unsafe { std::mem::transmute::<u8, bool>(b) }
   = help: this indicates a bug in the program: it performed an invalid operation, and caused Undefined Behavior
   = note: stack backtrace:
           0: invalid_bool at src/lib.rs:32:14
$ cargo test --lib -- --ignored miri_invalid_bool_value   （原生对照）
test tests::miri_invalid_bool_value ... ok        ← 静默通过！
```
**输出形态：`Undefined Behavior: <类别>` + `--> file:line:col` + 调用栈——结构化程度高，可直接转成 agent 的结构化错误。**

### ③.6 cargo-semver-checks（0.51.0）

本地两版本对比（semdemo 1.1.0 基线 vs 1.2.0 当前，当前含 4 处破坏）：
```
$ cargo semver-checks check-release -Z unstable-options --baseline-root ../baseline
--- failure enum_variant_added: variant Level:Mid in .../current/src/lib.rs:16
--- failure function_missing: pub fn removed or renamed
Failed in: function semdemo::clamp_pct, previously in file .../baseline/src/lib.rs:2
--- failure function_parameter_count_changed: semdemo::apply_markup now takes 2 parameters instead of 1, in .../current/src/lib.rs:2
--- failure struct_pub_field_missing: field max of struct Config, previously in .../baseline/src/lib.rs:11
--- failure struct_pub_field_now_doc_hidden: field Config.max in .../current/src/lib.rs:10
     Summary semver requires new major version: 6 major and 0 minor checks failed
real 0m0.546s
```
每条带：lint 名 + 人话描述 + 官方 semver 参考 URL + 精确 file:line。**机器可解析（failure 块+Failed in 行），且每条自带修复指引链接。**

两组对照实验（解释"0 checks 全 skip"谜团）：
- proptest 1.12.0（本地）vs 1.11.0（registry 基线）：`202 checks: 202 pass, 58 skip`，冷 99s（82s 是 rustdoc 构建）；
- proptest 1.12.0（本地）vs 1.11.0（`--baseline-root` 本地副本）：同样 `202 checks: 202 pass, 58 skip`，32.4s；
- semdemo 1.1.0→1.2.0 **纯兼容新增**：`202 checks: 202 pass, 58 skip`；
- semdemo 0.1.0→0.2.0 / 1.1.0→2.0.0（major）：`0 checks: 0 pass, 260 skip`（"Starting 0 checks, 260 unnecessary"）。

→ 结论：**major 版破坏合法，0.51.0 把全部 lint 判 unnecessary 跳过；工具的正确战场是 minor/patch 里混入破坏**（AF 的 PR 基本都是 minor，正合适）。另外 `--baseline-root` 本身工作正常（本地/registry 基线皆可），配置格式上 `-Z unstable-options` 必须显式传。

### ③.7 cargo-deny（0.20.2）/ cargo-audit（0.22.2）

deny.toml 两个 0.20 版坑：`vulnerability = "deny"` 键已被删（PR 611），`severity-threshold` 同样被删；`unmaintained` 是范围枚举不是级别：
```
error[deprecated]: this key has been removed, see https://github.com/EmbarkStudios/cargo-deny/pull/611
error[unexpected-value]: expected '["all", "workspace", "transitive", "none"]'
```
advisories 要拉 GitHub 的 RustSec/advisory-db——**首次直接失败（拥塞），重试路线如下**：
```
$ cargo deny check
error: failed to fetch advisory database https://github.com/RustSec/advisory-db with cli: failed to clone
fatal: 无法访问 'https://github.com/RustSec/advisory-db/'：Failed to connect to github.com port 443 after 136316 ms
$ cargo audit
error: couldn't fetch advisory database: git operation failed: ... error sending request for url (.../info/refs?service=git-upload-pack)
```
`git ls-remote` 重试第 1 次即通 → 浅克隆 2 秒拿库 → 之后全部离线可跑：
```
$ git clone --depth=1 https://github.com/RustSec/advisory-db.git /home/pan-ding/advisory-db   （11M，2s）
$ cargo audit --db /home/pan-ding/advisory-db --no-fetch
      Loaded 1338 security advisories (from /home/pan-ding/advisory-db)
    Scanning Cargo.lock for vulnerabilities (107 crate dependencies)
AUDIT-EXIT=0                      （0 漏洞，0.9s）
$ cargo audit --db ... --no-fetch --json
{"database":{"advisory-count":1338,...},"lockfile":{"dependency-count":107},...,"vulnerabilities":{"found":false,"count":0,"list":[]},"warnings":{}}
$ cargo deny check                  （deny.toml 里 [advisories] db-path = "/home/pan-ding/advisory-db"）
advisories ok, bans ok, licenses ok, sources ok        real 2.749s
```
licenses 失败→修复样例（**每个 crate 都必须有 license 字段**，否则 deny 直接 FAILED）：
```
error[unlicensed]: demo = 0.1.0 is unlicensed        （Cargo.toml 补 license = "MIT" 后：licenses ok，0.35s）
```
**输出形态：deny 分类 ok/FAILED + error[code]+crate=version+原因；audit 稳定版即有 --json。都可机器解析。**

---

## ④ 对方案的反馈/决策输入

### ④.1 四层阶梯（agent 开发模式专用）

**L0 — 写码中每轮（秒级，agent 每轮必跑）**
- 命令：`cargo check -p <改动crate>`（只动到的 crate，不全 workspace）
- 实测：demo 0.18s / AF 增量 0.98s
- 反馈格式：`--message-format=json` 流式，每行一个诊断对象；agent 侧统一 envelope：`{tool:"check", level, file, line, col, message, replacement}`，`replacement` 直接来自 `spans[].suggested_replacement`
- 理由：check 无代码生成，是全链条里唯一"每轮都跑得起"的；clippy 增量同样 0.18s 但冷启 16s，放 L1

**L1 — 提交前（十秒级）**
- 命令：`cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo test -p <crate>`
- 实测：AF clippy 2.5s；demo 全测试增量 0.12s（冷 24.4s 含 dev-deps， CI 缓存可消化）
- 反馈格式：clippy JSON 诊断（同上）+ libtest 文本行（`test X ... FAILED` + panic 首行；要全结构化就 `+nightly ... -- --format json -Zunstable-options`）+ insta `.snap.new` diff + proptest `minimal failing input:` 行
- 门禁语义：fmt/clippy 是硬门；test 失败必须带最小反例或 diff，不允许裸 panic

**L2 — 合流/CI（分钟级，全量+慢档）**
- `cargo test --workspace -j4`（全量）
- `cargo semver-checks check-release -Z unstable-options --baseline-root <main 检出>`——仅对 minor/patch PR 开（major PR 它全 skip，别浪费）
- `cargo deny check` + `cargo audit --db <vendored> --no-fetch --json`（advisory-db 本地化，GitHub 拥塞时重试 3 次）
- criterion 全量 bench（`--warm-up-time 0.5 --measurement-time 1 --sample-size 10` 压到 ~2s/项；发布版再跑默认 10s/项）
- fuzz 短档：每个 target `cargo +nightly fuzz run <t> -- -max_total_time=60`（简单 panic 秒级必中，1 分钟档是性价比甜点）
- 反馈格式：semver-checks 每条违例（lint+描述+官方 URL+file:line）；deny 分类汇总；audit JSON `vulnerabilities.list[]`；fuzz artifact 路径+复现命令

**L3 — 周期/夜间（分钟~小时级）**
- 长 fuzz：`-max_total_time=1800` + 定期 `cargo fuzz cmin` 压缩语料库
- `cargo +nightly miri test --lib`（对纯 Rust 路径；UB 演示测试 `#[ignore]` 显式触发）
- advisory-db `git pull` 刷新 + deny/audit 全量
- criterion 默认档全量 + 历史 change% 对比（性能回归台账）

### ④.2 给 agent 的结构化错误协议（跨工具统一）

所有工具的失败都能归一成五字段，agent 不需要认九种格式：
```
{ tool, level, code, location:{file,line,col}, message, fix:{kind:"replacement"|"command"|"seed"|"artifact", payload} }
```
- clippy/check：fix=replacement（来自 suggested_replacement，可 `--fix` 自动应用）
- test(panic)：fix=null，location=panic 首行
- proptest：fix=seed（最小反例，写进 .proptest-regressions）
- insta：fix=command（`INSTA_UPDATE=always` 或 cargo insta accept，须显式）
- fuzz：fix=artifact（artifact 文件路径 + `cargo fuzz run <t> <path>` 重放）
- miri：fix=null，code=UB 类别（如 constructing invalid value）
- semver-checks：fix=官方 semver URL（人读后决策）
- deny/audit：fix=null 或 ignore 条目

### ④.3 AF 接入清单（只列清单，不实施）

1. **`rust-toolchain.toml`**（AF 根）：pin stable 通道；fuzz/miri 显式 `+nightly`（本机 nightly 已装，但 CI 需声明）。
2. **`[workspace.lints]`**（AF 根 Cargo.toml）+ 各 crate `[lints] workspace = true`：把 L1 的 `-D warnings` 门从"每人自觉"变成"编译期强制"。
3. **`deny.toml`**（AF 根）：四类检查全开；licenses allow 列按 AF 实际依赖树收敛（demo 实测：未列到的 license 只警告，缺 license 字段才 FAILED——所以重点是**每个 crate 的 Cargo.toml 补 `license` 字段**）。
4. **vendored advisory-db**：`git clone --depth=1` 到 `AF/ci/advisory-db`，deny 用 `db-path`、audit 用 `--db --no-fetch`；刷新脚本带 3 次重试（本机实测首次克隆失败、重试即通）。
5. **`fuzz/` 目录**（按 crate 价值排序）：af-workorder（门控状态机/订单解析）、af-effect-gateway（幂等键/载荷指纹）、af-sampling（不变式引擎）、af-graph（schema 解析）——每个 `cargo fuzz init` 后补空 `[workspace]` 表；target 命名 `<crate>_<入口>`；corpus 入库、artifacts 忽略。
6. **`benches/` + criterion dev-dep**：只给热点 crate（af-meter/af-sampling/af-trace/af-release-gate 的每件/每事件路径）；冷门 crate 不背 bench 编译成本。
7. **proptest dev-dep**：纯函数/不变式密集处——af-calendar（SLA 计算）、af-meter（窗口计量）、af-sampling（三不变式）、af-stop（止损规则优先级）、af-workorder（门控谓词）。本次实测它在 demo 里抓到一个真 bug（截断），对 AF 的"整数最小单位/钳制"类代码是刚需。
8. **insta dev-dep**：输出型面——af-graph 的 `afg validate` 违规清单、af-workorder 的决策工件序列化、af-credential 的策略求交结果。
9. **unsafe/UB 测试规范**：UB 演示/边界测试一律 `#[ignore]`，miri 显式跑（否则原生 SIGABRT 会炸掉整个测试二进制，本次实测）；禁止 UB 测试进常规绿跑。
10. **CI 四层脚本**：L0 进 agent 运行时（每轮 check）；L1 进 pre-push hook 或 agent 提交动作；L2 进 PR CI；L3 进夜间 cron。semver-checks 仅在 minor/patch PR 启用。
11. **future-incompat 处置**：AF 现状 `proc-macro-error2 v2.0.1` 触发 future-incompat 警告，排期升级（否则未来某版 Rust 直接拒编）。
12. **`TMPDIR` 注意**：CI/开发机若 /tmp 是小 tmpfs，`cargo install` 会 Disk quota exceeded（os error 122）——构建机把 TMPDIR 指到大盘。

---

## ⑤ 遗留

1. **miri 与 rusqlite(bundled) 的兼容性未实测**：af-effect-gateway/af-station 带 C 编译的 libsqlite3-sys，miri 原理上不执行 C 代码，预计只能对"纯 Rust 单元"跑 miri（把 SQLite 相关测试隔离后）。**未验证，接入前需小规模实测。**
2. **AF 的 clippy 2.5s 是 11 个"已填依赖但代码量小"的 crate 测得**；真实代码量上来后 L1 时长会涨，需在首批真实 PR 上复测。
3. **semver-checks 冷启成本依赖 crate 大小**：proptest 级 82s（rustdoc 构建主导）；AF 各 crate 小，预计秒级，但**未在 AF 上实测**（只在 semver-demo/proptest 对上测过）。另外 `--baseline-root` 需要 CI 能拿到基线检出（git worktree/archive）。
4. **advisory-db 本地化是绕开 GitHub 拥塞的工程方案**，但库内容会过期；夜间 pull 失败时的降级策略（用旧库报警 vs 跳过）需要产品决策。
5. **criterion 的机器可读输出弱于其他工具**（文本表格），`--message-format` 类 JSON 输出未在 0.5.1 找到替代；性能回归判定目前靠文本 change% 行解析，接入时可接受或自研薄封装。
6. **未测项**：`cargo nextest`（未列入候选故未装）、cargo-insta 的 review 交互流（只验证了 accept/UPDATE 路径）、`cargo fuzz coverage`（未测）、nightly 版本漂移对 miri/fuzz 的影响（ nightly 是滚动频道，CI 需 pin 日期版）。
7. **demo 里两个 UB 例子是故意构造的**，不是 AF 真实代码缺陷；proptest 抓到的截断 bug 是我 demo 函数自身的（已修复），仅证明工具能力，不构成对 AF 代码的任何结论。

---

## 附：任务目录结构

```
AF/poc/poc-cargo-pipeline/
├── REPORT.md                  ← 本报告
├── demo/                      ← 独立 cargo 工程（Cargo.toml 带空 [workspace] 表）
│   ├── src/lib.rs             （clamp_pct/apply_markup_cents/oob_write/invalid_bool + parse_order 模块）
│   ├── tests/proptest_props.rs / insta_snap.rs + snapshots/
│   ├── benches/bench_markup.rs（criterion, harness=false）
│   ├── fuzz/                  （cargo-fuzz，target=parse_order，corpus 种子，空 [workspace] 表）
│   ├── deny.toml
│   └── target/                （构建产物）
├── semver-demo/               ← semver-checks 对比对（baseline/current/proptest-111-baseline 等）
└── logs/                      ← 各安装/运行的起止时间戳与日志
```

AF 根 Cargo.toml 未改动（`git status` 干净，仅新增未跟踪目录 poc/poc-cargo-pipeline/）。

# RESULT-UPDATE.md — R4 af-sampling proptest 属性测试

## ① 结论

**通过**。未改 CSP-1 / 金标 / 升迁状态机的已验证语义（双水位、三不变式保持原样）。`proptest 1.12.0` 作为 dev-dependency 引入。新增 3 组属性测试，`cargo test -p af-sampling` **26 全绿**（原 22 单元 + 1 端到端，另加 3 个 proptest；0 失败）。`cargo clippy --all-targets -p af-sampling -- -D warnings` 无警告。

测试数 23 → 26：只新增测试，没有删改原 23 个用例的断言语义。唯一生产测试改动是 `sim_1000` 里 `assert!(F >= F_MIN)` 换成运行期 `effective_rate() >= F_MIN`，为了消掉 `clippy::assertions_on_constants`；仍是“f 不低于 f_min”。

## ② 步骤

1. 读完 `crates/af-sampling/RESULT.md` 与 `csp1` / `golden` / `promotion` / `rng` / `tests/end_to_end.rs`。三不变式按原文取为：转抽检必有 ≥ i 连续已检合格、回全检必为抽中缺陷、有效抽检率不低于 f_min。双水位按原文取为：`current` 不超过 `attained`，熔断只降 `current`，连净回升不超过 `attained`，冲新谱系只走判据窗口。
2. `cargo add proptest --dev -p af-sampling`，锁定 crates.io 当时最新稳定版 **1.12.0**。理由写在提交 `925fc98`。
3. 修 `src/csp1.rs` 的常量断言，否则 `-D warnings` 在既有代码上就失败。
4. 新增 `tests/properties.rs` 三组属性（各 256 例，失败持久化关闭，反例印在测试输出里）：
   - `three_invariants_hold_for_any_injection_sequence`：任意合格/缺陷位序列 + 任意 `set_rate`（含越界钳制）+ 任意种子 + 开/关。
   - `dual_watermark_moves_only_on_threshold_crossing`：任意合法配方 + 任意 `RunOutcome` 序列。硬失败才 `Demoted`；`current < attained` 且连净跨过 `clean_to_recover` 才 `Recovered`（优先于判据，窗口不清）；窗口积满且三判据成立且还有上一级才 `Promoted`。`attained` 只升不降，单步最多动一级。
   - `golden_injection_does_not_change_lineage_counts`：同一配方与同一 `RunOutcome` 序列，旁边按任意 p / defect_share / 种子注入金标并过 CSP 与台账，谱系签名（current、attained、clean_streak、动作列、agent/hybrid/deterministic 停留次数）与不注入的回放一致。事件数等于运行条数，不加上注入件数。
5. `cargo fmt -p af-sampling`、clippy、`cargo test -p af-sampling` 后提交 `0f253ab`。

## ③ 证据

```
$ cargo search proptest --limit 1
proptest = "1.12.0"

$ cargo add proptest --dev -p af-sampling
Adding proptest v1.12.0 to dev-dependencies

$ cargo clippy --all-targets -p af-sampling -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)
（无 warning；修常量断言之前会在 csp1.rs:394 assertions_on_constants 失败）

$ cargo test -p af-sampling
running 22 tests ... test result: ok. 22 passed; 0 failed
running 1 test  ... test result: ok. 1 passed; 0 failed
running 3 tests
test three_invariants_hold_for_any_injection_sequence ... ok
test dual_watermark_moves_only_on_threshold_crossing ... ok
test golden_injection_does_not_change_lineage_counts ... ok
test result: ok. 3 passed; 0 failed
Doc-tests af_sampling ... test result: ok. 0 passed
```

合计 26 passed / 0 failed。属性测试三次运行都在约 0.03s 内结束（每组 256 例，序列短，无 sleep）。

## ④ 反馈

1. 判据窗口里的 `min_successes` 在当前实现下几乎是恒真：硬失败在入窗前就熔断返回，窗里全是成功，积满时成功数等于 `window`，而校验又要求 `min_successes <= window`。真正会拒晋升的是 `max_violations` 和 `min_consistency`。属性测试仍按三条一起判，以免以后把失败记录放进窗口时把这条判据悄悄丢掉。
2. 同一拍上“连净回升”优先于“判据晋升”，而且回升不清窗口。这是已验证行为（端到端 batch13 回升后窗口继续积）。属性测试把这个优先级钉死了；改成“回升顺手清窗口”会直接红。
3. 金标注入与谱系计数没有共享状态。谱系只在 `Engine::record` 上动。注入率、缺陷占比、CSP 是否因此多看了金标件，都不进入谱系计数。批次口径仍然必须用台账增量折成 `RunOutcome` 再喂引擎（RESULT.md 已记录的坑），属性测试不把台账自动接进引擎。
4. `f_min` 托底靠构造拒绝 + `set_rate` 钳制，属性测试对越界请求（0 和 >1）也检查钳制结果。经验抽检率的伯努利波动仍只留在原来的长跑单测里，没有写进属性测试，避免换种子就抖。

## ⑤ 遗留

1. 256 例是抽样，不是穷尽证明。反例可读（`ItemSeq` 打成 `C`/`D` 串，配方与 `S`/`F` 结果、谱系签名都带在失败信息里），但默认不写 `proptest-regressions` 文件。
2. `proptest` 写在本 crate 的 `[dev-dependencies]`，没有收进根 `[workspace.dependencies]`。任务改动范围只允许 `crates/af-sampling/**`；锁文件 `Cargo.lock` 随 `cargo add` 更新了（提交 `925fc98`），否则版本锁不住。
3. 升迁窗口仍是“积满即评估并清空”的周期制，不是滑动窗口。属性测试锁定的是这个已验证语义。
4. 金标仍然只在真实件之前入队。属性测试没有覆盖乱序或队列拥塞。
5. 旧 RESULT.md 写“clippy 未安装”。本环境 clippy 0.1.99 可用，本 crate 现已通过 `-D warnings`。

---

# RESULT-UPDATE — C4：af-sampling criterion 基准（波次 1 ⑦/T9-6 后半）

## ① 结论

**通过**。`af-sampling` 增加 criterion 0.8.2 基准两条：抽检相位上的 `Csp1::process`（抽样判定），以及按外部配方从 Agent 升到 Hybrid 再升到 Deterministic 的 `Engine::record`（三谱系升迁）。`cargo test -p af-sampling` 全绿，26 个测试（22 单元 + 1 端到端 + 3 个 proptest），数量与上一节 R4 记录相同，本任务没有增删测试。`cargo bench -p af-sampling --bench sampling_paths` 退出码 0。`cargo clippy --all-targets -p af-sampling -- -D warnings` 无警告。

任务开始时 `sim_1000` 里仍是 `assert!(F >= F_MIN)`。`-D warnings` 会在这条常量断言上失败（`ci/clippy-baseline.txt` 记着这一条）。断言改成 `m.effective_rate() >= F_MIN`，仍然是“有效抽检率不低于 f_min”。状态机没改。

## ② 步骤

- `Cargo.toml`：在已有的 `proptest = "=1.12.0"` 旁边加上 `criterion = "0.8.2"`，与 `af-trace` 同一版本。`[[bench]] name = "sampling_paths"`，`harness = false`，没有 `test = true`。
- 工作区 `Cargo.lock`：在 `af-sampling` 的依赖列表里加上已有的 `criterion`。没有新的第三方版本。根 `Cargo.toml` 未改。`ci/clippy-baseline.txt` 未改（不在本任务目录里）。
- `src/csp1.rs`：只改 `sim_1000` 的那条常量断言，换成运行期 `effective_rate()`。经验抽检率断言保持原样。
- `benches/sampling_paths.rs` 两条基准。不读墙钟，不用 sleep。随机源是种子 2026 的 xorshift。计时不含状态机构造。每组 10 个样本，预热 1 秒，测量 3 秒。
  - `sampling_decision/csp1_process_sampling_phase`：`i = 1`，`f = 0.2`，`f_min = 0.05`。窗外用一件合格品进入 `Sampling`。计时 10_000 次 `process(true)`。合格品不会回全检，整批停在伯努利判定上。窗外断言相位仍是 `Sampling`，检验件数等于同一夹具的探针，并且探针件数落在 `(0, 10_000)` 里（抽中与放行两条分支都走到）。`effective_rate() >= f_min`。
  - `lineage_promotion/agent_hybrid_deterministic`：配方 JSON 与升迁单测相同（窗口 5、最少成功 5、最大违规 0、一致性 0.98、连净 3）。每轮克隆 256 个引擎，每个引擎记 10 次干净 `RunOutcome`。计时只包住 `record`。窗外断言每次都是第 5 条 `Promoted(Hybrid)`、第 10 条 `Promoted(Deterministic)`，其余 `Hold`，并且 `current == attained == Deterministic`。
- 未改 CSP-1 / 金标 / 升迁的转移规则。双水位和三不变式保持 `RESULT.md` 与上一节 R4 的已验证语义。

## ③ 证据

命令均在 `source /data/af-factory/env.sh` 之后执行。

`cargo clippy --all-targets -p af-sampling -- -D warnings`：`Finished dev profile [unoptimized + debuginfo] target(s) in 2.19s`，退出码 0，无本 crate 警告。改断言之前，同一命令在 `csp1.rs` 的 `assert!(F >= F_MIN)` 上以 `clippy::assertions_on_constants` 失败。

`cargo test -p af-sampling`（输出里没有 `Running benches/`）：

```
running 22 tests
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 3 tests
test three_invariants_hold_for_any_injection_sequence ... ok
test dual_watermark_moves_only_on_threshold_crossing ... ok
test golden_injection_does_not_change_lineage_counts ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
Doc-tests af_sampling
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo bench -p af-sampling --bench sampling_paths`：`Finished bench profile [optimized] target(s) in 28.49s`，退出码 0。`Gnuplot not found, using plotters backend`。基准内部断言若失败会直接 panic，这两条都跑完了。

控制台 `time` 三元组是 criterion 的 typical（有斜率时用斜率）95% 区间。均值和标准差取 `target/criterion/<id>/new/estimates.json` 的 `mean` / `std_dev` 点估计。

```
sampling_decision/csp1_process_sampling_phase
                        time:   [44.062 µs 44.596 µs 46.020 µs]
                        thrpt:  [217.30 Melem/s 224.24 Melem/s 226.96 Melem/s]
均值 45.77 µs，标准差 3.05 µs（95% 均值区间 [44.20 µs, 47.79 µs]）
10 个样本里 2 个 high severe 离群点
Collecting 10 samples in estimated 3.0020 s (68k iterations)

lineage_promotion/agent_hybrid_deterministic
                        time:   [66.920 µs 68.253 µs 72.139 µs]
                        thrpt:  [35.487 Melem/s 37.508 Melem/s 38.255 Melem/s]
均值 77.03 µs，标准差 14.65 µs（95% 均值区间 [69.29 µs, 86.29 µs]）
Collecting 10 samples in estimated 3.0008 s (32k iterations)
```

抽样判定每件约 4.6 ns（10_000 次 `process`）。升迁每条 `record` 约 30 ns（256 × 10 条）。升迁均值高于斜率点估计（68.25 µs），标准差约是均值的五分之一，10 个样本里尾部比较重。

## ④ 反馈

- 抽样热点按“已在 `Sampling`、输入全是合格品”来测。抽中缺陷会回 `Screening`，那是另一条已验证路径，不在这条基准的计时里。`f = 0.2` 且有 `f_min` 托底，探针要求检验件数既不是 0 也不是整批，用来确认伯努利分支真的发生。
- 升迁基准用的是外部 JSON 配方，引擎里没有内置阈值。连净回升条件要求 `current < attained`；这条爬升上两者一直相等，所以动作只有 `Hold` 和两次 `Promoted`。熔断降级没有放进计时。
- `record` 会把事件推进 `events`。基准测的是这个公开路径，包括事件日志的增长。
- 环境没有 gnuplot。criterion 改用 plotters，统计结果仍然打印，基准退出码为 0。

## ⑤ 遗留

- 每组 10 个样本。升迁的标准差偏大，只适合看数量级，不能当成稳定的性能回归线。
- `ci/clippy-baseline.txt` 仍列着 `assertions_on_constants` / `csp1.rs` 这一条。源码里这条已经没了。基线文件不在本任务改动范围内，没有收紧。门禁对“比基线更少”是放行的。
- 金标注入、队列乱序、滑动窗口都没有进基准。与 `RESULT.md` ⑤ 和上一节 R4 相同。

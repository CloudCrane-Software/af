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

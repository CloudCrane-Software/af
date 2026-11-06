# RESULT-UPDATE · 任务 B1 workspace.lints

## 结论

通过。根 `Cargo.toml` 增加 `[workspace.lints.rust]` 与 `[workspace.lints.clippy]`：`unsafe_code = "forbid"`，`unused_*` / `dead_code` 及同类保守项停在 `warn`。11 个 crate 末尾都有 `[lints] workspace = true`。依赖声明未改，未引入第三方 crate。

`cargo check --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 均为退出码 0。测试 **216 passed + 1 ignored**，与基线 216+1 一致，没有新增或删除测试。

## 步骤

1. 读各 crate 的 RESULT.md 与现有 clippy 基线。仓库内没有 `unsafe` 关键字。
2. 根清单写入起步集。rust 侧点名 warn 级的 `dead_code`、`unused_*`、不可达代码、命名风格、`deprecated`。clippy 侧只收默认组里的 `let_unit_value`、`no_effect`、`unnecessary_operation`、`unused_unit`。不设 `pedantic`、`nursery`、`restriction`、`cargo`。
3. 11 个成员 crate 的 `Cargo.toml` 末尾加 `[lints] workspace = true`。
4. `-D warnings` 打到既有 clippy 告警。按 clippy 建议做了最小改动，判定与公开类型保持原样：
   - `af-calendar`：`weekday_single_shift` 的下标循环改为迭代器。
   - `af-station`：文档行首的 `+` 改成“加上”，避免被当成 Markdown 列表；`Ok(self.get()?)` 改为直接返回；`map` + `flatten` 改为 `filter_map`。
   - `af-release-gate`：下限比较改为 `partial_cmp != Some(Less)`，NaN 仍拒绝；两处 `drain(..).collect()` 改为 `mem::take` + `append`；两处 `loop { let Some else break }` 改为 `while let`；`k % 2 == 0` 改为 `is_multiple_of(2)`。
   - `af-sampling`：`assert!(F >= F_MIN)` 挪进 `const` 块，仍检查同一对常量。
   - `af-trace`：`write_critical` 上 `allow(clippy::result_large_err)`。错误仍按值带回 `TraceEvent`，没有装箱。
5. `cargo fmt --all` 后复跑 clippy 与全量测试。

## 证据

命令都先 `source /data/af-factory/env.sh`。工具链 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`。

```text
$ cargo check --workspace -j 4
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.63s
    （exit 0。cargo 另打印既有 future-incompat：proc-macro-error2 v2.0.1，不导致失败）

$ cargo clippy --workspace --all-targets -j 4 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.41s
    （exit 0，无 warning/error 行）

$ cargo fmt --all -- --check
    （exit 0）

$ cargo test --workspace -j 4 -- --test-threads=8
    Finished `test` profile ... in 8.02s
    各 target 的 test result 均为 ok，合计 216 passed、0 failed、1 ignored
    ignored 仍是 af-calendar 的 dense_hourly_scan_3y_berlin_and_new_york
```

216 的构成（passed）：calendar 11+18，credential 4+4，effect-gateway 2+7，graph 8+21，meter 15+5+1，release-gate 1+14，sampling 22+1+3，station 8，stop 11+6，trace 4+1，workorder 42+3+3，doctest af-stop 1。

## 反馈

`[lints] workspace = true` 让这套级别在每次 `cargo check` / `cargo clippy` 都生效，不再靠人记得加参数。`unsafe_code` 是唯一的编译失败项。其余起步集按任务停在 `warn`：普通 `cargo check` 遇到 `unused` 仍是警告、退出码 0；要把它变成失败，需要把级别改成 `deny`，或继续在门外加 `-D warnings`。这次验收走的是后者，并且已经全绿。

`!(lower < upper)` 不能改成 `lower >= upper`。NaN 比较全是 false，前者拒绝 NaN，后者会放行。`partial_cmp != Some(Ordering::Less)` 与原条件一致。

`result_large_err` 的根源是 `TraceError` 按值携带 `TraceEvent`。这是 fail-closed 退还事件的契约，装箱会改公开类型，所以只加了 `allow`。

## 遗留

- `proc-macro-error2` 的 future-incompat 提示来自传递依赖（af-credential 的 cedar 等），本次不许改依赖，未处理。它不让 `-D warnings` 失败。
- `ci/clippy-baseline.txt` 仍记录已消失的告警。该文件不在本任务改动范围内，没有重跑 `clippy_gate.py --update`。基线门是“只拦新增”，次数下降仍绿；收紧基线要另做。
- 起步集没有把 `clippy::all` 写进清单。`cargo clippy` 默认仍检查 `clippy::all`。清单里的四条 clippy lint 只是把 unused/无效果一类点名出来，避免打开 pedantic。

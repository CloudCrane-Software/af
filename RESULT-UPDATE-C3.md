# RESULT-UPDATE — C3 cargo fuzz 骨架（af-sampling + af-graph）

## ① 结论

**通过。** `fuzz/` 是独立工作区（空 `[workspace]` 表），两个 target 各跑 2000 次、`--sanitizer none`、无 panic。`cargo test --workspace` **216 passed / 0 failed / 1 ignored**（ignored 是既有的 `af-calendar` 慢扫描，本次未改任何 crate 源码）。fuzz 包自身 **6 passed / 0 failed**。

## ② 步骤

1. 读 `crates/af-sampling/RESULT.md`、`RESULT-UPDATE.md` 与 `crates/af-graph/RESULT.md`、`RESULT-UPDATE-T8.md`。升迁入口仍是 `Engine::from_recipe_json` + `record`；图模板入口仍是 `serde_json` → `GraphTemplate` 再 `validate`（与 `afg validate` 相同）。未改这两处已验证语义。
2. `cargo fuzz init --fuzzing-workspace=true -t af_sampling_invariant_input`。生成物把依赖写成 metadata 第一个包 `af-calendar`、`path = ".."`（根清单是虚拟工作区，不是包）。改为包名 `af-fuzz`、空 `[workspace]`、去掉错误依赖。`libfuzzer-sys` 钉 **0.4.13**（cargo-fuzz 0.13.2 模板要求 0.4 线，crates.io 当前最新稳定）。
3. `af_sampling_invariant_input`：字节 → 钳制后的配方 JSON + `RunOutcome` 序列 → `from_recipe_json` / `record`。`window` 钳在 1..=32，违规数用 u16。不钳的话 `Vec::with_capacity(window)` 会在 `u32::MAX` 上分配失败，冒烟会被当成崩溃。最小合法种子：`window=1` 的一条干净观测，晋升到 Hybrid。
4. `cargo fuzz add af_graph_schema_parse`。字节 → `serde_json::from_slice::<GraphTemplate>`，规模内再 `validate`。单作用域节点超过 48 只做解析：环检测是递归 DFS，已验证语义按几十个节点设计。直接依赖 `serde_json = 1.0.154`（与根 workspace 锁同版本），不是新的第三方 crate。
5. 种子与冒烟产出的 corpus 入库。`fuzz/.gitignore` 去掉默认的 `corpus` 忽略，保留 `artifacts` / `coverage` / `target`。
6. 质量门：`cargo fmt`、`cargo clippy --all-targets --manifest-path fuzz/Cargo.toml -- -D warnings`、`cargo test --manifest-path fuzz/Cargo.toml`。未改 crate 的 `Cargo.toml`（path 依赖已能编过）。

## ③ 证据

```
$ cargo fuzz init --fuzzing-workspace=true -t af_sampling_invariant_input
（生成 name = "af-calendar-fuzz"、path = ".."、[workspace] members = ["."]；随后改为空表）

$ cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
Finished `dev` profile ...   # 零警告

$ cargo test --manifest-path fuzz/Cargo.toml
running 6 tests
test graph::tests::garbage_does_not_panic ... ok
test graph::tests::min_legal_seed_parses_and_validates ... ok
test graph::tests::orphan_seed_is_unreachable_node ... ok
test graph::tests::oversized_template_stops_after_parse ... ok
test sampling::tests::min_legal_seed_promotes_one_level ... ok
test sampling::tests::short_input_does_not_panic ... ok
test result: ok. 6 passed; 0 failed
```

fuzz 包测试 0 → 6，全是新增（种子回放与拒绝路径）。根 workspace 测试数不变。

```
$ cargo fuzz run --sanitizer none af_sampling_invariant_input -- -runs=2000
Finished `release` profile ... in 1m 05s
INFO:        1 files found in fuzz/corpus/af_sampling_invariant_input
#2000	DONE   cov: 136 ft: 206 corp: 24/348b lim: 22 exec/s: 0 rss: 29Mb
Done 2000 runs in 0 second(s)
exit=0

$ cargo fuzz run --sanitizer none af_graph_schema_parse -- -runs=2000
INFO:        2 files found in fuzz/corpus/af_graph_schema_parse
#2000	DONE   cov: 546 ft: 804 corp: 94/6761b lim: 121 exec/s: 0 rss: 28Mb
Done 2000 runs in 0 second(s)
exit=0
```

两次都有 `WARNING: Failed to find function "__sanitizer_acquire_crash_state"`（以及 print_stack_trace / set_death_callback）。`--sanitizer none` 不链接 sanitizer runtime，这三条是警告，进程以 0 退出，`fuzz/artifacts/` 下 **0 个文件**。

```
$ cargo test --workspace --offline
EXIT:0
合计 passed=216 failed=0 ignored=1
ignored: dense_hourly_scan_3y_berlin_and_new_york（af-calendar 既有慢扫描，本次未动）
```

根 `Cargo.toml` / `Cargo.lock` 无 diff。`cargo-fuzz` 二进制在 glibc 2.38 上打印 `GLIBC_2.39 weak version not found`，命令本身仍执行成功。本机没有 clang，`libfuzzer-sys` 0.4.13 用系统 `c++`（g++ 12.3）编过 libFuzzer。

## ④ 反馈

1. **虚拟工作区上 `cargo fuzz init` 会绑错包。** `cargo metadata` 的第一个包是 `af-calendar`，模板写 `path = ".."`。根清单没有 `[package]`，这个依赖编不过。独立 fuzz 工作区必须手改路径，并改成空 `[workspace]`（模板给的是 `members = ["."]`）。
2. **`Engine::from_recipe_json` 对合法但巨大的 `window` 会在 `Vec::with_capacity` 上失败。** 校验只要求 `window >= 1`，不设上限。不可信配方若直接进这个入口，冒烟/线上都可能被分配失败打成崩溃。harness 把 window 钳在 1..=32 后调用。建议引擎侧也封顶，或把容量改成按实际观测增长。
3. **`validate` 的环检测是递归 DFS**，注释写明按几十个节点设计。harness 对单作用域超过 48 个节点的输入只做 schema 解析。这不是把 panic 吞掉：超限输入根本不进 `validate`。
4. **`--sanitizer none` 在 stable 1.99.1 上能跑通**，不需要 nightly，也不需要 `-Zbuild-std`。缺的 sanitizer 符号只是警告。要抓内存错误仍得上 address sanitizer。
5. fuzz 包不在根 `members = ["crates/*"]` 里。`cargo test --workspace` **不会**编、也不会跑这 6 个测试。CI 若要守住 harness，需要单独 `--manifest-path fuzz/Cargo.toml`。

## ⑤ 遗留

- 冒烟是每个 target 2000 次、无 sanitizer。不是长时间战役，也没有 ASan/MSan。
- corpus 共 118 个：sampling 24（含 `min_recipe_one_event`）、graph 94（含 `min_legal.json` 与 `orphan.json`）。其余是这次 libFuzzer 留下的输入，换种子会变。
- 升迁引擎的 `with_capacity(window)` 与图校验的递归 DFS 仍在库里。本次改动范围不允许改这两个 crate 的源码，只在 harness 侧躲开。
- 图 target 不调用 CEL。`cel-parser` 对尾部运算符的 panic 仍只由 `Predicate` 的 `catch_unwind` 兜底（见 af-graph RESULT-UPDATE-T8），不在本骨架路径上。
- `cargo-fuzz` 0.13.2 的 glibc 弱符号警告未处理；当前能跑。

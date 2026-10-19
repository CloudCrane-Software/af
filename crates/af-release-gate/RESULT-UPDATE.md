# RESULT-UPDATE — W2C af-release-gate 持久化（波次 2 kernelrun）

## ① 结论

**通过**。CONWIP 在制队列与渐放游标落在 rusqlite（bundled）单文件库里。`open` 首启返回 `Created`，重入返回 `AlreadyDeclared`。schema 版本或 WIP 上下限、夜间窗口与声明不一致时启动失败，并且不改已存声明。重启后在制件数、权重、渐放游标原样可读，后续准入仍守住 `wip + weight > upper`（精确等号可入）。

已验证语义没有改：策略顺序仍是 canary → 夜间 → WIP；窗内 WipCap 仍在扣信用之后拒绝；渐放 `open` 只在谓词通过时从 gated 到全量，`released` 在晋升后冻结；拒绝不把工单写入在制。

`cargo test -p af-release-gate` **36 全绿**（原 18 + 新增 18；0 失败）。`cargo clippy --all-targets -p af-release-gate -- -D warnings` 无警告。

测试数 18 → 36：原 1 个日模拟、14 个单元、3 个 proptest 仍在，断言没改。新增 `tests/persist.rs` 17 个，以及 proptest `conwip_does_not_over_release_after_restart` 1 个。

## ② 步骤

1. 读完 `RESULT.md`、`RESULT-UPDATE.md`（B4）、`src/gate.rs`、`tests/properties.rs`。保持的结论：WIP 判定是 `wip + weight > upper`；拒绝不改 WIP、在制件数、渐放计数；夜间初始信用 1.0，WipCap 发生在信用已扣之后。
2. `Cargo.toml` 增加 `rusqlite = { workspace = true }`（锁里已有 0.32.1，feature `bundled`）。没有新的第三方 crate。`Cargo.lock` 只在本 crate 的依赖列表加了 `rusqlite` 一行。
3. `src/clock.rs`：`Clock::wall_ms` 注入 Unix 毫秒。准入、完成、夜间窗口、渐放仍只看调用方传入的 `MinuteOfDay`。墙钟只写入 `updated_wall_ms`。
4. `src/store.rs` 的 `DurableGate`：
   - 配方：WAL、`busy_timeout=5000ms`、`synchronous=NORMAL`、`foreign_keys=ON`。
   - 表：`gate_meta`（schema 版本与声明、在制合计、夜间信用、断粮边沿）、`wip_item`（队列，外键指向 meta）、`canary`（游标）。`PRAGMA user_version` 与行内 `schema_version` 都是 1。
   - 空库且没有别的用户表 → 建表并写入声明，返回 `Created`。已有 `gate_meta` → 核对版本和 `wip_upper_minutes` / `wip_lower_minutes` / 夜间窗口起止与比例的位型，一致则装回队列和游标，返回 `AlreadyDeclared`。不一致、库损坏（在制合计对不上条目权重和、`released > max_probes`、已晋升但 `completed == 0`）、或库里是别的表 → 返回错误，不写声明。
   - 谓词是函数，不落库。库里有渐放行却没传入谓词，或没有渐放行却传入谓词，都拒绝打开。
   - `try_admit` / `complete` / `begin_canary` 走原来的 `ReleaseGate`，状态变了才在一个事务里整表重写运行态。声明列不在更新语句里。
5. 同一批提交带上 `tests/persist.rs` 的 17 个重启与 fail-closed 用例。提交 `088e9d9`。
6. `tests/properties.rs` 追加 `conwip_does_not_over_release_after_restart`：256 例，失败不写回归文件。随机序列在随机一步关库再打开，之后每一步的 WIP、在制条目、渐放游标、夜间信用、事件与一份没有重启的内存闸相同，并且 WIP 不超过上限。提交 `9c208c5`。

## ③ 证据

工具链：`source /data/af-factory/env.sh` 之后为 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1 (5f94df478 2026-08-27)`。命令都带 `--offline`。

```
$ cargo clippy --all-targets -p af-release-gate --offline -- -D warnings
    Checking af-release-gate v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.35s
```

```
$ cargo test -p af-release-gate --offline

running 0 tests
test result: ok. 0 passed

running 1 test
test day_traffic_respects_caps_and_never_starves ... ok
test result: ok. 1 passed

running 14 tests
test result: ok. 14 passed

running 17 tests
test result: ok. 17 passed

running 4 tests
test canary_batch_is_monotonic_and_never_reverts ... ok
test rejection_does_not_partially_commit ... ok
test conwip_capacity_never_over_releases ... ok
test conwip_does_not_over_release_after_restart ... ok
test result: ok. 4 passed; 0 failed; finished in 0.97s

Doc-tests af_release_gate
test result: ok. 0 passed
```

合计 36 passed / 0 failed。重启属性测试单独跑时 0.82s（256 例，无 sleep）。没有写出 `proptest-regressions/`。

提交：

- `088e9d9` af-release-gate: 把 CONWIP 在制与渐放游标落 SQLite
- `9c208c5` af-release-gate: 补重启后 CONWIP 不超发的 proptest

## ④ 反馈

1. 渐放谓词无法落库。重入时由调用方按库里是否已有渐放行决定传不传。传错直接启动失败，避免用一个空谓词把 gated 批次悄悄判成别的结果。
2. 任务点名的是在制计数和渐放游标。账本同时写下夜间信用、上一次窗口观察和断粮边沿。这三样决定下一次 `try_admit` / `complete` 会不会放行、会不会补发 `NightWindowExit` 或 `RefillSatisfied`。不写的话，重启会把信用重置成 1.0，窗内可能多放一件。
3. 事件不回放。重启后的 `drain_events` 是空的；边沿只在下一次真实状态变化时再发出。
4. 窗内超重件仍先扣夜间信用再因 WipCap 拒绝，重启后信用还在、工单不在队列里。这是 RESULT 反馈 4 的原行为，持久化把它保留下来。
5. `seed_wip` 仍不衡上限，也没有接到 `DurableGate`。日模拟继续用内存闸。
6. 在制的权威值是每件权重与运行中的 WIP。重入时二者的位型必须一致，否则当损坏拒绝打开，避免把 WIP 读成 0 再超发。现有用例的权重都是整分钟，加减不丢精度。

## ⑤ 遗留

- RESULT.md 里的遗留仍在：策略顺序硬编码、`admit_blocking` 用固定 `now` 重试、夜间暂存归还顺序、模拟未建模审阅人异质性、评估报告是累计口径。本任务不改这些。
- `SharedGate` 的阻塞准入仍是进程内锁，不走账本。持久化入口是 `DurableGate` 的非阻塞 `try_admit`。
- 落库失败时内存闸已经前进，调用方应丢掉这个实例再 `open`。进程在事务提交前崩溃，库停在上一份快照。
- schema 只有版本 1。版本不符不会迁移。
- 根 `README.md` 仍写着 15 个测试，不在本任务改动范围。
- 256 例是抽样。反例会打印缩到最小的 `Case` 和切断点。默认不写回归文件。

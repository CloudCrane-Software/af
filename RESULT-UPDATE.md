# RESULT-UPDATE — T2 根 Cargo.toml 收编 [workspace.dependencies]

## ① 结论

**完成（thiserror 主版本按锁定结果分开保留）。** 公共依赖已收进根 `[workspace.dependencies]`，版本取 `Cargo.lock` 当前解析，没有升级也没有降级。9 个 crate 的对齐依赖改为 `{ workspace = true }`。`cargo check --workspace --locked` 通过；`cargo test --workspace` **174 passed / 0 failed / 1 ignored**，与基线 174+1 一致。`Cargo.lock` 无差异。clippy 基线门通过（29 条既有警告全部落在基线内，零新增）。

`thiserror` 的直接依赖同时锁定 **1.0.69** 与 **2.0.21**。workspace 的键名就是 Rust 包名，不能在键 `thiserror` 下同时继承两个主版本；把三份 1.0.69 收成 2.0.21 是升级，把五份 2.0.21 收成 1.0.69 是降级。因此根上只登记 2.0.21，1.0.69 仍写在 `af-effect-gateway`、`af-meter`、`af-station`。

## ② 步骤

1. 读完 11 个 crate 的 `RESULT.md`，盘点直接依赖与 `Cargo.lock` 解析版本。
2. 根 `Cargo.toml` 增加 `[workspace.dependencies]`（提交 `69f0005`）。
3. 版本一致的公共依赖改为 `{ workspace = true }`（提交 `056cb33`）。
4. `cargo check --workspace --locked --offline -j 4`、`cargo fmt --all`、clippy 基线门、`cargo test --workspace --locked --offline -j 4`。

盘点（直接依赖）：

| 依赖 | 解析版本 | 直接使用方 | 处理 |
|---|---|---|---|
| serde（feature `derive`） | 1.0.229 | calendar、effect-gateway、graph、meter、release-gate、sampling、trace、workorder | 根上合并 feature，crate 用 `workspace = true` |
| serde_json | 1.0.154 | 同上 8 个 | `workspace = true` |
| rusqlite（feature `bundled`） | 0.32.1 | effect-gateway、station | 根上合并 feature，crate 用 `workspace = true` |
| thiserror | 2.0.21 | calendar、release-gate、sampling、trace、workorder | `workspace = true` |
| thiserror | 1.0.69 | effect-gateway（`"1"`）、meter（`"1"`）、station（`"1.0"`） | 留在各 crate，要求串未改 |
| chrono（feature `serde`） | 0.4.45 | 仅 calendar | 留在本 crate |
| chrono-tz（feature `serde`） | 0.10.4 | 仅 calendar | 留在本 crate |
| sha2 | 0.10.9 | 仅 effect-gateway | 留在本 crate |
| anyhow | 1.0.104 | 仅 credential | 留在本 crate |
| biscuit-auth | 6.0.0 | 仅 credential | 留在本 crate |
| cedar-policy | 4.14.0 | 仅 credential | 留在本 crate |

`af-stop` 无外部依赖，`Cargo.toml` 未改。`af-credential` 的依赖都是单 crate 独有，`Cargo.toml` 未改。

feature 等价：改前所有 serde 直接依赖都开 `derive`，所有 rusqlite 直接依赖都开 `bundled`，这两项写在根上。chrono / chrono-tz 的 `serde` feature 仍在 af-calendar 本地声明。

## ③ 证据

环境：先 `source /data/af-factory/env.sh`（cargo 1.99.1 / rustc 1.99.1）。随后把 `CARGO_TARGET_DIR` 指到 `/data/af-factory/i3/target`。`env.sh` 默认的 `/data/af-factory/target` 被同机其他实例共用，并行 `cargo` 会抢同一目录。

`cargo metadata --no-deps --offline` 继承结果（节选）：

```text
af-calendar: serde ^1.0.229 feats=[derive], serde_json ^1.0.154, thiserror ^2.0.21
             chrono ^0.4 feats=[serde], chrono-tz ^0.10 feats=[serde]
af-effect-gateway: rusqlite ^0.32.1 feats=[bundled], serde ^1.0.229 feats=[derive],
                   serde_json ^1.0.154, thiserror ^1, sha2 ^0.10
af-station: rusqlite ^0.32.1 feats=[bundled], thiserror ^1.0
af-release-gate: serde ^1.0.229 feats=[derive], serde_json ^1.0.154, thiserror ^2.0.21
```

```text
$ cargo check --workspace --locked --offline -j 4
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 48.79s
EXIT:0
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
```

`proc-macro-error2` 的 future-incompat 是改前就有的传递依赖警告（af-credential / af-release-gate 的 RESULT.md 已记录），本次没有新增直接依赖。

```text
$ cargo fmt --all -- --check
FMT_EXIT:0
$ cargo fmt --all
# git status 只有 9 个 crate 的 Cargo.toml，没有 .rs 改动
```

```text
$ cargo clippy --workspace --all-targets --locked --offline -j 4 --message-format=json
CLIPPY_JSON_EXIT:0
$ python3 ci/clippy_gate.py /tmp/i3-clippy.jsonl --root /data/af-factory/i3
CLIPPY GATE: PASS —— 现有 29 条警告 / 15 签名，均在基线内（基线 15 签名）
GATE_EXIT:0
```

```text
$ cargo clippy --workspace --all-targets --locked --offline -j 4 -- -D warnings
DENY_EXIT:101
error: could not compile `af-graph` (lib) due to 1 previous error
error: could not compile `af-release-gate` (lib) due to 5 previous errors
```

`-D warnings` 被基线里已有的 lint 挡住（首批落在 `af-graph` 的 `useless_format` 与 `af-release-gate` 的 `while_let_loop` 等）。这些文件不在本任务改动范围，没有改 Rust 源码去清掉。零新增以基线门为准。

```text
$ cargo test --workspace --locked --offline -j 4
TEST_EXIT:0
passed=174 failed=0 ignored=1
```

分 crate（与改前 RESULT 计数一致）：calendar 18+11（1 ignored）、credential 4+4、effect-gateway 6、graph 16、meter 10+5、release-gate 1+14、sampling 22+1、station 8、stop 11+1 doctest、trace 4+1、workorder 34+3。未新增测试。

```text
$ git diff --stat -- Cargo.lock
# 空。--locked 编译与测试均未改写锁文件。
```

## ④ 反馈

1. 根表适合钉「已经对齐的那一个解析版本」。serde / serde_json / rusqlite 改前要求串就一致（`"1"` / `"0.32"`），收编只是把锁定版本写死到 `1.0.229` / `1.0.154` / `0.32.1`，锁文件因此可以不动。
2. 两个主版本的同名 crate 无法用一个 `workspace = true` 键同时继承：成员表的键就是 `use` 的包名，`package` 覆盖在 `workspace = true` 时会被 cargo 忽略（本机 cargo 1.99 实测：`unused manifest key: dependencies.thiserror.package`，实际仍解析到键 `thiserror` 的版本）。要保留 `use thiserror::Error` 就不能把 1.0.69 另起一个键。
3. `cargo clippy -- -D warnings` 与仓库的 clippy 基线门不是同一条标准。基线门（`ci/clippy_gate.py` + `ci/clippy-baseline.txt`）量的是新增；`-D warnings` 会把 29 条既有警告当成失败。只改 `Cargo.toml` 的任务过不了 `-D warnings`，除非去改各 crate 的源码。

## ⑤ 遗留

- **thiserror 1.0.69 与 2.0.21 仍然并存。** 直接依赖：1.0.69 在 af-effect-gateway、af-meter、af-station；2.0.21 在另外 5 个 crate，并已收进根表。传递依赖里 biscuit-auth / biscuit-parser 继续锁定 1.0.69，cedar-policy 继续锁定 2.0.21。合成一个主版本要单独拍板（升 3 个 crate 或降 5 个 crate），并重跑那一侧的测试；本次按「不升级不降级」没有做。
- `-D warnings` 全 workspace 仍是红的，签名与 `ci/clippy-baseline.txt` 一致。清掉它们要改对应 `.rs`，超出本任务改动范围。

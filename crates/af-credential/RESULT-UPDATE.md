# RESULT-UPDATE.md — C1 insta 快照接入 af-credential

## ① 结论

**通过**。`crates/af-credential` 接入 insta 1.49.0（`yaml`）做 `assert_yaml_snapshot!`。T3 词表语义没有改：`Cents` 仍是 `i64` 美分，`budget_cents` / `spend_cap_cents` / `spend_cents` 同词同型；衰减仍只加 check、不改事实；`authorize` 仍把 `FailedLogic` 折成 `Ok(false)`，限额超限等仍是真错误。

本 crate 原先没有 `RESULT-UPDATE.md`。T3 对齐已经在源码和 `RESULT.md` 里（`pub type Cents = i64`，谓词常量，与 af-meter 同型）。这次只加快照，不改那些结论。

测试从 8 个增到 11 个。多出来的 3 个都是新快照测试。`cargo test -p af-credential` 两轮全绿，第二轮不改 `.snap`。`cargo clippy --all-targets -p af-credential -- -D warnings` 无新增警告。

## ② 步骤

1. 读完 `crates/af-credential/RESULT.md` 与 `src/{credential,judge,policy}.rs`、`tests/{attenuation,intersection}.rs`。crate 目录下没有既有 `RESULT-UPDATE.md`。
2. `[dev-dependencies]` 增加 `insta = { version = "1.49.0", features = ["yaml"] }` 与 `serde = { workspace = true }`（1.0.229，已带 `derive`）。insta 1.49 的 `yaml` 只打开 serde，序列化是 insta 自带的，不拉 `serde_yaml`。serde 只给测试里的快照视图做 `Serialize`。`cargo check` 更新了工作区 `Cargo.lock`：新增 insta 1.49.0、console 0.16.6、encode_unicode 1.0.0、similar 2.7.0。没有改其他 crate 的源码或版本。
3. `authorize` 的判定抽到私有 `run_authorize`（仍用原来的 `verify_limits`：facts 1000 / iterations 100 / max_time 50ms）。新增 `denial()`：同一条路径，放行 `Ok(None)`，`FailedLogic` 展开成 `Denial`，不写 stderr。`authorize` 的 `Ok(false)` 与 stderr 文案不变。
4. 新增 `tests/yaml_snapshots.rs` 三个 `assert_yaml_snapshot!`，`.snap` 在 `tests/snapshots/`：
   - `two_policy_intersection_minimum`：Cedar 两条 permit ∩ 衰减后的 biscuit。工具交集是 `[test]`，花费上限取 `min(1000, 500) = 500`。最小授权只有 `reviewer + test`，观察到的最大放行花费是 500（501 在子凭证上被拒、在根上仍过）。`builder + git` 在根上放行，衰减后拒绝，但 `token_carries_capability` 仍为真（`git` 事实还在）。
   - `cents_issuance_payload`：序列化往返后的载荷。`type_name::<Cents>() == "i64"`。authority 块含 `budget_cents(1000)` 与 `spend_cents <= budget_cents`。衰减块是 `request_tool ∈ ["test"]` 与 `spend_cents <= 500`。子凭证的 authority 块与根相同，事实里的 `budget_cents` 仍是 1000。签名字节不进快照；同一载荷换密钥，线上长度不变（根 303、衰减后 515），长度写入快照。
   - `rejection_error_structure`：`denial()` 与 `authorize` 的布尔结果一致。覆盖衰减封死 git（block 1 check 0）、花费落在两档上限之间（block 1 check 1，`$s <= 500`）、同时超过两档上限（block 0 与 block 1 两条花费 check）、未知工具 `deploy`（`no_matching_policy` + 白名单 check）、错误命名空间（`no_matching_policy` 且 failed_checks 为空）、根凭证超预算、以及 cap 内放行（`denial: ~`）。
5. 快照生成用 `INSTA_UPDATE=1`。之后不带该变量跑了两轮 `cargo test -p af-credential`，三份 `.snap` 都通过，工作区没有 `.snap.new`。
6. 提交前跑过 `cargo fmt -p af-credential`、`cargo clippy --all-targets -p af-credential -- -D warnings`、`cargo test -p af-credential`。所有 cargo 命令前都 `source /data/af-factory/env.sh`。

## ③ 证据

```
$ source /data/af-factory/env.sh && cargo check -p af-credential --all-targets
     Locking 4 packages to highest compatible versions
      Adding console v0.16.6
      Adding encode_unicode v1.0.0
      Adding insta v1.49.0
      Adding similar v2.7.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.18s
```

新包许可证（crate 清单，对照 `deny.toml` 的 allow 列）：insta Apache-2.0，console MIT，encode_unicode Apache-2.0 OR MIT，similar Apache-2.0。本次没有跑 `cargo deny`。

```
$ cargo clippy --all-targets -p af-credential -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.41s
```

退出码 0。仍有一条既有 note：`proc-macro-error2 v2.0.1` future-incompat（cedar 传递依赖，`RESULT.md` 已记录），不是本 crate 的 warning。

```
$ cargo test -p af-credential
running 0 tests
test result: ok. 0 passed
running 4 tests (tests/attenuation.rs)
test result: ok. 4 passed; 0 failed
running 4 tests (tests/intersection.rs)
test result: ok. 4 passed; 0 failed
running 3 tests (tests/yaml_snapshots.rs)
test cents_issuance_payload ... ok
test rejection_error_structure ... ok
test two_policy_intersection_minimum ... ok
test result: ok. 3 passed; 0 failed
Doc-tests af_credential
test result: ok. 0 passed
```

紧接着再跑一轮同样的 `cargo test -p af-credential`，11 个测试仍全绿，`.snap` 无 diff，没有 `.snap.new`。

拒绝结构与 `RESULT.md` 记录的日志一致，例如衰减封死 git：

```
authorization failed: an allow policy matched (policy index: 0), and the following
checks failed: Check n°0 in block n°1: check if request_tool($t), ["test"].contains($t)
```

花费 800（根 1000 过、子 500 拒）：

```
Check n°1 in block n°1: check if spend_cents($s), $s <= 500
```

## ④ 反馈

1. 快照应锁逻辑载荷，不锁签名字节。密钥每签一次都变；Datalog 源、`Cents` 整数和线上长度稳定。`wire_len` 能进快照，是因为换密钥后长度被断言相等。
2. `denial()` 是加出来的读口，没有改 `authorize` 的折返。网关若要 block/check 序号，应调 `denial()`，不要解析 stderr 文案。
3. `inspect` 的 `tools` 是 `HashSet`，顺序不稳定。快照前排了序。不要对 `HashSet` 的 Debug 做快照。
4. 金额词表再次被快照钉住：衰减后事实仍是 `budget_cents(1000)`，更紧的 500 只出现在 check 字面量 `$s <= 500` 里，不是改写事实。这与「预算不能住在凭证里、累计账本在 af-meter」一致。
5. 命名空间不匹配时 biscuit 报 `no_matching_policy`，`failed_checks` 为空，Display 仍以 `checks failed: ` 结尾（空列表）。调用方应按 `kind` 分支，不要靠这条后缀判断「有 check 失败」。
6. insta 1.49 默认把多行字符串收成带 `\n` 的双引号。`INSTA_YAML_BLOCK_STYLE=1` 才会用字面量块。本次没设这个环境变量，避免快照依赖它。
7. `denial` 与 `authorize` 共用 `verify_limits`。默认 1ms 墙钟限额仍然不能依赖，本次没有改限额。

## ⑤ 遗留

- 累计预算仍不在凭证里（af-meter 账本）。本次没有做预算主体绑定。
- Cedar 仍是 schema-less。
- `DenialBody::InvalidBlockRule` 与 `AuthorizerNotEmpty` 已映射（`authorize` 也把它们当拒绝），当前签发路径打不到，快照里这两种没有样例。`DenialView` 上对应字段在其余拒绝里是 `~`。
- 根密钥轮换、third-party block 仍未做（见 `RESULT.md`）。
- `proc-macro-error2` future-incompat 仍在，来自 cedar 传递依赖。
- 本次没有跑 `cargo deny check`。新依赖许可证已按清单核对，在 `deny.toml` 的 allow 列里。

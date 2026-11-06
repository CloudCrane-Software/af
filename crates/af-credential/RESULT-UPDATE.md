# RESULT-UPDATE.md — W2F 根密钥持久形态 + bootstrap/replay

## ① 结论

**通过**。`af-credential` 的持久根不再每次 `KeyPair::new()`。`open(dir)` 首启（`Created`）生成 ed25519 根，写入 `<dir>/root.key`（权限恰好 `0600`），签发引导凭证并落库；重入（`AlreadyDeclared`）读回同一把公钥，引导凭证按落库字节重放，不重新签名。`root.key` 损坏、权限不是 `0600`、库只有一半、schema 版本或声明（namespace / tools 顺序 / budget_cents）对不上，都启动失败，不另生成密钥。

`rotate(dir)` 显式换根并记下 `generation`。旧根签发的凭证（含衰减块）在 `GracePolicy.grace` 内可验；`now >= grace_until_ms` 拒绝。宽限按退役的那一代单独计，后一次轮换不改前一代的截止。零宽限在轮换的同一时刻拒绝旧凭证。

`issue_root` 仍是一次性随机密钥，已有衰减 / 交集 / 快照测试语义没改。Datalog 执行限额仍是 `verify_limits()`（facts 1000 / iterations 100 / max_time 50ms），超限仍是真错误。验签路径不跑 authorizer。

未新增第三方 crate，也没加新的加密库。签名继续用 biscuit-auth 自带的 ed25519。元数据用 workspace 已有的 rusqlite 0.32.1（bundled）。

测试从 11 个增到 35 个。`cargo test -p af-credential` 全绿。`cargo clippy --all-targets -p af-credential -- -D warnings` 无新增警告。

## ② 步骤

1. 读完 `RESULT.md` ④-6 与 `src/credential.rs` 的 `KeyPair::new()` 路径。限额修复保持：`authorize` / `inspect` 仍走显式 `verify_limits()`。
2. `issue_root` 的组块抽到 `build_root_biscuit`。不带 `root_key_id` 时字节布局与原来一致。持久签发才写入代数。
3. 新增可注入时钟（`SystemClock` / `ManualClock`）。宽限比较只用 `Clock::now_ms()`。
4. `open` / `open_with_clock`：目录里没有 `root.key` 且没有 `credential.db` 时 `Created`；两份都在时 `AlreadyDeclared`；只在一份时 `Incomplete`。`credential.db` 用 WAL、`busy_timeout=5000ms`、`synchronous=NORMAL`、`foreign_keys=ON`。声明存在 `schema_meta`，代数公钥与宽限在 `root_generation`，引导凭证原文在 `bootstrap_credential`。
5. `rotate` / `rotate_with_clock` 以及 `OpenedRoot::rotate`：先写 `root.key.pending`（同一把新种子），再在一个事务里退役旧代、插入新代，然后原子替换 `root.key`。库已提交但 `root.key` 还是旧的时，下次打开用 journal 里的种子补导出，不另生成。journal 没提交则删掉。journal 损坏或权限不对则失败，不动 `root.key`。
6. 验签矩阵：轮换前、宽限内（含衰减凭证）、截止前 1ms、截止点、重启后、零宽限、连续两次轮换各自的宽限。另有损坏、`0644`/`0400`、声明不符、半份存储、符号链接、伪造但格式合法的 `root.key`、schema 不符、引导凭证被改、代数值不连续。
7. 提交前跑了 `cargo fmt -p af-credential`、`cargo clippy --all-targets -p af-credential -- -D warnings`、`cargo test -p af-credential`。cargo 前都 `source /data/af-factory/env.sh`。

## ③ 证据

依赖没有新包。`Cargo.lock` 只在 `af-credential` 的依赖列表里加上已有的 `rusqlite`、`serde_json`、`thiserror 2.0.21`。

```
$ source /data/af-factory/env.sh && cargo clippy --all-targets -p af-credential --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.26s
```

退出码 0。仍有一条既有 note：`proc-macro-error2 v2.0.1` future-incompat（cedar 传递依赖，`RESULT.md` 已记录），不是本 crate 的 warning。

```
$ cargo test -p af-credential --offline -- --test-threads=8
running 12 tests (unittests src/lib.rs)
test result: ok. 12 passed; 0 failed
running 4 tests (tests/attenuation.rs)
test result: ok. 4 passed; 0 failed
running 4 tests (tests/intersection.rs)
test result: ok. 4 passed; 0 failed
running 12 tests (tests/root_persist.rs)
test result: ok. 12 passed; 0 failed
running 3 tests (tests/yaml_snapshots.rs)
test result: ok. 3 passed; 0 failed
Doc-tests af_credential
test result: ok. 0 passed
```

35 个测试全绿。相对基线 11 个（衰减 4 + 交集 4 + 快照 3），多出的 24 个是：时钟 2、根存储单测 10、`tests/root_persist.rs` 12。原三组没有改断言。

单测里的库配方读数：`foreign_keys=1`，`journal_mode=wal`，`synchronous=1`（NORMAL），`busy_timeout=5000`。把 `bootstrap_credential.generation` 改成不存在的 99 被外键拒绝。

## ④ 反馈

1. 私钥只放在 `root.key`（崩溃窗口里还有 `root.key.pending`）。库里是公钥、宽限截止和引导凭证字节。格式合法但公钥对不上的 `root.key` 不会被库里的记录覆盖，因为库里没有种子。
2. 引导凭证的重放是字节相同，不是同一声明再签一次。biscuit 每次 `build` 都会抽下一跳密钥，重签会改掉线上字节。
3. 宽限在 `verify` 读字节时执行。`now < grace_until_ms` 才接受退役代数；相等即拒绝。已经留在内存里的 `Biscuit` 不会被事后撤销，网关每次都应从字节验签。
4. `open` 在引导凭证的签名仍能对上其代数公钥时成功，即使该代数已经过了宽限。过期后 `verify(bootstrap)` 返回 `GraceExpired`，但目录仍能打开，避免宽限一到进程就起不来。
5. 声明按首启原文比对。`["git","test"]` 与 `["test","git"]` 是不符，直接失败。
6. 权限必须恰好 `0600`。`0644` 和 `0400` 都拒绝，并且不把权限改回去。
7. 每次 `open` / `verify` / `issue` / `rotate` 持有目录上的排他 flock，并重新读盘。另一个进程轮换之后，本进程下一次调用看得到新代数。句柄上的 `outcome` 和引导凭证字节是这次 `open` 的结果，不随别人的轮换改变。
8. `credential.db`（以及 wal/shm）在打开连接时被收紧到 `0600`，避免引导凭证跟着 umask 变成组可读。这不改 `root.key`。

## ⑤ 遗留

- 累计预算仍不在凭证里（af-meter）。Cedar 仍是 schema-less。third-party block 仍未做。这些都是 `RESULT.md` 里的原遗留。
- 当前私钥丢了且没有匹配的 journal 时，目录起不来。没有恢复口令，也没有把旧种子留在库里。
- `proc-macro-error2` future-incompat 仍在，来自 cedar 传递依赖。
- 本次没有跑 `cargo deny check`。没有新增包，许可证集合不变。

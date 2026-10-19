# RESULT-UPDATE — T5 + T6 · af-workorder 哈希与签名

日期：2026-10-19。分支 `task-T56`。未改 `af-credential` 源码与其公共 API。`Cargo.lock` 只给 `af-workorder` 补了直接依赖边（`sha2 0.10.9`、`ed25519-dalek`，版本都是锁里已有的），没有升版本。

`cargo fmt` 使用 `cargo fmt -p af-workorder`，避免格式化其他 crate。

## T5 · FNV-1a64 → SHA-256

提交：`c535ac3`

### ① 结论

通过。`payload_hash` 改为 SHA-256 小写十六进制。`DecisionArtifact` 的字段没变，仍是 `payload_hash: String`。四层校验顺序没变。

选型：直接依赖 `sha2 = "0.10"`，解析为锁文件里已有的 **0.10.9**（`ed25519-dalek` 的传递依赖，`af-credential` 经 biscuit-auth 带入）。不引入 blake3。

### ② 步骤

1. `src/hash.rs`：删除 `fnv1a64` / `fnv1a64_hex`，改为 `sha256` / `sha256_hex`（`SHA256_LEN = 32`，`SHA256_HEX_LEN = 64`）。
2. `src/decision.rs`：签发与校验改走 `sha256_hex`。长度断言从 16 改为 64，并断言等于 `sha256_hex(payload)`。
3. `src/lib.rs`：公开导出改为 `sha256` / `sha256_hex`。crate 内不再有 FNV 调用点。
4. `Cargo.toml` 增加 `sha2 = "0.10"`。

测试：hash 模块原 2 个 FNV 占位测试替换为 2 个新测试，单测总数仍是 34，集成测试仍是 3。

- `nist_sha256_known_vectors`：空串与 `"abc"` 的 FIPS 180-4 向量，并断言重复计算相同。
- `distinct_inputs_have_distinct_hashes`：7 个不同输入（含 `"abc"` 与 `"abb"` 的单比特差异）哈希两两不同，hex 为 64 位小写。

### ③ 证据

```text
source /data/af-factory/env.sh
cargo fmt -p af-workorder
cargo clippy --all-targets --offline -p af-workorder -- -D warnings
# Finished `dev` profile ... af-workorder, exit 0, 无警告
cargo test --offline -p af-workorder
# unittests: ok. 34 passed; 0 failed
# tests/lifecycle.rs: ok. 3 passed; 0 failed
# doc-tests: ok. 0 passed; 0 failed
```

已知向量（测试内断言，运行通过）：

- `sha256("")` = `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- `sha256("abc")` = `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`

### ④ 反馈

RESULT 里「换哈希时 `DecisionArtifact` 接口不变，只动 `hash.rs`」仍然成立：结构体字段与四层校验顺序没动。调用点必须从 `fnv1a64_hex` 改到 `sha256_hex`，否则还会算出 16 位 hex。公开导出里的 `fnv1a64` / `fnv1a64_hex` 已删除；仓库内没有其他 crate 引用它们。

`sha2` 0.10 与 effect-gateway 的载荷指纹同一主版本，两边十六进制都是小写 64 字符。

### ⑤ 遗留

- 决策工件的签名在 T5 结束时仍是占位校验。T6 没有把它改成真签名，见 T6 ⑤。
- `RESULT.md` 仍记录 PoC 当时的 FNV 结论，作为历史证据保留，没有改写。

## T6 · ed25519 下沉到变更单

提交：`120c013`

### ① 结论

通过。变更单的签发与验签在 `af-workorder` 内闭环，不调用 `af-credential`，也不把订单签名绕进 biscuit token。

原语是 `ed25519-dalek = "2.2"`，解析为锁文件里已有的 **2.2.0**（biscuit-auth 使用的同一版本）。没有启用 `rand_core`：种子由调用方注入，本 crate 不读系统熵。没有 `sleep`。

`Signature` 字段保持 RESULT 已对齐的形状：算法标签 + `key_id` + 64 字节。真签名的算法标签是 `ed25519`，与决策工件的 `ed25519-placeholder` 分开。

### ② 步骤

1. 新增 `src/sign.rs`：`Ed25519Keypair::from_seed`、`Ed25519PublicKey::from_bytes` / `verify`。验签顺序：算法标签 → `key_id`（公钥字节的小写 hex）→ 长度 → `verify_strict`（拒绝可锻签名）。小阶公钥与不在曲线上的字节在 `from_bytes` 拒绝。`Debug` 只打印公钥标识。
2. `ChangeOrder::signing_message`：`b"af-workorder/change-order/v1\0"` 接 `serde_json` 编码。本结构没有 Map，编码由字段声明顺序决定。
3. `ChangeOrder::sign` 先 `validate` 再签名，非法变更单签不出去。`verify` 先验签再做业务校验。
4. `SignedChangeOrder`：`sign` / `verify` / `spawn_successor`。后继单只在验签通过后生成。原来的 `ChangeOrder::spawn_successor` 没改，未签名路径的已有测试保持原语义。
5. `SignError` 与 `ChangeError::SigningPayload` / `SignatureInvalid`。

新增 6 个测试，单测 34 → 40，集成测试仍是 3：

| 测试 | 覆盖 |
|---|---|
| `sign::sign_verify_roundtrip_is_deterministic_and_debug_hides_seed` | 同一种子两次签名字节相同；公钥字节能还原并验过；Debug 不含种子 hex |
| `sign::rejects_tamper_wrong_key_bad_algorithm_and_invalid_public_key` | 翻签名字节、错钥、只改 key_id、占位算法标签、空签名、离曲线公钥、全零小阶公钥、改消息 |
| `change::signed_change_order_roundtrip_verifies` | 域前缀、签名确定性、64 字节、serde 往返后仍验过 |
| `change::tampered_order_or_signature_fails_verify_and_spawn` | 改差异描述、翻签名字节、另一把钥；验签失败则不生成后继单 |
| `change::invalid_change_order_is_not_signed` | from = to 时 `SignedChangeOrder::sign` 返回 `FromEqualsTo` |
| `change::verified_signature_spawns_successor` | 验签通过后后继单绑定 `g-4` 且 `supersedes = wo-1` |

### ③ 证据

```text
source /data/af-factory/env.sh
cargo fmt -p af-workorder
cargo clippy --all-targets --offline -p af-workorder -- -D warnings
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.40s
# exit 0, 无警告
cargo test --offline -p af-workorder
# unittests src/lib.rs: ok. 40 passed; 0 failed; 0 ignored; 0 measured
# tests/lifecycle.rs: ok. 3 passed; 0 failed
# Doc-tests af_workorder: ok. 0 passed; 0 failed
```

`git diff` 相对 T5 提交，`Cargo.lock` 只多了一行 `af-workorder` 依赖 `ed25519-dalek`。`crates/af-credential` 无 diff。

### ④ 反馈

1. 下沉方式是复制原语，不是依赖 `af-credential`。凭证 crate 的公共面是 biscuit 签发/衰减/求交，没有可复用的「给任意字节签 ed25519」trait。若 workorder 依赖它，变更单签名会绕进 token。dalek 2.2.0 已在锁里，直接用即可。
2. `key_id` 用公钥字节的 hex，验签时和给出的公钥比对。只改 `key_id` 不改签名字节会走到密码学校验并失败，不能靠标签蒙混。
3. 域分离前缀避免同一把钥以后签决策工件时和变更单串单。被签字节不要手写拼接业务字段，以 `signing_message()` 为准。
4. 未签名的 `ChangeOrder::spawn_successor` 仍可用。要「先验签再生成后继单」走 `SignedChangeOrder::spawn_successor`。这是加出来的路径，没有改 RESULT 已验证的未签名生成语义。
5. 种子注入、不启用 `rand_core`，测试不依赖系统熵，签名按 RFC 8032 确定性重复。

### ⑤ 遗留

- **决策工件仍是占位验签**（算法标签 `ed25519-placeholder`，只查非空）。RESULT 已验证的四层校验和现有测试把这个行为钉死；本次任务卡要求的是变更单闭环。若决策工件也改成真签名，要改 `DecisionArtifact::seal` / `validate_against` 并替换占位测试，应单独做。
- 没有密钥标识注册表或轮换。调用方自己保管种子，验签方拿公钥字节。
- 预算凭证仍只看 `signed` 布尔位，没有接这套 ed25519。
- `Sla.deadline` 仍是 RFC3339 字符串比较。本次未接入 af-calendar。
- 根 `README.md` 里「FNV / ed25519 下沉」那条清单未改（不在改动范围）。

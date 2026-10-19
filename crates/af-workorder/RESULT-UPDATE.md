# RESULT-UPDATE — W3D · WorkOrder 稳定身份与分片映射

日期：2026-10-19。分支 `task-W3D`。业务语义沿用已合流的 T4 / T56 / C5：门控两层、`graph_version` 无 setter、SLA 截止是 `DateTime<Utc>`、`payload_hash` 是 SHA-256 小写十六进制、变更单签名是注入种子的 ed25519。`CHANGE_ORDER_SIGNING_DOMAIN` 字节未改。

源码只动 `crates/af-workorder/**`。没有新的第三方 crate，`Cargo.lock` 没有改。

提交：`06dbdb1`（stable_id 与分片）、`b10d613`（拒绝式信封）、`f883eb0`（决策哈希与签名域回归）。

## 结论

通过。`WorkOrder` 增加稳定身份 `stable_id`。`WorkOrder::new` 用域前缀 `af-workorder/stable-id/v1\0` 接订单 id 做 SHA-256，写成 `sid-` 加 64 位小写十六进制。不读系统熵，也不读引擎 run。`continue_as_new` 只替换 `run_id`。`stable_id`、`id`、`graph_version`、状态和履历都不变。换 run 不是状态机边，不写入 `HistoryRecord`。

`with_stable_id` 原样带回已有身份，不按订单 id 重算。同一 `stable_id`、不同 run，`same_identity` 为真。后继工单是新单，按新 id 生成新的 `stable_id`，不继承原单。

分片键 `shard_key = sha256(stable_id 的字符串) mod shard_count`。摘要按大端 256 位整数取模。`shard_count` 是调用方传入的配方层值，不存在工单上。0 返回 `ShardError::ZeroShardCount`。同一身份与同一分片数多次调用结果相同，换 run 不改变分片。

serde 不给 `stable_id` / `run_id` 填默认值。`decode_work_order` 与 `WorkOrder` 的 `Deserialize` 共用同一道门禁：缺字段、JSON null、空串或类型不对，都返回 `RejectionEnvelope`，`field` 点名是哪一个。两个都缺时先点名 `stable_id`。空 `run_id` 表示尚未绑定，允许；缺键不允许。

决策工件四层校验顺序没变。订单路径 `decision_payload` / `seal_decision` 把 `stable_id` 写进被签字符串，`run_id` 不写进去。回归测试钉死原文与哈希。变更单签名域仍是 `b"af-workorder/change-order/v1\0"`，被签字节仍是该前缀接变更单自己的 JSON，不含 `stable_id`。

## 步骤

1. `src/identity.rs`：`StableId`、`RunContext`、`shard_key`、`decision_payload`。`src/order.rs` 增加私有字段 `stable_id` 与 `run_id`，以及 `continue_as_new` / `same_identity` / `with_stable_id`。`WorkOrder::new` 的参数没变。
2. `decode_work_order` 先查身份字段，再读入同形的 `WorkOrderWire`。`Deserialize` 走同一函数，所以 `serde_json::from_str::<WorkOrder>` 也不会补默认身份。错误类型是 `OrderDecodeError::Rejected(RejectionEnvelope)`。
3. `WorkOrder::seal_decision` 对 `decision_payload` 调用原来的 `DecisionArtifact::seal`。自由字符串的 `seal` 没改。`change.rs` 只加了签名域回归测试，签发与验签代码没动。

测试数字（相对 C5 记录的 42 个单测、9 个集成测试）：

| 提交 | 新增 | 之后 |
|---|---|---|
| `06dbdb1` | 单测 4：生成向量、空白拒绝、分片向量、生命周期内身份不随 run 变。集成 2：不同 run 身份连续、分片确定性 | 单测 46，集成 11 |
| `b10d613` | 单测 1：老载荷拒绝式信封。集成 1：缺 `stable_id`、缺 `run_id`、空 `stable_id`、完整往返 | 单测 47，集成 12 |
| `f883eb0` | 单测 1：签名域字节与签名体不含 `stable_id`。集成 1：决策哈希纳入 `stable_id` | 单测 48，集成 13 |

已有测试没有改断言。集成测试仍是 calendar 3、json 快照 3、lifecycle 3，另加 identity 4。

## 证据

```text
source /data/af-factory/env.sh
cargo check -j 4 -p af-workorder
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.70s
# exit 0

cargo fmt -p af-workorder

cargo clippy --all-targets -j 4 -p af-workorder -- -D warnings
# Checking af-workorder
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.73s
# exit 0，无警告

cargo test -j 4 -p af-workorder
# unittests src/lib.rs: ok. 48 passed; 0 failed
# tests/calendar_sla.rs: ok. 3 passed; 0 failed
# tests/identity.rs: ok. 4 passed; 0 failed
# tests/json_snapshots.rs: ok. 3 passed; 0 failed
# tests/lifecycle.rs: ok. 3 passed; 0 failed
# Doc-tests af_workorder: ok. 0 passed; 0 failed
```

钉死的向量（测试内断言，运行通过）：

- `StableId::generate("wo-1")` = `sid-f905c3b464315a0a05fe2682f64a93aa2194016459b62a5b554f8d60692f451f`
- `shard_key("sid-fixed", 64)` = 32，`shard_key("sid-fixed", 100)` = 36，`shard_key("sid-other", 64)` = 11，`shard_key(_, 1)` = 0
- 决策 payload `af-workorder/decision-payload/v1|9|sid-fixed|15|release-request` 的 SHA-256 = `18d638628bd06103e7138ac40e673dee658d184f65965834b3a96c60e519754c`
- `CHANGE_ORDER_SIGNING_DOMAIN` = `af-workorder/change-order/v1\0`

## 反馈

1. 身份的权威值是落在工单上的 `stable_id`，不是每次用订单 id 重算。`new` 只在创建时算一次。CAN 续接用 `with_stable_id` 或反序列化后再 `continue_as_new`。若按 id 重算，调用方改了业务 id 就会被看成另一张单。
2. 变更单的后继单不继承 `stable_id`。换版是新工单（`supersedes` 指回原单），和引擎换 run 不是一回事。若产品要求换版也保持同一身份，需要另定规则。
3. 分片数留在配方层。本 crate 不保存它，也不读取配方文件。算法是整段 SHA-256 的大端取模，不是只取前 8 字节。换配方的分片数会改变 `shard_key`，这是参数变化，不是身份变化。
4. 拒绝式信封是结构化对象（`kind` / `field` / `detail`），`Display` 以「拒绝式信封」开头并带字段名。直接 `serde_json::from_str::<WorkOrder>` 的错误字符串同样包含这两点。不要用 `#[serde(default)]` 把老载荷补成空身份。
5. `DecisionArtifact::seal` 仍对调用方给出的字符串做哈希，四层顺序没动。订单决策要带上身份，走 `WorkOrder::seal_decision`。自由 payload 的旧测试因此不用改。
6. `WorkOrder` 的 `Deserialize` 先收成 `serde_json::Value` 再查字段。这是本 crate 的 JSON 边界。非对象（例如数组）仍由 serde 报类型错误，不伪装成缺 `stable_id`。

## 遗留

- 决策工件仍是占位验签（`ed25519-placeholder`）。本次没有改 `seal` / `validate_against` 的四层顺序。
- 没有稳定身份注册表。调用方自己保存 `stable_id`，续接时原样带回。
- 分片数的配方校验不在本 crate。传入 0 会拒绝，其它正整数照取模。
- `fuzz/fuzz_targets/af_workorder_order_parse.rs` 不在本任务改动范围。`WorkOrder::new` 的参数没变，旧的、没有 `stable_id` 的 JSON 现在解析失败，fuzzer 本来就把解析失败当成跳过。
- 预算凭证仍只看 `signed` 布尔位。履历仍没有时间戳。

---

# 历史 · C5 · insta 快照接入 af-workorder

日期：2026-10-19。分支 `task-C5`。业务语义沿用已合流的 T4 / T56：SLA 截止是日历解析出的 `DateTime<Utc>`，`payload_hash` 是 SHA-256 小写十六进制，变更单签名是注入种子的 ed25519。本任务只加 JSON 快照，不改这些语义。

源码改动在 `crates/af-workorder/**`。根 `Cargo.lock` 同时更新：新增 crates.io dev-dependency 必须落锁。其他 crate 的源码与清单没有改。

提交：`c05a9bb`（依赖）、`6699b4a`（测试与 `.snap`）。

## 结论

通过。dev-dependency 为 `insta = 1.49.0`（crates.io 当前最新稳定版）。`default-features = false`，只开 `json` 与 `redactions`。

三条 `assert_json_snapshot!`：

| 快照 | 覆盖 |
|---|---|
| `qualified_final_artifact_release_decision` | 合格终态工件 `fa-1`（`is_final && qualified`）挂上后 `close` 得到 `Closed`。审批人签发 `Decision::Approve`。`payload_hash` 涂成 `[payload_hash]`，`signature` 涂成 `[signature]` |
| `reject_decision_names_missing_qualified_final_artifact` | 只有「终态不合格」和「合格但非终态」工件时 `close` 被拒。审批人签发 `Decision::Reject`。快照写出 `missing: ["NoQualifiedFinalArtifact"]`、`detail: ["缺少合格的终态工件"]`，以及门控原文 `门控拒绝 Close:前置条件缺失 [NoQualifiedFinalArtifact]`。签名与哈希同样涂红 |
| `signed_change_order_roundtrip` | `SignedChangeOrder` 经 `serde_json` 往返后与原文相等，公钥验签通过，并能生成绑定 `g-4`、`supersedes = wo-1` 的后继单。`.signature` 涂成 `[signature]` |

`.snap` 在 `tests/snapshots/`。`INSTA_UPDATE=no` 连续跑两次，三份快照的 SHA-256 不变，没有 `.snap.new`。

测试数字：单测仍是 42。集成测试 6 → 9，多出来的 3 个都在 `tests/json_snapshots.rs`。已有测试没有改断言。

## 步骤

1. `Cargo.toml` 增加 insta 1.49.0。`Cargo.lock` 增加 insta 1.49.0，以及传递依赖 pest 2.9.2、pest_derive 2.9.2、pest_generator 2.9.2、pest_meta 2.9.2、similar 2.7.0、ucd-trie 0.1.7。这些许可证是 Apache-2.0 或 MIT OR Apache-2.0，落在仓库 `deny.toml` 已有 allow 列表里。
2. 新增 `tests/json_snapshots.rs`。决策视图用 `#[serde(flatten)]` 把 `DecisionArtifact` 的 serde 字段摊到 JSON 根上，涂红选择器因此是 `.signature` 与 `.payload_hash`。变更单直接对 `SignedChangeOrder` 涂红 `.signature`。快照落盘后，测试读 `.snap` 文本，断言含占位、不含哈希原文、key_id，以及占位签名字节对应的 JSON 数字 `222`（`0xde`）。
3. `INSTA_UPDATE=1` 生成三份 `.snap`。之后用 `INSTA_UPDATE=no` 复跑，确认无 diff。截止时刻与签名种子都是测试里的常量，不读系统钟，也不读系统熵。

## 证据

```text
source /data/af-factory/env.sh
cargo check -j 4 -p af-workorder
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.07s
# exit 0

cargo fmt -p af-workorder

cargo clippy --all-targets -j 4 -p af-workorder -- -D warnings
# Checking af-workorder
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.65s
# exit 0，无警告

INSTA_UPDATE=no cargo test -j 4 -p af-workorder
# unittests src/lib.rs: ok. 42 passed; 0 failed
# tests/calendar_sla.rs: ok. 3 passed; 0 failed
# tests/json_snapshots.rs: ok. 3 passed; 0 failed
# tests/lifecycle.rs: ok. 3 passed; 0 failed
# Doc-tests af_workorder: ok. 0 passed; 0 failed

INSTA_UPDATE=no cargo test -j 4 -p af-workorder --test json_snapshots
# ok. 3 passed; 0 failed
```

二次运行前后三份 `.snap` 的 SHA-256 相同：

```text
a1ed9dd7a24c75f3fdf1db8d5db54a178e514138c0690ff1118fbb1dffc537d2  json_snapshots__qualified_final_artifact_release_decision.snap
b9c75f07a31983b72c1e9d639d9e34fd85b387380a3585fc8dab115c8d300a04  json_snapshots__reject_decision_names_missing_qualified_final_artifact.snap
6bfb6ffb3bc3e2a2d8796bf305972207e86aa49f9dd520c8f971391659425c12  json_snapshots__signed_change_order_roundtrip.snap
```

`find crates/af-workorder -name '*.snap.new'` 无输出。

## 反馈

1. 注入种子的 ed25519 签名字节是确定的（T6 已用同一种子签两次对过）。本快照用固定种子 `0x7a` 重复 32 字节。涂红的作用是不把 `key_id`（公钥字节的小写十六进制）和签名字节钉进快照；换种子或换钥时，未涂红的快照会 diff。决策工件的签名仍是 `DecisionArtifact::seal` 的占位：算法标签 `ed25519-placeholder`，64 个 `0xde`。`validate_against` 的四层顺序没有动。
2. `payload_hash` 对固定 payload 也是确定的 SHA-256。快照仍按任务把它涂成 `[payload_hash]`。涂红之前，测试断言长度等于 `SHA256_HEX_LEN`，且等于 `sha256_hex(payload)`。
3. `DecisionArtifact` 的字段里没有「缺什么」。拒绝快照在工件 serde 字段旁边放了门控的 `missing`、`detail` 和 `gate_message`，`.snap` 里可以直接看到缺的是合格终态工件。工件本身的字段名与顺序没有改。
4. insta 1.49.0 的 `json` feature 只打开 serde，不另带一份 serde_json。`redactions` 带入 pest。关掉 default features 后没有 console。`cargo add` 把 pest 锁在 2.9.2（当时索引上另有 2.9.4）。insta 要求 `pest ^2.1.3`，2.9.2 满足，锁文件保持 `cargo add` 的解析结果。
5. 更新已入库快照的方式是 `INSTA_UPDATE=1`（或 `always`）。默认的 `auto` 在失败时会写 `.snap.new` 而不是改 `.snap`。

## 遗留

- 决策工件仍是占位验签。真 ed25519 只在变更单上。若要把决策工件也改成真签名，要改 `seal` / `validate_against` 并替换占位测试，应单独做；那时快照里的签名选择器可以保持不变。
- 整段 `.signature` 被涂成一个字符串后，`.snap` 正文不再包含算法标签。算法由测试断言保住：决策工件是 `ed25519-placeholder`，变更单是 `ed25519`。
- 没有安装或调用 `cargo-insta`。审查流程不在本任务里。
- 根 `README.md` 与 `deny.toml` 未改。新依赖的许可证已经在 allow 列表中。

---

# 历史 · T5 + T6 · af-workorder 哈希与签名

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

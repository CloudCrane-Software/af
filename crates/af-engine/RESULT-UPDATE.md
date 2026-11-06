# RESULT-UPDATE — F5 · approve_decision 绑订单

日期：2026-10-19。分支 `task-F5`。引擎改动只在 `crates/af-engine/**`。编排侧见仓库根 `RESULT-UPDATE.md`。没有新的第三方 crate，没有改 `af-workorder`。

提交：`96a3faa`。

## 结论

`approve_decision` 的 payload 由引擎从订单现算：`WorkOrder::decision_payload(graph_version)`。域分离格式绑住 `stable_id`，subject 绑住 `graph_version`。调用方自带 payload 必须逐字相同，否则 `BadRequest`（message 含「payload 与订单现算的 decision_payload 不一致」）。然后才对这份现算字符串做四层校验。工件对、订单错（另一个 `stable_id` 或另一个 `graph_version`）拒绝。换 `run_id` 仍通过。不修改订单。`release` 门控仍不读决策。

这覆盖 W3A 里「只拿调用方 payload 调用 `validate_against`」的行为。那是 F5 要修的洞。W4F 的版本信封没有改。

## 步骤

失败测试先红：`stable_id` 换成 `sid-other` 时旧实现返回 `ok: true`。再改 `src/lib.rs` 的 `Op::ApproveDecision`，并更新 README、契约请求样例和原有审批测试，使它们使用现算 payload。

测试数字：集成测试 25 → 26，新增 `approve_decision_rejects_matching_artifact_on_the_wrong_order`。单测仍是 4。

## 证据

见仓库根 `RESULT-UPDATE.md` 的命令输出。本 crate：`cargo test -j 4 -p af-engine` 集成测试 26 passed。clippy `--all-targets -D warnings` 无警告。

## 反馈

自带 payload 不一致是 `BadRequest`，不是 `Decision::PayloadHashMismatch`。四层校验只在 payload 已经等于现算值之后进行。

## 遗留

编排是否真的调用本 op，不在本 crate。根 `RESULT-UPDATE.md` 记录了 workflow 侧的实测。

---

# RESULT-UPDATE — F6 · 拒绝式信封结构化透传

日期：2026-10-19。分支 `task-F6`。只改 `crates/af-engine/**` 与 `crates/af-workorder/**`。没有新的第三方 crate，`Cargo.lock` 没有改。

本 crate 的提交：`5ab047e`（红：复现信封被压成 BadRequest）、`c8e1f94`（绿：`kind`/`field`/`detail` 透传）。工单侧「未知字段列名单」记在 `crates/af-workorder/RESULT-UPDATE.md`（`fa38d87`、`d5d3ed3`）。

已验证语义没有推翻：版本门不够、下限无法解析，仍是 `BadRequest`，message 仍含「请升级引擎」或「无法解析」。未知 op、坏 JSON、参数类型错、订单快照里的非法状态（如 `Flying`）仍是 `BadRequest`。`Gate` / `Decision` 仍是库 serde 的原样。请求顶层未列出的字段仍被忽略。`engine_version` 仍是编译期 `0.1.0`。

## 结论

通过。订单快照若被 `af-workorder` 的 `RejectionEnvelope` 拒绝，响应不再是 `{"kind":"BadRequest","message":"订单快照无法反序列化: 拒绝式信封:..."}`。`error.kind` 为 `RejectionEnvelope`，`field` 与 `detail` 原样来自库信封，没有 `message`。`BadRequest` 与 `RejectionEnvelope` 是两种 kind。拒绝体不带 `order`，编排壳不会用有损快照覆盖状态。

库信封自己的 `RejectionKind`（`missing_field` / `blank_field` / `invalid_field` / `unknown_field`）不另占一个线字段。线上的 `kind` 固定是协议名 `RejectionEnvelope`。缺的是哪一类，看 `field` 与 `detail` 的原文。

## 步骤

1. 红：`rejection_envelope_keeps_field_and_detail`。去掉 `stable_id` 的 `release` 请求，stdout 的 `error.kind` 当时是 `BadRequest`，整句结构在 `message` 里。
2. 绿：`parse_order` 改走 `decode_work_order_value`。`OrderDecodeError::Rejected` 映射成 `WireError::RejectionEnvelope { field, detail }`。`Malformed` 与 `Invalid` 仍走 `BadRequest`，message 前缀仍是「订单快照无法反序列化:」。
3. 契约样例增加 `order_missing_stable_id` 与 `order_unknown_fields`。`README.md` 的错误表把这两种 kind 分开。

测试数字（相对 W4F 的单测 4、集成 25）：单测仍是 4（`version::tests`）。`src/main.rs` 单测仍是 0。集成测试 25 → 26，多出来的是 `rejection_envelope_keeps_field_and_detail`。契约请求/响应对 26 → 28。文档测试仍是 0。已有契约样例没有改字节。

## 证据

红（`5ab047e`，修复前，缺 `stable_id`）：

```text
source /data/af-factory/env.sh
cargo test -j 4 -p af-engine --test engine rejection_envelope_keeps_field_and_detail
# assertion `left == right` failed:
# {"engine_version":"0.1.0","error":{"kind":"BadRequest","message":"订单快照无法反序列化: 拒绝式信封:老载荷缺少字段 stable_id,拒绝补默认值"},"ok":false}
# left: String("BadRequest")
# right: "RejectionEnvelope"
# test result: FAILED. 0 passed; 1 failed; 25 filtered out
```

绿：

```text
source /data/af-factory/env.sh
cargo check -j 4 -p af-engine
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.42s
# 随后去掉未再使用的 serde::Deserialize

cargo fmt -p af-engine
# 无输出，exit 0

cargo clippy --all-targets -j 4 -p af-engine -- -D warnings
# Checking af-engine v0.1.0
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.32s
# exit 0，无警告

cargo test -j 4 -p af-engine
# unittests src/lib.rs: ok. 4 passed; 0 failed
# unittests src/main.rs: ok. 0 passed; 0 failed
# tests/engine.rs: ok. 26 passed; 0 failed; 0 ignored; 0 measured
# Doc-tests af_engine: ok. 0 passed; 0 failed

cargo test -j 4 -p af-workorder -p af-engine
# af-engine 单测 4、集成 26；af-workorder 单测 49、集成 13；文档测试均为 0
# exit 0
```

修复后的响应（与契约文件一致，测试内与库信封的 `field` / `detail` 对拍）：

```text
{"ok":false,"error":{"kind":"RejectionEnvelope","field":"stable_id","detail":"老载荷缺少字段 stable_id,拒绝补默认值"},"engine_version":"0.1.0"}
{"ok":false,"error":{"kind":"RejectionEnvelope","field":"injected_alpha, injected_beta","detail":"不认识的字段: injected_alpha, injected_beta,拒绝载入以免静默丢弃"},"engine_version":"0.1.0"}
```

`order_bad_state` 仍是 `BadRequest`，message 仍含「订单快照无法反序列化:」和 `unknown variant `Flying``。

`git diff --name-only aa4689b -- . ':!crates/af-workorder' ':!crates/af-engine'` 无输出。

## 反馈

1. `WorkOrder::deserialize` 只会把信封 `Display` 成一句 serde 错误。要留住 `field` 与 `detail`，必须走 `decode_work_order_value`（或 `decode_work_order`），不能再 `map_err` 成字符串。
2. 线上 `kind` 是协议名 `RejectionEnvelope`，不是库里的 `missing_field`。`detail` 仍是库原文，所以「缺少字段」「为空或含空白」「类型不对」「不认识的字段」还在。
3. 多个不认识的字段名在 `field` 里用 `, ` 连接，与库的 `RejectionEnvelope::unknown_fields` 相同。引擎不重排、不改写。
4. 请求顶层的额外字段仍被忽略。这次拒绝的是 `order` 对象上的未知字段，不是请求信封上的未知字段。
5. 版本不够和版本解析失败没有新的 kind。W4F 那条「版本错误用 BadRequest」仍然成立。

## 遗留

- `orchestration/workflow.py` 的 `_engine` 在 `ok` 为假时用 `error.message` 拼异常文本，用 `error.kind` 当类型。`RejectionEnvelope` 没有 `message`，异常文本里的 message 会是空。kind 会变成 `RejectionEnvelope`。该文件不在改动范围。覆盖状态只发生在 `ok` 为真之后，所以这次拒绝不会写回有损订单。
- 嵌在 `line` / `voucher` / `artifact` 里的未知字段，库侧还没有拒绝。见 af-workorder 的 F6 遗留。
- W4F 留下的编排选路、变更单 op、SLA 逾期 op，本次没有改。

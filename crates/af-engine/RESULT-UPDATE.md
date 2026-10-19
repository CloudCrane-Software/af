# RESULT-UPDATE — W4F · Worker Versioning v2 上线流程化

日期：2026-10-19。分支 `task-W4F`。只改 `crates/af-engine/**`。没有新的第三方 crate，没有改其他 crate，`Cargo.lock` 无变化。

提交：`158e88e`（版本信封与测试）、`a2de168`（`VERSIONING.md` 与干跑）。

本 crate 没有单独的 `RESULT.md`。动手前读的是下面保留的 W3A 记录，其中的已验证语义没有推翻。

## 结论

通过。请求可以带 `engine_version_min`。本二进制的 `engine_version`（编译期 `CARGO_PKG_VERSION`，当前 `0.1.0`）低于它时拒绝，message 提示升级；响应仍然带 `engine_version`。字段不存在则不设下限，旧请求的行为与契约样例保持原样。字段存在但不是字符串，或不能按 semver 解析时拒绝，不把看不懂的下限当成没有下限。下限门在 `op` 和订单解析之前，版本不够时不返回领域错误。

`VERSIONING.md` 写下 `EngineChannel { stable, canary }` 的编排选择规则：新规则先发 canary；旧 run 锁旧版，引擎无状态，所以锁的是「该 run 全程用同版本」这一条约定；回滚就是摘除 canary 指针。`scripts/engine-channel-dry-run.sh` 只对操作步骤做 `command -v`，不执行切换。

## 步骤

1. 在固定检查顺序里插入下限门：空行 → JSON → 对象 → `engine_version_min`（字段存在时）→ `op` → 订单 → 参数 → 领域判定。比较放在 `src/version.rs`，三段 semver，预发布按规范比较，build 元数据不参与比较。
2. 集成测试覆盖下限正例（等于当前版本、`0.0.0`、`0.1.0-rc.1`、`0.1.0+build.7`）、负例（`0.1.1` / `0.2.0` / `1.0.0` / `99.0.0`、缺订单之前短路、无法解析、类型错误），以及放行、拒绝、版本门拒绝都带 `engine_version`。单测覆盖 semver 优先级链和解析失败原因。
3. 写 `VERSIONING.md`。操作步骤是一组单命令。集成测试从文档抽出这组命令，逐条 `command -v`，再运行干跑脚本，并确认没有创建 `ENGINE_CHANNEL_DIR`。

测试数字：单测 0 → 4，全部新增（`version::tests`）。集成测试 21 → 25，新增 4 个：`engine_version_min_accepts_current_lower_and_prerelease`、`engine_version_min_rejects_when_engine_is_older_or_min_is_unusable`、`engine_version_field_is_always_present`、`versioning_steps_dry_run_checks_commands_exist`。`src/main.rs` 单测仍是 0，文档测试仍是 0。没有改其他 crate 的测试。原有 21 个集成测试与 `tests/contract/` 样例仍通过。

## 证据

```text
source /data/af-factory/env.sh
cargo check -j 4 -p af-engine
# Checking af-engine v0.1.0
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
# exit 0

cargo fmt -p af-engine -- --check
# 无输出，exit 0

cargo clippy --all-targets -j 4 -p af-engine -- -D warnings
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
# exit 0，无警告

cargo test -j 4 -p af-engine
# unittests src/lib.rs: ok. 4 passed; 0 failed
# unittests src/main.rs: ok. 0 passed; 0 failed
# tests/engine.rs: ok. 25 passed; 0 failed; 0 ignored; 0 measured
# Doc-tests af_engine: ok. 0 passed; 0 failed
```

`git diff --stat fd72185 -- . ':!crates/af-engine'` 无输出。

干跑脚本当场输出（未创建通道目录）：

```text
dry-run: cargo: /data/af-factory/.cargo/bin/cargo :: cargo build -p af-engine --bin af-engine
dry-run: mkdir: /usr/bin/mkdir :: mkdir -p -- "$ENGINE_CHANNEL_DIR/bins" "$ENGINE_CHANNEL_DIR/pins"
dry-run: cp: /usr/bin/cp :: cp -f -- "$ENGINE_BIN" "$ENGINE_CHANNEL_DIR/bins/$ENGINE_BUILD"
dry-run: ln: /usr/bin/ln :: ln -sfn -- "$ENGINE_CHANNEL_DIR/bins/$ENGINE_BUILD" "$ENGINE_CHANNEL_DIR/canary"
dry-run: readlink: /usr/bin/readlink :: readlink -f -- "$ENGINE_CHANNEL_DIR/canary"
dry-run: test: test :: test -x "$ENGINE_CHANNEL_DIR/canary"
dry-run: rm: /usr/bin/rm :: rm -f -- "$ENGINE_CHANNEL_DIR/canary"
dry-run: test: test :: test ! -e "$ENGINE_CHANNEL_DIR/canary"
dry-run: not-executed
dry-run: ok
```

版本不够时的响应形状（下限 `99.0.0`，本二进制 `0.1.0`）：

```text
{"ok":false,"error":{"kind":"BadRequest","message":"引擎版本 0.1.0 低于 engine_version_min 99.0.0，请升级引擎"},"engine_version":"0.1.0"}
```

## 反馈

1. 下限比较是 crate 内的 semver 子集，没有引入 `semver` crate。接受 `MAJOR.MINOR.PATCH`、预发布标识和 build 元数据。不接受 `v` 前缀、缺段和前导零。正式版高于同号预发布版；数字标识按整数比（`beta.2` 低于 `beta.11`）。
2. 版本不够和解析失败都是已有的 `BadRequest`，没有新的 `kind`。两边靠 message 区分：过低含「请升级引擎」，解析失败含「无法解析」。
3. 通道目录、pin 和符号链接由编排按 `VERSIONING.md` 维护。引擎进程不读这些文件，也不在进程内切换二进制。
4. 干跑退出 0 只说明操作步骤里的命令在 `PATH` 中。脚本不执行 `cargo build`、`cp`、`ln`、`rm`。
5. `engine_version` 仍是编译期常量。两份二进制要各自编译；运行时设置 `CARGO_PKG_VERSION` 改不了响应，这一点与 W3A 相同。

## 遗留

- 编排层还没有按 `VERSIONING.md` 选择 stable / canary，也还没有在请求里填写 `engine_version_min`。那些文件不在本任务的改动范围里。
- 没有任何 pin 再引用时才删除 `bins/` 里的本体。文档写了这条规则，没有自动清理。
- W3A 留下的变更单、SLA 逾期 op、决策占位验签，本次没有改。

## 附：W3A 记录（保留，未推翻）

# RESULT-UPDATE — W3A · af-engine 薄壳引擎 bin

日期：2026-10-19。分支 `task-W3A`。未改 `af-workorder` 的源码与语义。根 `Cargo.toml` 的 `members = ["crates/*"]` 已覆盖新目录，没有改根清单。`Cargo.lock` 只增加 `af-engine` 这一条 path 包，没有新的第三方 crate。

提交：`46fb3af`（协议与 README）、`4f73f7e`（21 个集成测试与 `tests/contract/` 样例）。

## 结论

通过。`af-engine` 是无状态 bin：stdin 一行 JSON 请求，stdout 一行 JSON 响应，然后退出。订单快照整单进、整单出。`WorkOrder`、`GateError`、`DecisionError` 都用 `af-workorder` 的派生 serde，不在本 crate 重定义。

九个 op 都映射到现有公共 API。`attach_voucher` 调用 `WorkOrder::attach_budget_voucher`。`approve_decision` 调用 `DecisionArtifact::validate_against`，不改订单。未知 op、坏 JSON、字段类型错、订单快照反序列化失败都是 `{"kind":"BadRequest","message":...}`。`engine_version` 是编译期 `env!("CARGO_PKG_VERSION")`，当前为 `0.1.0`。

必经引擎：`release`、`close`、`approve_decision`。允许编排层本地执行（引擎仍提供）：`bind_line`、`attach_voucher`、`start`、`attach_artifact`、`complete`、`cancel`。说明写在 `crates/af-engine/README.md`。

## 步骤

1. 新建 `crates/af-engine`（lib + bin）。`src/main.rs` 只读 stdin 第一行并写出一行响应。`src/lib.rs` 按固定顺序检查：空行、JSON、对象、op、订单快照、参数、领域判定。
2. 领域失败把库错误原样放进响应：`{"kind":"Gate","gate":<GateError serde>}` 或 `{"kind":"Decision","decision":<DecisionError serde>}`。放行只带 `order`，拒绝只带 `error`，两者都带 `engine_version`。协议内的拒绝退出码为 0。
3. `tests/engine.rs` 用 `std::process` 起子进程喂 JSON。每个 op 至少一对正例和负例。负例与库方法的 `serde_json::to_value` 对拍。另有坏 JSON、缺字段、类型错、版本字段、无换行 EOF、只处理第一行，以及八步跨进程生命周期。
4. `tests/contract/` 留下 26 对单行 `*.request.json` / `*.response.json`（响应是 stdout 原文）和一张 `work_order.created.json`。测试把样例与现场进程输出逐字节比较。

测试数字：本 crate 原先没有测试。现为集成测试 21 个，全部新增。单测 0，文档测试 0。没有改其他 crate 的测试。

## 证据

```text
source /data/af-factory/env.sh
cargo check -j 4 -p af-engine
# Checking af-engine v0.1.0
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s
# exit 0

cargo fmt -p af-engine -- --check
# 无输出，exit 0

cargo clippy --all-targets -j 4 -p af-engine -- -D warnings
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
# exit 0，无警告

cargo test -j 4 -p af-engine
# unittests src/lib.rs: ok. 0 passed; 0 failed
# unittests src/main.rs: ok. 0 passed; 0 failed
# tests/engine.rs: ok. 21 passed; 0 failed; 0 ignored; 0 measured
# Doc-tests af_engine: ok. 0 passed; 0 failed
```

`git diff --stat b7d3a9b -- . ':!crates/af-engine' ':!Cargo.lock'` 无输出。`Cargo.lock` 相对基线 9 行，只有 `af-engine` 包条目。

现场 stdout 摘录（与契约文件一致）：

```text
{"ok":false,"error":{"kind":"BadRequest","message":"未知 op: frob"},"engine_version":"0.1.0"}
{"ok":false,"error":{"kind":"Gate","gate":{"PreconditionsNotMet":{"transition":"Release","missing":["LineVersionNotBound","BudgetVoucherNotIssued"]}}},"engine_version":"0.1.0"}
{"ok":false,"error":{"kind":"Decision","decision":{"PermissionDenied":{"decision":"Approve","required":"Approve","granted":["Read"]}}},"engine_version":"0.1.0"}
```

`InProgress` 且已有合格终态工件时，`close` 仍是 `IllegalTransition`（`from: InProgress`，`to: Closed`，`allowed: [Complete, Cancel]`），不是 `NoQualifiedFinalArtifact`。运行时把 `CARGO_PKG_VERSION` 设为 `9.9.9`，响应仍是 `0.1.0`。

## 反馈

1. `release` / `close` 的门控和 `approve_decision` 的四层校验都在这个进程里做。审批通过不会自动 `release`：工单类型没有决策字段，`release` 的缺失清单仍然只有产线绑定和预算凭证。编排层要自己先调 `approve_decision`，再调 `release`。
2. 线 op 名 `attach_voucher` 对应库方法 `attach_budget_voucher`。状态不对时，`MutationForbidden.op` 仍是 `"attach_budget_voucher"`，不是 `"attach_voucher"`。
3. `engine_version` 用编译期 `env!("CARGO_PKG_VERSION")`。编排拉起二进制时通常没有这个环境变量；若改成运行时读取，版本会被调用方改掉，或在变量缺失时无版本可报。
4. 协议内的放行和拒绝都退出 0，判定看 `ok`。只有 stdout 写失败才非 0。stderr 不承载协议。
5. 允许本地执行的 op 没有门控清单。编排层可以不调用本 bin，但规则仍以 `af-workorder` 为准。之后送进 `release` / `close` 时，引擎只按快照里的事实做门控。
6. 本机 cargo 1.99 不把 `CARGO_BIN_EXE_af_engine` 注入 `env!`，集成测试在运行时读这个变量。二进制名里的连字符对应下划线。

## 遗留

- 变更单（换 `graph_version`）不在 op 集里。本进程没有改版本的路径。
- 没有 SLA 逾期 op。`is_overdue` 要调用方注入时刻，本进程不读钟。
- 决策工件仍是 `af-workorder` 已有的占位验签（`ed25519-placeholder`）。本次没有改那条语义。
- Python / Temporal 编排不在本任务范围。契约在 `README.md` 和 `tests/contract/`。
- 没有给 `af-workorder` 加 `check_close_readiness`。`InProgress` 上的 `close` 保持已验证的 `IllegalTransition`。

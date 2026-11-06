# RESULT-UPDATE — F3 · fuzz/ 锁冲突与旧 API

## 结论

通过。`fuzz/Cargo.lock` 里的 47 段冲突标记已删掉，由 cargo 重新生成，cargo 可以解析。`af_effect_gateway_idempotency` 与 `af_workorder_order_parse` 按当前公共 API 调用：`declare_intent` / `execute` 递交 `WorkOrder`，声明成功时快照的 `declared_order_state` 等于声明当时的订单态，之后订单再迁移不改写。两个 target 各 2000 次、`--sanitizer none`、退出码 0、无 panic，`fuzz/artifacts` 下 0 个崩溃样本。

`cargo check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全绿。fuzz 包测试 6 → 14，多出来的 8 个是本次回归。未改任何产品 crate，未新增第三方 crate。根 `Cargo.lock` 无 diff。

## 步骤

1. 先读 `crates/af-effect-gateway/RESULT.md` 与 `RESULT-UPDATE.md`、`crates/af-workorder/RESULT.md` 与 `RESULT-UPDATE.md`，以及图 / 抽样的 RESULT。不改已验证语义：声明只允许 Released / InProgress，执行发送只允许 InProgress，已执行只重放；`stable_id` 创建时生成，缺字段走拒绝式信封，不补默认身份；`continue_as_new` 只换 `run_id`；不调用 `Transition::from_state`（Cancel 在那里是 `unreachable!`）。
2. 复现锁文件。`fuzz/Cargo.lock` 从第 6 行起是 `<<<<<<< HEAD`，`^<<<<<<<` 共 47 行。`cd fuzz && cargo check` 在解析锁时失败，退出码 101。
3. 删掉 `fuzz/Cargo.lock`，让 cargo 按独立工作区重新生成。清单补上源码已经在用、但合并时丢掉的路径依赖 `af-graph`、`af-sampling`，并登记已有的 `af_sampling_invariant_input` bin。没有新的 crates.io 依赖。`libfuzzer-sys` 仍解析为 0.4.13。
4. 锁能解析之后，复现旧签名。`af_effect_gateway_idempotency` 上 7 处 `declare_intent` 和 1 处 `execute` 报 E0061，缺 `&WorkOrder`。这是修之前的红。
5. 两个 harness 的逻辑收进 `fuzz/src/gateway.rs` 与 `fuzz/src/workorder.rs`，bin 只转发。网关按字节选订单态，声明时把这张 `WorkOrder` 递进去；同载荷两把键的 `payload_sha256` 必须相同，快照订单态必须等于声明当时的 `order.state()`，重复声明、开工、执行、完成之后都不改写。工单侧走 `decode_work_order`，缺 `stable_id` 只得到拒绝式信封；`new` 的身份等于 `StableId::generate(id)`；门控和 `continue_as_new` 不改 `stable_id` / `graph_version`。时刻用 `ManualClock` 和钳过的 `DateTime<Utc>`，没有 `sleep`。
6. `fuzz/.gitignore` 里另有两段冲突标记，其中一行会把 `corpus` 忽略掉。按 C3 已验证的口径改回：忽略 `target` / `artifacts` / `coverage`，种子可以入库。树上原来没有 corpus。补回现有测试点名的三份种子（`min_legal.json`、`orphan.json`、`min_recipe_one_event`），否则那 6 个旧测试在干净检出里会读文件失败。冒烟长出来的 corpus 没有入库。

提交：`874024f`。

## 证据

工具链：`source /data/af-factory/env.sh` 之后 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1`。fuzz 的 check / clippy / test / run 使用 `CARGO_TARGET_DIR=/tmp/f3-fuzz-target`，避免和共享 `target` 抢锁。工作目录是 `/data/af-factory/fx3`。

红（修之前）：

```text
cd fuzz && cargo check --offline -j 4
error: failed to parse lock file at: .../fuzz/Cargo.lock
TOML parse error at line 6, column 9
  6 | <<<<<<< HEAD
    |         ^
  key with no value, expected `=`
EXIT:101
```

`grep -c '^<<<<<<<' fuzz/Cargo.lock` = 47。

锁重新生成、target 尚未改时：

```text
error[E0061]: this method takes 4 arguments but 3 arguments were supplied
 --> fuzz_targets/af_effect_gateway_idempotency.rs:39:30
  (同样的缺参还有 :40 :42 :56 :59 :66 :72 的 declare_intent，以及 :61 的 execute)
  argument #4 of type `&af_workorder::order::WorkOrder` is missing
error: could not compile `af-fuzz` (bin "af_effect_gateway_idempotency") due to 8 previous errors
EXIT:101
```

绿：

```text
cargo fmt --manifest-path fuzz/Cargo.toml
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets --offline -j 4 -- -D warnings
# Finished `dev` profile ... af-fuzz in 0.22s
# CLIPPY:0，无警告行

cargo check --manifest-path fuzz/Cargo.toml --offline -j 4
# Finished `dev` profile ... in 0.22s
# CHECK:0

cargo test --manifest-path fuzz/Cargo.toml --offline -j 4
# running 14 tests
# graph 4、sampling 2、lock 2、gateway 3、workorder 3，全部 ok
# test result: ok. 14 passed; 0 failed; 0 ignored
# Doc-tests af_fuzz: 0 passed
# TEST:0
```

计数：修之前能编过的 lib 测试是 C3 留下的 6 个（图 4、抽样 2）。本次新增 8 个，所以是 14。旧 6 个断言没改。

```text
cargo fuzz run --sanitizer none af_workorder_order_parse -- -runs=2000
# #2000 DONE   cov: 1209 ft: 2182 corp: 166/505b lim: 4 exec/s: 0 rss: 35Mb
# Done 2000 runs in 0 second(s)
# WORKORDER:0

cargo fuzz run --sanitizer none af_effect_gateway_idempotency -- -runs=2000
# #2000 DONE   cov: 910 ft: 1296 corp: 85/252b lim: 4 exec/s: 2000 rss: 35Mb
# Done 2000 runs in 1 second(s)
# GATEWAY:0

find fuzz/artifacts -type f | wc -l
# 0
```

两次都有三行 `WARNING: Failed to find function "__sanitizer_*"`（`--sanitizer none` 不链接 ASan）。`cargo-fuzz` 0.13.2 启动时打印 `GLIBC_2.39` weak version not found。进程都以 0 退出，没有 panic。

`git diff -- Cargo.lock Cargo.toml`（仓库根）无输出。`grep` 冲突标记：`fuzz/` 下 0 处。

## 反馈

1. 这两处缺陷是编译期的，不是运行期断言失败。锁文件解析失败时 `cargo test` 起不来；签名不对时 bin 直接 E0061。红证据是这两次 `cargo check`。随后把同一条不变量写成库内测试，修完是绿的。
2. `fuzz/` 是空 `[workspace]` 的独立工作区。删锁再 `cargo check` 只重写 `fuzz/Cargo.lock`。路径依赖上的 `edition.workspace = true` 仍由根工作区继承，这次能编过。
3. 合并把 `src/lib.rs` 里的 `graph` / `sampling` 留在树上，却从清单里拿掉了 `af-graph` 和 `af-sampling`。不补这两条路径依赖，锁文件修好之后 `cargo check` 仍会在 `use af_graph` / `use af_sampling` 处失败。补的是本仓库已有 crate，不是新的第三方 crate。传递依赖（如 `cel-interpreter`、`rusqlite`）跟着进了 fuzz 锁，版本是 cargo 按清单解析的结果，`libfuzzer-sys` 仍是 0.4.13，`serde_json` 仍是 1.0.154，`chrono` 仍是 0.4.45。
4. 网关不保存工单。harness 每次调用递交同一张已经迁移的 `WorkOrder`。快照是声明那一次的 `order.state()`。已执行的意图在订单离开 InProgress 之后仍只重放，这是 W3B 已验证的行为，harness 不把重放当成失败。
5. 工单 JSON 缺 `stable_id` 现在解析失败，fuzzer 把失败当成跳过，不生成替补身份。自己 `WorkOrder::new` 出来的单会带上身份，往返必须保住 `stable_id` 和 `run_id`。

## 遗留

- 冒烟只覆盖任务点名的两个 target，各 2000 次，无 sanitizer。`af_graph_schema_parse` 和 `af_sampling_invariant_input` 能编过，这次没有再跑 2000 次。
- C3 记录的 118 个冒烟样本不在这个克隆里。入库的只有测试点名的 3 个种子。这次 libFuzzer 新写的样本已删，没有入库。
- `fuzz/.gitignore` 的冲突标记不在红队那句证据里，但不清掉的话 `corpus` 会被忽略，上面 3 个种子进不了 git，旧测试在干净检出里失败。一并清了。
- 产品 crate 的源码与根锁文件没有改。`Transition::from_state(Cancel)` 仍然会 panic，harness 继续避开。
- 网关仍信任调用方递交的 `WorkOrder`，不自己重跑工单状态机。这是 W3B 已记录的边界，本次没有改网关。

---

# RESULT-UPDATE — F5 · 审批门空心化修复

日期：2026-10-19。分支 `task-F5`。改了 `crates/af-engine/**` 与 `orchestration/workflow.py`、`orchestration/test_approval_gate.py`。没有新的第三方 crate，没有改 `af-workorder` 的源码。`Cargo.lock` 无变化。

提交：`96a3faa`（引擎按订单现算 payload）、`17b9125`（编排真调引擎，轨迹改读响应键）。

## 结论

通过。`approve_decision` 不再用调用方自带的 payload 做哈希。引擎用 `WorkOrder::decision_payload(graph_version)` 现算被签字符串：域分离格式里有 `stable_id`，subject 是这张订单的 `graph_version`。请求里的 `payload` 仍必填，必须与现算结果逐字相同，否则 `BadRequest`。一致之后才做原来的四层校验。工件与自带 payload 彼此一致、但订单的 `stable_id` 或 `graph_version` 不同，会被拒绝。换 `run_id` 不改变绑定。`approve_decision` 仍不修改订单，`release` 的门控仍不读决策。

编排侧 signal 载荷是 `{approver, role, artifact}`。workflow 在 `release` 之前调用引擎 `approve_decision`。引擎不过时 `ApplicationError.type` 取 `error.kind`（`Decision` 或 `BadRequest`），不写 `approved_by`，也不 `release`。引擎接受之后，审批人角色留在决策工件的 `role` 字段上，并进入 workflow 结果。轨迹日志的 `state_after` 改读 `resp["order"]["state"]`，`missing` 改读 `error.gate.PreconditionsNotMet.missing`。`tag` 由编排写成这次的 op，不再恒为 null。

## 步骤

1. 先写失败测试 `approve_decision_rejects_matching_artifact_on_the_wrong_order`。修复前运行：工件按 `sid-f905…` / `g-3` 签好，请求换成 `stable_id = sid-other` 且仍带原 payload，引擎返回 `ok: true`。这是红队说的「一份决策工件可对任意工单通过」。
2. `approve_decision` 改为现算 payload 并拒绝不一致的自带字符串。原有正例、只读角色拒绝、哈希不匹配、契约样例改绑到现算 payload。哈希不匹配仍是 `Decision::PayloadHashMismatch`，但只在自带 payload 已经等于订单现算值、工件却签在别的字符串上时出现。
3. `orchestration/workflow.py`：signal 登记审批人、角色和决策工件；`release` 前调用 `approve_decision`；轨迹改读真实响应键。`ENGINE_BIN` 在 `CARGO_TARGET_DIR/debug/af-engine` 存在时用它（`env.sh` 把 target 放在仓库外）。
4. `orchestration/test_approval_gate.py` 用本机 temporal CLI 的 dev server 跑三条路径，并直接打引擎核对轨迹键。

测试数字：`af-engine` 集成测试 25 → 26，新增 1 个（上面的拒绝测试）。单测仍是 4（`version::tests`），`main` 单测仍是 0，文档测试仍是 0。`af-workorder` 源码与测试未改。本次跑到的数字是单测 49、集成 13（calendar 3、identity 4、json 快照 3、lifecycle 3）。

## 证据

修复前（红），只跑新测试：

```text
source /data/af-factory/env.sh
cargo test -j 4 -p af-engine --test engine approve_decision_rejects_matching_artifact_on_the_wrong_order -- --exact
# test approve_decision_rejects_matching_artifact_on_the_wrong_order ... FAILED
# assertion `left == right` failed: 工件对、订单身份错必须拒绝
# left: Bool(true)
# right: false
# 响应 ok:true，order.stable_id 为 sid-other
# test result: FAILED. 0 passed; 1 failed; 25 filtered out
```

修复后：

```text
source /data/af-factory/env.sh
cargo check -j 4 -p af-engine
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s
# exit 0

cargo fmt -p af-engine
# 无输出，exit 0

cargo clippy --all-targets -j 4 -p af-engine -- -D warnings
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.32s
# exit 0，无警告

cargo test -j 4 -p af-engine -p af-workorder
# af-engine lib: ok. 4 passed; 0 failed
# af-engine tests/engine.rs: ok. 26 passed; 0 failed; 0 ignored; 0 measured
# af-workorder lib: ok. 49 passed; 0 failed
# calendar_sla 3、identity 4、json_snapshots 3、lifecycle 3，均 0 failed
```

编排侧（本机 temporal dev server，CLI 1.9.1，Server 1.32.0）。`WorkflowEnvironment.start_local(dev_server_existing_path=/data/af-factory/temporal)`，Python 用 `/opt/gpumachine/temporal-qw-worker/.venv`（temporalio 1.34.0）：

```text
source /data/af-factory/env.sh
cargo build -j 4 -p af-engine --bin af-engine
/opt/gpumachine/temporal-qw-worker/.venv/bin/python orchestration/test_approval_gate.py
# trace keys ok
# unapproved release rejected
# Temporal Server: localhost:40117（in-memory）
# approver role recorded
# approved release proceeded
# ok
# exit 0
```

轨迹日志摘录（`data/engine_calls.log`，运行时文件，未入库）：

```text
{"tag":"release","op":"release","ok":false,"state_after":null,"missing":["LineVersionNotBound","BudgetVoucherNotIssued"]}
{"tag":"approve_decision","op":"approve_decision","ok":true,"state_after":"Created","missing":null}
{"tag":"complete","op":"complete","ok":true,"state_after":"Completed","missing":null}
```

手工再跑一遍的步骤：

1. `source /data/af-factory/env.sh`
2. `cargo build -j 4 -p af-engine --bin af-engine`（二进制在 `$CARGO_TARGET_DIR/debug/af-engine`）
3. `/opt/gpumachine/temporal-qw-worker/.venv/bin/python orchestration/test_approval_gate.py`
4. 退出码 0，且依次打印 `trace keys ok`、`unapproved release rejected`、`approver role recorded`、`approved release proceeded`。

三条路径对应：没有通过本单 `approve_decision` 时订单停在 `Created` 且 trace 无 `released`，对不上 `stable_id` 的工件以 `ApplicationError.type = Decision` 失败；工件按本单 `stable_id` + `g-bridge` 签好后 workflow 走到 `Completed`，history 含 `Created → Released`；结果里 `decision_artifact.role` 与 `approver_role` 都是 `Approver`，`actor_id` 是 `user-approver`。

## 反馈

1. 现算 payload 的 subject 就是 `graph_version` 原文。`decision_payload` 已经用长度前缀隔开 `stable_id` 和 subject，所以图版本不必再包一层。`run_id` 仍不进入被签字符串，与 af-workorder 已验证的「换 run 不改变决策哈希」一致。
2. 自带 payload 不一致是协议层 `BadRequest`，不是 `PayloadHashMismatch`。四层校验只对着现算字符串做。这样「工件和自带 payload 互相一致、订单是另一张」不会被误报成工件内部哈希错误。
3. `release` 仍然不读决策。审批和放行是编排依次调用的两步。把「已审批」写进 `release` 的缺失清单会推翻已验证的门控语义。
4. 引擎接受任何库所接受的 `Decision`，包括 `Reject`。本进程不另加「必须是 Approve」。角色是否够格仍由四层校验的第②③层决定。
5. 轨迹的 `missing` 只在 `PreconditionsNotMet` 上有值。`IllegalTransition` 没有这份清单，`missing` 为 null 是对的。`state_after` 在拒绝响应里也是 null，因为拒绝不带 `order`。
6. `orchestration/client.py` 仍把 `approve` 发成一个字符串。那个文件不在本任务的改动范围里。新 signal 要字典 `{approver, role, artifact}`，集成测试直接这样发。

## 遗留

- `orchestration/client.py` 的 `approve <approver>` 还是裸字符串，跟新 signal 对不上。要沿用 client 的话，得另改那个文件。
- 决策工件仍是 `ed25519-placeholder`。本次没有改验签。
- `forge_artifact` 里原有的逐秒 `asyncio.sleep` 没动。它是活动心跳，不是审批门的时钟。审批门不 sleep。
- workflow 在 `run` 里才 `import identity`，sandbox 会警告「Module identity was imported after initial workflow load」。这是原有写法，测试没有因此失败。
- `data/engine_calls.log` 与 `data/ledger-a.json` 是这次实测写出的运行时文件，没有入库。

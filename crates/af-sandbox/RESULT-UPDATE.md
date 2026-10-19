# RESULT-UPDATE — W5A af-sandbox 沙箱客户端

## ① 结论

**通过**。新建 `crates/af-sandbox`：沙箱会话语义层 `Session::open` / `exec` / `collect` / `close`。资源限额（cpu 毫核、内存字节、超时毫秒）由调用方注入。`exec` 用 `af_credential::denial` 要求工具白名单放行 `sandbox:exec`。沙箱并发维预算由调用方在调用前做成 `BudgetDecision`，本 crate 只接受或原样拒绝，不调用 af-meter。后端不可用时 `exec` 返回 `SandboxDown`。`resume` 或后端代际变化之后，旧句柄再执行、再取产物、再关闭都是 `StaleEpoch`，不向后端探活，也不把旧 id 重新打开。

`cargo test -p af-sandbox` 全绿：库内 5 + 集成 21 + 文档 1，共 27。`cargo clippy --all-targets -p af-sandbox -- -D warnings` 退出码 0。直接依赖增加 `biscuit-auth 6.0.0`（锁里已有，与 af-credential 同版），没有新包，没有改版本。其它 crate 的源码没有改。

真实 CubeSandbox 没有进 `cargo test`。这个克隆里没有 `poc/poc-sandbox`，本机也没有在本任务里拉起 v0.7.3。

## ② 步骤

1. 读了 `af-credential/RESULT.md`、`af-effect-gateway/RESULT.md`、`af-meter/RESULT.md`。白名单判定沿用已验证的 `denial`：衰减删不掉 `tool(...)` 事实，只看 `inspect` 会把已收窄的凭证放行。一次判定只注入一个 `request_tool`。累计预算仍不在凭证里。纪元围栏沿用网关的「宿主持有当前纪元，句柄携带签发纪元，`resume` 加一」。并发槽沿用 meter 的 `Concurrency`（`acquire` / `release`），不走 `try_consume`。
2. 新建 crate。工作区 `members = ["crates/*"]`，根 `Cargo.toml` 不用改。`Cargo.lock` 只多了 `af-sandbox` 这个包节点，依赖 `af-credential`、`biscuit-auth`、`thiserror 2.0.21`。
3. `Session::open(host, image, resources)` 把限额原样交给后端。`exec(cmd, credential_scope, budget)` 的顺序是：纪元 / boot 围栏、后端可用性、会话超时、凭证白名单、预算决策、后端执行、后端报告的耗时。`collect` 不查凭证和预算，缺一个路径就整次失败。`close` 在同一纪元里重复调用是成功，且只打一次后端。
4. 凭证：`CredentialScope` 借用 `Biscuit`。`allow_sandbox_exec` 先 `inspect` 记下事实是否还在，再 `denial`。拒绝带上原始 `Denial`。`denial` / `inspect` 的 `Err`（限额、损坏）是 `CredentialFault`，不放行。不调用 `evaluate`。现有 Cedar 策略只写了 `git` 和 `test`，没有 `sandbox:exec`。`spend_cents` 仍是凭证的单次上限，不是并发槽。
5. 预算：`decide_concurrency` 问 `ConcurrencyBudget` 钩子，请求维度是 `SandboxConcurrency`、槽位 1。`exec` 不再问钩子。只有「并发维且槽位 ≥ 1」的 `Charged` 能执行。拒绝、零槽、其它维都是 `BudgetRejected`，决策原样在错误里。
6. 失败关闭与纪元：`SandboxHost::boot` 的 `boot_id` 由调用方按进程注入，本 crate 不取随机数。`resume` 纪元加一并 `retire_all`。后端 `generation` 变化也把纪元加一。后端宕着时 `resume` 仍然推进纪元，然后返回 `SandboxDown`。`reattach` 只查本宿主的表：boot 不同是 `StaleEpoch`；boot 和纪元都能对上但没有这个 id 是 `SessionGone`，不向后端认领。
7. 时钟：`ManualClock` / `SystemClock`。超时只用 `Clock::now_ms` 和后端报告的 `elapsed_ms`。到达上限即 `TimedOut`，不睡眠。加法溢出无法表示截止点时按超时拒绝。
8. 提交前在本目录执行了 `cargo fmt -p af-sandbox`、`cargo clippy --all-targets -p af-sandbox -- -D warnings`、`cargo test -p af-sandbox`。每条 cargo 之前都 `source /data/af-factory/env.sh`，并带 `--offline -j 4`。

## ③ 证据

依赖没有新包。`Cargo.lock` 的 diff 只有 `af-sandbox` 0.1.0 的 9 行，`biscuit-auth` 仍是锁里的 6.0.0。

```
$ source /data/af-factory/env.sh && cargo clippy --all-targets -p af-sandbox --offline -j 4 -- -D warnings
    Checking af-credential v0.1.0
    Checking af-sandbox v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.77s
```

退出码 0。编译 af-credential 时仍有一条既有 note：`proc-macro-error2 v2.0.1` future-incompat（cedar / biscuit 传递依赖，`af-credential/RESULT.md` 已记录）。它不是本 crate 的 warning，`-D warnings` 没有因此失败。

```
$ cargo test -p af-sandbox --offline -j 4 -- --test-threads=8
running 5 tests (unittests src/lib.rs)
test result: ok. 5 passed; 0 failed
running 21 tests (tests/semantics.rs)
test result: ok. 21 passed; 0 failed
Doc-tests af_sandbox
test result: ok. 1 passed; 0 failed
```

27 个全是新测试。基线是没有这个 crate。库内 5 个：预算接受 / 拒绝原样 / 错误维度与零槽、超时边界含溢出、空白镜像与限额。文档测试 1 个：`decide_concurrency` 只问钩子。集成 21 个覆盖：限额与时钟原样注入、白名单放行且钩子只调用一次、非零退出码仍是完成的 exec、缺工具不打后端、衰减后 `inspect` 仍有 `sandbox:exec` 但 exec 拒绝、命名空间与 `spend_cents` 上限、预算拒绝原样带回、错误维度与零槽、`SandboxDown` 优先于坏凭证、时钟超时不打后端、后端报告耗时到达上限则丢掉输出、`resume` 围栏后新会话可用、后端宕着 `resume` 仍作废旧句柄、代际变化不经 `resume` 也围栏、别的宿主不认领活会话、关闭幂等、旧纪元优先于宕机和坏凭证、收集产物与缺路径失败、重启后 collect 是 `StaleEpoch`。

`rg -n 'sleep|thread::sleep' crates/af-sandbox/src crates/af-sandbox/tests` 无匹配。

## ④ 反馈

1. 工具白名单要走 `denial`，`fact_present` 只是拒绝上的观察字段。衰减后的事实还在，不能当放行条件。这是 af-credential 已经钉死的结论，集成测试 `attenuated_token_keeps_fact_but_exec_is_denied` 再钉了一次。
2. `exec` 不调用 `evaluate`。Cedar 策略文本仍然只有 reviewer/`test` 和 builder/`git`。把交集判定塞进沙箱执行会让每条 `sandbox:exec` 都失败，而且失败原因会混进角色策略。角色策略要单独加，不在这个 crate 里改 af-credential。
3. 并发预算和凭证里的 `spend_cents` 是两回事。前者是槽，对应 meter 的 `Concurrency::acquire`；后者是单次美分上限。`BudgetDecision` 的 `decided_at` / `release_at` 单位跟 meter 的调用方约定走，本 crate 不把它们换算成超时毫秒，到点也不替调用方 `release`。
4. 错误不要并成一个字符串。`SandboxDown`、`StaleEpoch`、`ToolDenied`、`BudgetRejected`、`TimedOut` 分开。已经钉住的优先级：围栏，然后可用性，然后会话截止，然后凭证，然后预算，然后后端，然后后端报告的耗时。
5. 句柄跨重启要同时有纪元和 `boot_id`。只重置内存纪元到 0 时，相同 boot 加空表会得到 `SessionGone`，不会去后端把旧容器认领回来。调用方若要跨进程延续纪元，需要自己保存计数器并在启动时 `resume`，同时换新的 `boot_id`。
6. 真实 CubeSandbox 适配器实现 `SandboxBackend` 即可：服务不可达时 `available` 为 false；进程重启必须改变 `generation` 并丢掉旧会话。不要把这条线路放进 `cargo test`。

## ⑤ 遗留

- 没有 CubeSandbox 线协议实现，也没有在本任务里对 v0.7.3 做真实调用。`poc/poc-sandbox` 不在当前克隆中。
- 纪元和会话表只在内存里。本 crate 不落 SQLite。进程退出后的 fencing 靠调用方的 `boot_id` 和它自己的纪元存储。
- 不调用 af-meter。槽的扣减和归还仍在调用方。`exec` 成功不会自动 `release`。
- `Drop` 不关闭会话。调用方没 `close` 时，后端会话要等 `resume` 的 `retire_all`，或者后端自己重启。
- 宿主用一把 `Mutex` 串行访问后端。没有做多线程压测。
- `proc-macro-error2` 的 future-incompat note 仍在，来自 af-credential 的传递依赖。本任务没有升级 biscuit 或 cedar。

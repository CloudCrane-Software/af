# RESULT.md — poc-credential 凭证签发与权限求交

crate：`crates/af-credential`（workspace：`af/`）  日期：2026-10-19

## ① 结论

**通过**。`cargo test -j 4 -p af-credential` 全绿（8/8：衰减组 4 + 交集矩阵组 4）。
biscuit-auth 6.0.0 与 cedar-policy 4.14.0 均可直接使用、纯 Rust、无 openssl，全依赖树冷编译约 62s（-j 4）。

## ② 做了什么

1. **签发（biscuit）**`src/credential.rs`：`issue_root(RootSpec{tools:["git","test"], namespace:"line-a", budget_cents:1000})`——authority 块带事实 `tool("git")`、`tool("test")`、`namespace("line-a")`、`budget_cents(1000)` + 根块护栏 check `spend_cents <= budget_cents`。注意 biscuit 6 **删除了 Float**，预算以“美分”整数编码（usd 10.0 → 1000）。
2. **衰减**：`attenuate(token, ChildSpec{tools:["test"], spend_cap_cents:500})` = holder 侧 append 一个块，只加两条 check：`request_tool ∈ ["test"]` 与 `spend_cents <= 500`。测试断言：衰减后反序列化，block_count=2，**block0 的 `tool("git")` 事实原封不动**（append-only，删不掉），但行为上 `authorize(git)` 被衰减 check 拒绝、`authorize(test)` 通过——旧权限拿不回来。另测预算衰减（根 spend≤1000 ∩ 子 spend≤500 → 800 拒/300 过）、跨命名空间拒、字节翻转防篡改、异根公钥验签失败。
3. **Cedar 策略**`src/policy.rs`：`permit(Role::"reviewer", Action::"test", Line::"line-a")` 与 `permit(Role::"builder", Action::"git", Line::"line-a")`，schema-less 判定（Request::new 五参数平铺形式）。
4. **交集判定器**`src/judge.rs`：`evaluate(token, AccessQuery{role,tool,namespace,spend_cents})` → `Verdict{allow = cedar_allow && biscuit_allow}`，另报 `token_carries_capability`（从凭证事实提取）。四用例矩阵全过：双方允许→放行；Cedar 允许但衰减把 git 封死→拒（且断言 git 事实仍在 token 里）；凭证有 git 但 Cedar 对 reviewer 无 permit→拒；双方都无（deploy）→拒。
5. 词表以 crate 常量钉死（`F_TOOL`/`F_NAMESPACE`/`F_BUDGET_CENTS`/`F_REQUEST_*`/`F_SPEND_CENTS`），验证方事实与凭证事实用不同谓词名区分（`request_tool` vs `tool`），防同名谓词互相污染。

## ③ 证据

命令（均在 `af/` 根）：
`cargo fetch` → biscuit-auth 6.0.0 / cedar-policy 4.14.0（5.4s 下载完）；
`cargo test -j 4 -p af-credential`：

```
running 4 tests (tests/attenuation.rs)
test budget_attenuation_caps_spend ... ok
test tampered_token_fails_verification ... ok
test issue_and_verify_root_token_facts ... ok
test attenuation_keeps_facts_but_revokes_git ... ok
test result: ok. 4 passed; 0 failed
running 4 tests (tests/intersection.rs)
test case1_both_allow_pass ... ok
test case4_neither_allows_deny ... ok
test case3_token_has_but_cedar_denies ... ok
test case2_cedar_allows_but_token_lacks_deny ... ok
test result: ok. 4 passed; 0 failed
```

拒绝原因日志（`-- --nocapture`，biscuit 精确指出是哪条 check 拦的）：

```
denied git@line-a: ... allow policy matched (policy index: 0), and the following
  checks failed: Check n°0 in block n°1: check if request_tool($t), ["test"].contains($t)
denied test@line-a: ... Check n°1 in block n°1: check if spend_cents($s), $s <= 500
denied test@line-a: ... Check n°0 in block n°0: check if spend_cents($s), budget_cents($v), $s <= $v
```

依赖与耗时：`cargo tree -p af-credential` 一层 = anyhow + biscuit-auth 6.0.0 + cedar-policy 4.14.0；全树 `grep -c 'openssl|ring|rustls|native-tls'` = **0**（签名走 curve25519-dalek 纯 Rust）。首次 `cargo build -j 4 -p af-credential`（含全部依赖冷编译）61.8s wall；仅本 crate clean 重建 2.46s。仅一条 warning：proc-macro-error2 future-incompat（cedar 传递依赖）。

## ④ 对最终方案的反馈

1. **词表绑定适配度：biscuit 中低、Cedar 高**。biscuit 的谓词名/符号是自由字符串，6.x 无任何 schema 机制，check/rule/policy 全是运行时解析的字符串 DSL——笔误编译期不报。正式方案必须像本 PoC 这样：业务层只见类型化 API（RootSpec/ChildSpec/AccessQuery），Datalog 字符串全部收进 af-credential 内部常量。Cedar 则有正式 schema（可对实体类型/属性做词表校验），本 PoC 走 schema-less，正式版建议接入 `cedar_policy::Schema`。
2. **预算不能住在凭证里**。衰减单调性：预算从 10 降 5 无法表达为“改事实”，只能叠加更紧的 per-call cap check；且 `spend_cents` 是验证方声明的事实，凭证管不住累计消耗。结论：累计预算真相源放 af-meter 账本，凭证只携带 per-call 上限（或预算主体绑定），网关按账本余量注入 spend。
3. **biscuit check 是存在性查询**：验证方若注入多个 `request_tool`，有一个落在白名单即通过。af-effect-gateway 必须保证一次请求恰好一个 request 事实（本 PoC 按约定实现，biscuit 层无法强制）。
4. **接口建议**：对外只暴露 `issue_root / attenuate / serialize / deserialize_verify / evaluate`（bytes + PublicKey 进出，KeyPair 不出 crate）；Cedar 策略文本应作为可独立演进产物（文件/配置）而非代码内 const；词表加一致性测试（Cedar action 名 ⊆ biscuit tool 值域）。
5. 工程坑备忘：builder 的 check/policy 只收 `&str` 不收 `String`（`TryFrom<&str>`）；biscuit 6 无 Float → 预算词表统一 `*_cents: i64`，需与 af-meter 对齐；`authorize()` 的“拒绝”= `Err(Token::FailedLogic)`，与其他错误要分流（本 PoC 已分）。
6. **biscuit 默认执行限额不可依赖（2026-10-19 补测发现）**：默认 `RunLimits{max_facts:1000, max_iterations:100, max_time:1ms}`——**1ms 墙钟**在满载并行（`cargo test -j 6` 全 workspace）下被 CPU 争用挤爆，`budget_attenuation_caps_spend` 间歇性以 "Reached Datalog execution limits" 硬错误 panic（单跑必过、满载偶挂的负载敏感 flake；根因经 biscuit 6.0.0 源码 `datalog/mod.rs` Default 实现定位）。已修：`credential.rs` 显式 `verify_limits()`（facts/iterations 维持默认，max_time 放宽 50ms），`authorize()` 走 `set_limits`、`inspect()` 走 `query_with_limits`。修复后满载全量两轮全绿。设计输入：**内核一切授权判定必须显式配置执行限额**，限额超限是“真错误”不是“拒绝”（fail-closed 但会炸调用方），这条写进波次 1 的词表对齐评审。

## ⑤ 遗留问题

- 累计预算治理未做（需 af-meter 账本 + “预算主体绑定”式凭证，本 PoC 只做了 per-call cap 衰减）。
- Cedar 未接 schema（schema-less）；接入后需同步维护 schema 文件并重测 Request 校验路径。
- biscuit root key 轮换（RootKeyProvider / root_key_id）与密钥分发未验证。
- third-party block（跨域代理签发）API 在 6.0 有变动（`unsafe_deprecated_deserialize` 出现），内核暂不需要，未碰。
- proc-macro-error2 future-incompat 警告来自 cedar 传递依赖，等上游修即可。

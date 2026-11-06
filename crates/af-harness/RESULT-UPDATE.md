# RESULT-UPDATE.md — H1 af-harness

## 结论

通过。新建 `crates/af-harness`，统一执行器按四元组数据跑 Run。`cargo test -p af-harness` 30 个测试全绿，clippy `--all-targets -D warnings` 没有本 crate 的警告。

## 步骤

1. 读了 af-credential、af-meter、af-trace、af-stop、af-effect-gateway 的 RESULT 和公开签名，以及 af-station 的 WAL 开库配方。没有改这些 crate。
2. 规格信封、字面渲染、注入钟、错误类型。
3. OpenAI / Anthropic 适配器和 crate 内 HTTP stub。HTTP 客户端用 ureq 2.12.1（tls / rustls），不读环境代理。
4. Run 循环：凭证求交 + `denial`、副作用走网关、止损、按轮扣预算、trace、`harness.db` 续跑。
5. `cargo fmt`、`cargo clippy --all-targets -p af-harness -- -D warnings`、`cargo test -p af-harness`。实现提交 `845e5a9`。

## 证据

```
$ source /data/af-factory/env.sh && cargo test -j 4 -p af-harness -- --test-threads=8

running 30 tests
...（30 条均为 ok，名单见 RESULT.md ③）
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s

Doc-tests af_harness
test result: ok. 0 passed; 0 failed
```

```
$ source /data/af-factory/env.sh && cargo clippy -j 4 --all-targets -p af-harness -- -D warnings

    Checking af-harness v0.1.0 (/data/af-factory/h1/crates/af-harness)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.65s
```

没有 `warning: ... af-harness`。cargo 打印了依赖 `proc-macro-error2 v2.0.1` 的 future-incompat 提示，不是本 crate 的诊断。

测试数：本 crate 从 0 增到 30。新增覆盖渲染、两种协议解析、工具路由、幂等续跑、预算耗尽即停、指纹止损、凭证越权拒绝、WAL 声明分叉。

依赖写进了实现提交说明：ureq =2.12.1（同步 HTTP，rustls）；biscuit-auth =6.0.0（公开签名需要，不升版本）；af-workorder 路径依赖（网关签名要 `WorkOrder`）。

## 反馈

- 不要用 `af_credential::evaluate` 做工具放行。求交后 `attenuate`，再用 `denial`。空交集用哨兵名，避免退回父凭证全量事实。
- 续跑必须递交首启时的预算声明。当前余额在 `harness.db` 的 `current` 里。
- 明文密钥不进规格、不进 Debug、不进 trace。`key_ref` 拒绝 `sk-` 前缀。
- 完成信号是子串，会命中更长的词。这是有意不引入正则库。

## 遗留

- 真实供应商冒烟只编译了 `examples/smoke.rs`，本环境没有配 `AF_HARNESS_*`，没有打过外网。
- 同一目录不能并发两个 `Opened`。
- 计数预算只在单元测试里扣过；Run 验收用美分账本。
- 网关撤回窗口为 0，执行器没有撤回入口。

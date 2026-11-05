# 指纹（feat/risk3-afmeter-concurrency，续完会话）

- 基线：`origin/product/main` = `bc1ef7d9eb2e1f3466536f8c9e58af75249429a3`
  （Merge feat/t7-1-f2-dispatch-runner）
- 工作树：`/home/pan-ding/af-dev/af/wt/risk3-afmeter-concurrency`
- 分支：`feat/risk3-afmeter-concurrency` 仍指向基线；**未 commit / 未 push**
- 改动面：
  - `crates/af-meter/src/lib.rs`（+183/−24，生产 fail-closed + 6 条钉面测试）
  - `poc/poc-risk3-afmeter/`（本目录：侦查/先红后绿/四门）
- 非交付：`.offload/`、`out/`（会话开始前或上轮残留，未纳入本包）

## 文件 sha256

```
43d9453e7302769d9a9ce7be5700450492ad3c7a01662ab1b4ee447ddffbbe16  crates/af-meter/src/lib.rs
9034b7caf7b0f1628ac153033d9b2a4098af5abc68490764d5fdcc50f9763c7e  poc/poc-risk3-afmeter/README.md
82ecbcd8bc6472af3c850c2e82694fc8e5659735dbd641cf9d01f19e834d7144  poc/poc-risk3-afmeter/GAPS.md
3ce0af470275e7b4e4a36e3676724d7dc0fcdd285dcc3c1b93e2ed91df93305d  poc/poc-risk3-afmeter/before/probe-before.txt
eaad3ee4ff56227ac8a5c40966781c70ec40e73270a081c113cd58a6e91dbac0  poc/poc-risk3-afmeter/before/lib-tests-red.txt
dd223e5cf7167d70deff37ccbfdc81d8a49af522ca9f309de8f11cbaae3620da  poc/poc-risk3-afmeter/before/pin-tests-red-filtered.txt
e17d598f80fb7dc363dacfd5a841d53e1a11d5843d8abcef92454061f0beb689  poc/poc-risk3-afmeter/before/head-deduct-noop.txt
a8972ac462487db800b0f81054ada9f0427a7c97f117d3d5c09a0dfc225b129b  poc/poc-risk3-afmeter/after/lib-tests-green.txt
8a61d8ea78b86fba7631460a998f29482ac686518052dc55d8296827c98eb7db  poc/poc-risk3-afmeter/after/crate-tests-green.txt
da9e9c57d7829cd4a6f0e2d47fc8de98eb9493da3803d21dfc38032bb9df7be7  poc/poc-risk3-afmeter/after/gates.txt
9b798ec6b90b87740cc153d4cf24c546f0a64c37d4f49ec95739ad08037d1185  poc/poc-risk3-afmeter/after/lib-tests-green-summary.txt
f15b9edaf25d1010a841162e687291a3b64b5696d1d55c484f4a590c44770341  poc/poc-risk3-afmeter/raw/gate2-test-workspace.log
b90d185b0df4e2bdc71d298fb68f6c07ac97a1802510134a2958f5ee82446298  poc/poc-risk3-afmeter/raw/gate3-clippy.jsonl
fa4a05ff387f540573f26e66129997d5643c0720e535ec38097ae64833e634c1  poc/poc-risk3-afmeter/raw/gate3-clippy-gate.log
268b79660a1a618850e3e19ced61d5d5368f5f19f3b5d1f0d31ef8dc792877c2  poc/poc-risk3-afmeter/raw/gate4-gate-inputs.log
```

`raw/gate1-fmt.log` 为空文件（fmt --check 无输出，sha256 = e3b0c442… 空串）。
clippy-baseline.txt sha256 `a48639e75b5203fef7e1ffc7026b8f0d9de20f3ce862fa3988a1ec6b49326ba4`（钉 0 数据行，未变）。

## 门读数汇总

| 门 | 结果 |
|---|---|
| 1 fmt | exit 0 |
| 2 test | 任务书钉 1578/0/9 → 终跑 **1592/0/9**（+14 = 本施工 +6 + HEAD 已含 risk5 +8），exit 0 |
| 3 clippy | `-D warnings` exit 0；json compiler-message errors=0 warnings=0；`clippy_gate.py` PASS（基线 0 签名零动） |
| 4 gate_inputs | exit 0（14 件钉对账一致） |

先红后绿：`--lib` **32 passed / 5 failed**（HEAD 生产 + 钉面）→ **37 passed / 0 failed**（修复恢复后）。

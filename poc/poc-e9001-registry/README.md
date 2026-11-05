# POC · E9001 组件注册供给批

- 基线：`origin/product/main` = `bb7d1e3ac814280f514eab05dcfe663b15dbdbb2`
- 分支：`feature/af-dsl-component-registry-e9001`
- 工件：`gap-list.json` / `registry-diff.md` / `raw/compile-all.before.json` / `raw/compile-all.after.json`

## 四段链

| | compilable | pass_four_stage |
|---|---|---|
| 前（生产注册表 63 条） | 44 | **10** |
| 后（119 条） | 44 | **44** |

34 条 E9001 bind 红全部消除。残余 0。

## 选 a / 选 b

- **选 a 真注册 56 名**：每条 `impl_ref` 指向已接线 builtin 或真实 model_route 端点，不是空转声明。闭包与策略对照见 `gap-list.json`。
- **选 b 降级 3 组**：`pkg.fs.workdir_rw` / `pkg.net.proxy_allowlist` 从 uses.tools 卸下（能力面，escape 已记账）；`pkg.mem.*` 在 st-b-C042 降为已注册 `pkg.sweng.locate/view/edit`。

## 五门

| 门 | 读数 |
|---|---|
| fmt | `cargo fmt --check --all` exit 0 |
| test | workspace 1620 passed / 0 failed / 9 ignored（基线 1617/0/9；+3 本批断言。p3_judge 须 `LC_CTYPE=C.UTF-8` + `env -u BASH_FUNC_which%%`） |
| clippy `-D warnings` | exit 0 |
| clippy_gate | PASS（0 签名 / 基线 0 签名）；json error=0 warning=0 |
| gate_inputs | 14 件钉对账一致（本批未改被钉件） |
| strategies_check | exit 0；44/0/17（compilable/needs_syntax/not_applicable） |

判据面零弱化：`ci/strategies_check.py` / `ci/criteria.yaml` / gate_inputs 钉未改。

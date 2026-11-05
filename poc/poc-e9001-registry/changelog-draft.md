# changelog-draft · E9001 组件注册供给批

- 主代理追加口径：2026-11-05 波次 E9001（行首时间以 计划/变更日志.md 实际追加为准）。

## 施工

1. 基线 fetch `origin/product/main` = `bb7d1e3`；新分支 `feature/af-dsl-component-registry-e9001`。
2. seam `compile-all . ./af/packages` 实跑：10/44 过全链，34 条断 bind（E9001）。
3. 选 a：注册表 +56（route 7 / tool 30 / unit 19），`impl_ref` 指向已接线 builtin 或真实端点；`tool_impl_name` 把新 tool 名落到 env/locate/view/edit 四臂。
4. 选 b：st-C122 / st-b-C079 卸 `pkg.fs.workdir_rw` / `pkg.net.proxy_allowlist`；st-b-C042 `uses.memory` 的 `pkg.mem.*` 降为 `pkg.sweng.locate/view/edit`。
5. 计数测试 63→119；digest 钉改为与 live registry 对账。

## 机械验证读数

- seam 四段链：10/44 → **44/44**
- `python3 -B ci/strategies_check.py`：exit 0，44/0/17
- `cargo test -j4 --workspace`：1620/0/9（基线 1617/0/9，+3 本批断言）
- fmt / clippy `-D warnings` / clippy_gate PASS / gate_inputs 14 件一致

## 边界

- unit 别名共用 `pkg.sweng.test_unit` 真实现（语义是判定单元，不是各自领域的独立判据器）。
- tool 别名共用 locate/view/edit/env 四臂（merge 策略工具落 edit；lint/test/runtime 落 env）。
- store retrieve 的 `component:` 字段 bind 不核（pkg.skill.loader / pkg.ir.bm25_ce / pkg.mem.grep 仍在 snippet 内）。
- p3_judge 在带 `BASH_FUNC_which%%` 的交互 shell 下会红；`env -u` 后 5/0/1。
- 未改判据面；未新建空包目录。

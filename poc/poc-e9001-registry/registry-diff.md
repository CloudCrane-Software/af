# 注册表 diff 摘要 · E9001

- 基线 digest：`63c3478e9321e1c58318620a592e9953ef34cee3a5d19ea4c3300a722fafe15b`（63 条）
- 扩容 digest：`f5a6de7c9faa7859ab2b8c206a7517b996e770a78099baea9ac7cd1e565f7103`（119 条）
- 增量：+56（model_route 7 + tool 30 + unit 19）

## 落位

| 文件 | 前 | 后 | 增量 |
|---|---|---|---|
| `af/packages/components.json`（根） | 44 | 96 | +52（route 7 + tool 26 + unit 19） |
| `af/packages/sweng/components.json` | 5 | 9 | +4（lint / edit_udiff / edit_whole / edit_linted） |
| adaskit / autogen / reportkit / swarmkit | 2+5+6+1=14 | 14 | 0 |
| **合计** | **63** | **119** | **+56** |

## 声明纪律（fail-closed 未动）

- `ComponentDecl` 仍 `deny_unknown_fields`：缺 `effect_class`（tool）/ 缺 `endpoint`（model_route）装载即拒。
- 类不匹配仍 E9003（`registry_decl_test::class_mismatch_against_repo_registry_is_rejected`）。
- 新 tool 的 `impl_ref` 一律 `executor:builtin/<已接线四臂>@af-execline`。
- 新 unit 的 `impl_ref` 一律 `executor:builtin/pkg.sweng.test_unit@af-execline`（`drive_check_node` 按锚分发，真跑 test_unit）。
- 新 model_route 端点与 `route.main` / `route.cheap` 同形（GLM-5.3 / GLM-5.3-Flash，`key_ref=zhipu-coding-plan`）。

## 执行别名

`af-execline::pkg::sweng::tool_impl_name`：新 tool 名映射到 env / locate / view / edit 四条已接线臂；`node_tool_fn` 按映射名分发。G4 十五件身份名不变。

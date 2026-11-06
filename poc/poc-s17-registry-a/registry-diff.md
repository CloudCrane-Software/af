# 注册表 diff 摘要 · S17 供给注册批 A

- 基线 digest：`f5a6de7c9faa7859ab2b8c206a7517b996e770a78099baea9ac7cd1e565f7103`（119 条）
- 本批 digest：`4a49856fb7d650258a9f5fbb5c122dcac7031c8f959b98d61edde6c3c04f0d44`（127 条）
- 增量：+8（unit 7 + tool 1）

## 落位

| 文件 | 前 | 后 | 增量 |
|---|---|---|---|
| `af/packages/components.json`（根级） | 96 | 104 | +8 |
| `af/packages/sweng/components.json` | 9 | 9 | 0 |
| adaskit / autogen / reportkit / swarmkit | 14 | 14 | 0 |
| **合计** | **119** | **127** | **+8** |

## 声明纪律（fail-closed 未动）

- `ComponentDecl` 仍 `deny_unknown_fields`：缺 `effect_class`（tool）/ 缺
  `endpoint`（model_route）装载即拒——新 tool `pkg.iface.mcp_client@1` 带
  `effect_class: external_once`。
- 类不匹配仍 E9003：本批施工中被该门实证抓到一次——st-b-C098 的 loader 原按
  `uses.tool` 引 `pkg.review.scope_diff@1`（注册类为 **Unit**，st-C122 用于
  gate 节点），bind 段 E9003 拒；改引已注册 tool `pkg.searchkit.corpus@1`
  （st-C007/st-C009 同型 loader）后四段链全过。修法记入该条 snippet 的 intent。
- 7 个新 unit 的 `impl_ref` 一律 `executor:builtin/pkg.sweng.test_unit@af-execline`
  （E9001 同型锚：判定只取退出码与输出文本，零模型）。
- 新 tool `pkg.iface.mcp_client` 的 `impl_ref` 指向**新写的**记录型 builtin
  （`crates/af-execline/src/pkg/iface.rs`），不是空转声明：每次调用真发底层
  已接线工具并落 `{interface, tool_name, args_digest, ok}` 一行。

## 执行别名（未新增）

`af-execline::pkg::sweng::tool_impl_name`：`pkg.iface.mcp_client` 走 `other`
恒等臂（本名即执行面键）——包装器的底层工具名是运行期入参 `tool_name`，
不做静态别名改写。G4 十五件身份名不变；E9001 别名表不变。

## 模板面

`sweng/prompts/` +14 个 `.md`（本批 7 条 snippet 的新引用）+ `MANIFEST.sha256`
+14 行（闭集测试 `pkg_sweng_package.rs::
sweng_prompt_manifest_is_closed_over_files_on_disk` 逐件核对 sha256）。

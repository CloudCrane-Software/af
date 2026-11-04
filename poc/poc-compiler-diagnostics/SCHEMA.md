# af.diagnostics.v1 · 机读诊断产物 schema 字段说明

编译器诊断的机读 JSON 产物面（G2）。正本 = `crates/af-compiler/src/diagnostics.rs`
（serde 结构体即 schema；测试 `tests/diagnostics_test.rs` 的 golden 字面量冻结形状）。

## 顶层

| 字段 | 类型 | 语义 |
|---|---|---|
| `schema` | string | 恒 `"af.diagnostics.v1"`。向后不兼容的演进必须升版本号。 |
| `diagnostics` | array | 诊断条目，产生序（确定性：同输入同产物逐位一致）。空数组 = 本次运行无**已编码**诊断——**不等于运行成功**（见「空数组语义」）。 |

## 条目（8 键恒在场，无省略键）

| 字段 | 类型 | 语义 | 取值来源 |
|---|---|---|---|
| `code` | string | 稳定码，**逐字取自既有码表方法**，本产物面零新码 | af-dsl `ErrorCode::code()`（E0001-E0004 / E1xxx / E2xxx 全码族）；af-compiler `BindCode::as_str()`（E9001-E9003） |
| `severity` | string | 严重度；v0.1 恒 `"error"`（既有码族全部 fail-loud 拒绝语义） | 产物面常量 |
| `message` | string | 精确信息，逐字不改写 | af-dsl `Violation::message` / af-compiler `BindViolation::message`（stderr 人读面同一份） |
| `phase` | string | 产生阶段：`"load"`（装载拒绝，`af_dsl::load_str`）/ `"validate"`（语义校验，`af_dsl::validate[_with]`）/ `"bind"`（组件绑定，stage 3） | 产物面按管线位面赋值 |
| `location` | object \| null | 位置；库面恒 null，CLI 面盖章为 `{file, line, column}` | 见下 |
| `node` | string \| null | 相关对象标识——诊断命中的图内主体：E900x = 节点点号路径；af-dsl = `Violation::scope`（`root` / `<路径>.loop_body` / `<路径>.map_body`） | 上游字段原样 |
| `reference` | string \| null | 相关对象标识（E900x）——命中的组件引用串原样（如 `pkg.kit.hammer@1`） | `BindViolation::reference` |
| `rule` | string \| null | 规则名（snake_case，**非码**，与 code 并列方便检索）；仅 af-dsl 诊断有 | `ErrorCode::name()` |

## location（3 键恒在场）

| 字段 | 类型 | 语义 |
|---|---|---|
| `file` | string | 源文档路径（CLI 面已知的输入路径原样） |
| `line` | number \| null | **v0.1 恒 null**：af-dsl 校验器无 span 记账——明确缺省，**不编造位置** |
| `column` | number \| null | 同上 |

## 判定语义（不许弱化——本面只多一个出口）

- 诊断的**产生条件、拒绝语义、码值、message 逐字不动**：产物面是纯投影
  （`from_load_error` / `from_validate_report` / `from_bind_violations` 三个纯函数，
  零判定零改写）。
- 码表不变：产物里的 code 全部来自既有 `code()`/`as_str()`，**零新增码**。

## 空数组语义（消费方契约）

产物在源文档读取成功后的**每条**退出路径都会写（成功与失败）。空数组
`"diagnostics": []` 只说明「无已编码诊断」；**成败判定必须结合退出码**：
无码的操作性失败（组件/图清单装载失败等）同样落空数组 + 非零退出码。

## 出口

CLI：`af-compiler-plan [--registry <包根>] [--diagnostics-out <产物路径>] <yaml>`。
不给 `--diagnostics-out` 时行为与扩展前逐字节一致（产物不落盘）。
库面：`af_compiler::DiagnosticsFile`（`to_json()` / `write_to()`）。

## 稳定性

- 字段名/条目键序/枚举值由 `tests/diagnostics_test.rs` 三面冻结：golden
  字面量（逐字节）、键集断言、反序列化回环。
- 同输入 → 同产物（确定性）；条目序 = 上游产生序（af-dsl 校验器文档序 /
  bind 的 BTreeMap 确定序），无重排。

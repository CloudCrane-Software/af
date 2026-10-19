# RESULT-UPDATE.md — W4B DSPy IR schema v1

## ① 结论

**通过**。新建 L2 适配器侧 crate `af-dspy-ir`（不是内核抽象）。IR JSON 是带字段名的真相源，信封只接受整数 `version = 1`。缺字段名、签名/demo 坏引用、删除不存在的字段都显式失败。DSPy 状态 JSON 能互转：存档的 signature.fields 没有名字时，按签名表位置（先输入、后输出）补回。

`cargo test -p af-dspy-ir`：**17 通过 / 0 失败**（集成测试 17，库内单测 0，文档测试 0）。`cargo clippy --all-targets -p af-dspy-ir -- -D warnings` 无警告。未引入新的第三方 crate。

本克隆没有 `poc-dspy/RESULT.md`。schema 按任务里已验证的两条语义落地，并与当前 DSPy `Signature.dump_state` / `Signature.delete` / `Module.dump_state` 对齐：存档字段只有 `prefix` 和 `description`；`delete` 是 `fields.pop(name, None)`，缺席不报错。Rust 侧不沿用这两处静默行为。

## ② 步骤

1. 读本克隆：没有 `crates/af-dspy-ir`，也没有 poc-dspy 的 RESULT。对照 DSPy 源码里的 `dump_state`、`delete`、`named_parameters` 保存形态。
2. 提交 `15f1270`：schema v1、拒绝式 `version` 信封、语义校验和删除。13 个集成测试。
3. 提交 `ea58923`：DSPy JSON 存档互转。再加 4 个集成测试，合计 17。
4. `Cargo.lock` 只增加 `af-dspy-ir` 包条目。依赖是工作区已有的 serde、serde_json、thiserror 2.0.21。根 `Cargo.toml` 未改（`members = ["crates/*"]` 会收进新 crate）。

## ③ 证据

```
$ source /data/af-factory/env.sh
$ cargo clippy --all-targets -p af-dspy-ir -j 4 --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.62s
```

退出码 0，无警告输出。

```
$ cargo test -p af-dspy-ir -j 4 --offline
running 0 tests
test result: ok. 0 passed; 0 failed
running 17 tests (tests/schema.rs)
test result: ok. 17 passed; 0 failed
Doc-tests af_dspy_ir
test result: ok. 0 passed; 0 failed
```

相对空 crate 基线 0，17 个都是新测试：信封/字段清单/语义校验/删除 13 个，存档互转 4 个。

`Cargo.lock` 新增依赖只有 `serde`、`serde_json`、`thiserror 2.0.21`，没有新包。

### v1 字段清单

规范样例是一张 `BasicQA`（输入 `question`、输出 `answer`）、一条 demo、一个 predictor、一份优化状态。38 个键都有正例；必填键删掉即拒绝，`Option` 键的反例是错误 JSON 类型（缺键等于 `null`，都表示未设置）。

| 键 | 类型 | 缺键 |
|---|---|---|
| `version` | u32，只能是 1 | DI1005 |
| `program_id` | string，非空 | DI1020 |
| `signatures` | array | DI1020 |
| `signatures.name` | string，非空，程序内唯一 | DI1020 |
| `signatures.instructions` | string，可空串 | DI1020 |
| `signatures.inputs` | array，字段表 | DI1020 |
| `signatures.inputs.name` | string，非空 | DI1020；空串是 DI1001 |
| `signatures.inputs.prefix` | string，可空串 | DI1020 |
| `signatures.inputs.description` | string，可空串 | DI1020 |
| `signatures.inputs.type_name` | string，非空 | DI1020；空串是 DI1011 |
| `signatures.outputs` | array，字段表 | DI1020 |
| `signatures.outputs.name` | string，非空，且不与输入重名 | DI1020；空串是 DI1001 |
| `signatures.outputs.prefix` | string，可空串 | DI1020 |
| `signatures.outputs.description` | string，可空串 | DI1020 |
| `signatures.outputs.type_name` | string，非空 | DI1020；空串是 DI1011 |
| `demos` | array | DI1020 |
| `demos.id` | string，非空，程序内唯一 | DI1020 |
| `demos.signature` | string，必须指向已有签名 | DI1020；不存在是 DI1002 |
| `demos.values` | object，键是签名字段名，必须盖住整张字段表 | DI1020 |
| `predictors` | array | DI1020 |
| `predictors.name` | string，非空，唯一，不能是 `metadata` | DI1020 |
| `predictors.signature` | string，必须指向已有签名 | DI1020；不存在是 DI1002 |
| `predictors.demos` | array of demo id | DI1020；不存在是 DI1003 |
| `predictors.config` | object | DI1020 |
| `predictors.config.temperature_millis` | i64 或 null。`0.7` 写成 `700` | 可省略 |
| `predictors.config.rollout_id` | i64 或 null | 可省略 |
| `predictors.config.n` | u32 或 null；有值时 ≥ 1 | 可省略 |
| `predictors.lm` | object 或 null | 可省略 |
| `predictors.lm.model` | string，非空（lm 存在时） | DI1020 |
| `predictors.lm.model_type` | string，非空（lm 存在时） | DI1020 |
| `predictors.lm.cache` | bool | DI1020 |
| `predictors.lm.num_retries` | u32 | DI1020 |
| `optimization` | object | DI1020 |
| `optimization.optimizer` | string 或 null，有值时非空 | 可省略 |
| `optimization.status` | `unoptimized` / `optimized` / `failed` | DI1020 |
| `optimization.metric` | string 或 null，有值时非空 | 可省略 |
| `optimization.score_millis` | i64 或 null。`0.875` 写成 `875` | 可省略 |
| `optimization.compiled` | bool | DI1020 |

`demos.values` 里面的键不是 schema 键，是签名字段名。

未知顶层键、错误 JSON 类型、非整数字段（例如分数 `0.875`、温度 `"hot"`）是 DI1014。`version` 为 `2`、`0`、`"1"`、`1.5`、`-1`、`null` 是 DI1006。非对象文档是 DI1014，不当成缺版本。

### 错误码

| 码 | 何时 |
|---|---|
| DI1001 | 签名字段名或 demo 值的字段名为空；删除时字段名为空；存档字段自带的 name 为空 |
| DI1002 | demo、predictor 或删除指向的签名不存在 |
| DI1003 | predictor 指向的 demo 不存在 |
| DI1004 | 删除的字段不在该签名上。程序不改 |
| DI1005 | 对象里没有 `version` |
| DI1006 | `version` 不是整数 1 |
| DI1007 | program_id、签名名、demo id、predictor 名为空 |
| DI1008 | 签名名、字段名、demo id、predictor 名或 demo 引用重复 |
| DI1009 | demo 的值用了签名表以外的字段名 |
| DI1010 | 存档 signature.fields 条数与签名表不一致 |
| DI1011 | 空 type_name、`n = 0`、空 model / model_type / optimizer / metric，或 predictor 名叫 `metadata` |
| DI1012 | demo 没覆盖签名上的某个字段 |
| DI1013 | predictor 引用的 demo 属于另一张签名 |
| DI1014 | JSON 类型、未知键、存档形状不对 |
| DI1015 | 存档里的 predictor 名在 IR 里不存在 |
| DI1016 | IR 里的 predictor 在存档里没有条目 |
| DI1017 | 存档字段自带的名字与签名表该位置不一致 |
| DI1018 | 存档 demo 条数与 predictor 的引用条数不一致 |
| DI1019 | 多个 predictor 共用一张签名，但存档里的指令或字段文本不同 |
| DI1020 | v1 必填键缺失 |
| DI1021 | 多个 predictor 共用一条 demo，但存档里的值不同 |

## ④ 反馈

1. DSPy `load_state` 用 `zip(..., strict=False)` 按位置套字段，多出来的存档字段被丢掉。互转在条数不一致时返回 DI1010，避免名字错位还当成功。
2. 存档字段顺序固定为输入然后输出，与 `Signature.fields`（`{**input_fields, **output_fields}`）一致。交叉排列的输入/输出表达不了。
3. predictor 配置和优化状态不在 `Predict.dump_state` 里。它们只活在 IR。互转往返时这两段从 IR 原样保留，存档只覆盖指令、前缀、说明、demo 值和 LM 的四个稳定键（`model`、`model_type`、`cache`、`num_retries`）。
4. serde 对 `Option` 缺键直接读成 `None`，和 `null` 相同。这七个键的反例因此是错误类型，不是删键。
5. `serde_json` 没有打开 `preserve_order`，对象键按 BTree 排序。字段数组的顺序仍然是输入在前、输出在后。不要为了键序去开这个 feature，否则会作用到整个工作区。
6. 导出的 LM 不写 `_dspy_lm_class`。DSPy 对没有这个键的旧存档按 `dspy.LM` 装回。导入时这个键和其他 kwargs（例如 `temperature`）被忽略，不写进 IR。
7. 顶层 `metadata`（DSPy `save` 写的 `dependency_versions`）导入时跳过。predictor 不能叫 `metadata`，避免和这个键撞车。
8. 删除存在的字段时，同时去掉该签名各 demo 里的同名值，这样删完仍然能过校验。DSPy 只改签名类，demo 里的键会留下。

## ⑤ 遗留

- 本克隆没有 poc-dspy 的 36 条断言原文，没有在本机再跑一遍 Python。对齐依据是任务描述和 DSPy 当前源码里的 `dump_state` / `delete` / `save(save_program=False)`。
- `Predict.fields`（decision / extended signature）导入时忽略，不进 IR。
- LM 的 `api_base`、`temperature` 等 kwargs 不保存。要原样装回自定义 LM，还缺类路径和这些键。
- 导出不写 `metadata.dependency_versions`。DSPy `load` 会去读这份 metadata；直接把本 crate 的导出文件丢给 `Module.load` 会在 metadata 上失败。互转的对象是 `dump_state` 的 predictor 映射，不是 `save` 的整文件。
- demo 必须盖住签名上的每个字段。DSPy 允许半截 example，这里故意更严。
- 没有跑 `cargo deny check`。没有新包，许可证集合不变。

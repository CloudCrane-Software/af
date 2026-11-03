# RESULT-selector — P2-L09 selector 装载组件（失败分类账 #1-4 缺组件终裁补齐）

## ① 结论

**完成：4/4 草稿闭环转绿。** ADAS / AutoGen-Team-JSON / GPTSwarm / MetaGPT 四份
对齐草稿的 E2001（topology_selector 候选图未声明）经包图清单装载注入后全部
**PASS / 0 违例**；反例（不注入 = 默认空集）复现基线全拒（3/6/3/3 条，与
`~/.cache/afb-g3gate/drafts-run-fix1.txt` 一致）——无隐式放行。语法与语义
零改动：`m4.rs` / `model.rs` / 4 份草稿 / `计划/变更日志.md` 均未触碰。

- `cargo test -p af-dsl`：**全绿**（14 个测试二进制 **226 passed / 0 failed**，
  含新增 `registry_test` 20 例）。
- `cargo clippy -p af-dsl --all-targets`：**0 警告 0 错误**（零新增）。
- `cargo check --workspace`：通过（af-dsl 是库依赖，其余 crate 不受影响）。

## ② 交付面（设计五点 → 落地）

| 设计点 | 落地 |
|---|---|
| ① af/packages + manifest 规范 | `af/packages/README.md`（规范正本）+ 4 示例包。选 **JSON**：serde_json 已在 af-dsl 依赖面内，`deny_unknown_fields` + 装载器静态校验，零新依赖。组合规则 `pkg.{package}.{name}@{version}`，恰为候选图引用形 |
| ② validate 注入面 | **前任务已存在**（`validate/mod.rs:69` `ValidateOptions.declared_graphs` + `m4.rs:481` 核对，selector_test 已覆盖）；本任务确认不动、复用 |
| ③ registry 装载器 | `crates/af-dsl/src/registry.rs`（新模块，非新 crate——最小面）：`PackageManifest::parse`（解析+静态校验）/ `Registry::load`（扫 `<根>/<包>/manifest.json` → BTreeSet）/ `Registry::options()`（注入 ValidateOptions） |
| ④ 示例包 | adaskit(3) / autogen(5) / swarmkit(3) / sweng(3) = **14 条图引用**，逐字取自 4 稿 topology_selector（use+fallback 去重），`registry_test::DRAFT_REFS` 钉死闭包 |
| ⑤ 验收闭环 | `~/.cache/afb-g3gate-selector/g3gate`（同款判定路径 load_str→validate，外加注入；原 `~/.cache/afb-g3gate` 未动），结果见 ③节表 |

manifest 静态校验（装载即拒，不留到 validate 层）：`package`/`name`/`version`/
`source` 非空且前三者不含 `'@'`（组合面保留分隔符）；`graphs` 至少一项；
同清单与跨包组合引用不重复；目录即包候选——每个非隐藏目录必须有
`manifest.json`，非目录条目（README/.gitkeep）跳过；空根合法 = 空集 =
fail-closed。确定性：目录条目按路径排序处理，引用集 BTree。

## ③ 证据（4 稿重跑结果表）

命令（harness 可重建，源码在 `~/.cache/afb-g3gate-selector/g3gate/src/main.rs`）：

```
CARGO_TARGET_DIR=~/afbuild-phase2w cargo run --release --quiet -- \
  计划/研究组/对齐草稿/{ADAS,AutoGen-Team-JSON,GPTSwarm,MetaGPT}.yaml
# registry: 4 packages, 14 declared graphs
```

| 草稿 | 注入前（基线 drafts-run-fix1） | 注入后（本表） |
|---|---|---|
| ADAS.yaml | REJECT，3×E2001 | **PASS，0 违例** |
| AutoGen-Team-JSON.yaml | REJECT，6×E2001 | **PASS，0 违例** |
| GPTSwarm.yaml | REJECT，3×E2001 | **PASS，0 违例** |
| MetaGPT.yaml | REJECT，3×E2001 | **PASS，0 违例** |

反例（同 harness，`AF_PACKAGES=/tmp/af-packages-empty`）：4 稿 REJECT
3/6/3/3 条——与基线逐数一致，证明放行仅来自清单声明、非默认行为。

测试（均本机实跑，`CARGO_TARGET_DIR=~/afbuild-phase2w`）：

```
cargo test -p af-dsl                 # 226 passed / 0 failed（14 二进制全绿）
cargo test -p af-dsl --test registry_test   # 20 passed（RED 起步：E0432 unresolved import）
cargo clippy -p af-dsl --all-targets  # 0 warning / 0 error
cargo check --workspace              # 通过
```

registry_test 覆盖：manifest 解析 8 例（组合规则/未知字段/缺 source/空字段/
含 '@'/空清单/清单内重复）+ 扫描 7 例（排序集/缺根/文件当根/缺 manifest/
非法 manifest/跨包重复/空根 fail-closed/非包文件跳过）+ 闭环正反例
（同文档默认 E2001×2 → 注入后 0）+ options 纯注入 + 仓库正本 4 包 14 引用
闭包（覆盖 + 恰等于，防清单与草稿漂移）。

## ④ 偏差与裁量

1. **扫描规则一改**：首版「非目录条目即错」会拒掉规范文件
   `af/packages/README.md` 自身——改为「目录即包候选，非目录条目跳过」。
   抓笔误能力不降：包目录缺 `manifest.json` 仍报错。
2. **严格闭包测试按集合比较**：registry 引用集是 BTreeSet（字典序），与
   DRAFT_REFS（草稿声明序）排序不同——断言改为集合相等 + 基数 14。
3. **RegistryError 首错即停**（非 Report 一次收齐）：manifest 是静态资产
   非文档语义，装载期修一处即可；模块注释已登记此裁量。
4. 图本体（AFB 文件）未提供——按任务设计「首版只需声明集」，`source`
   串指向预留路径，装载期只登记不打开。

## ⑤ 遗留

- 失败分类账 #1-4 处置待编排者按本表重审销账（本任务不写 计划/变更日志.md）。
- af/packages 图本体 AFB 文件与 `source` 解析（打开/校验图体）留待后续包产物任务。
- Agensh-DeLM（#5，E2304 退回修订流）不在本任务范围，未触碰。

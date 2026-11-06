# RESULT-UPDATE — B3（波次1⑦/T9-8）：insta 快照测试接入 af-graph

只动 `crates/af-graph/`（另加根 `Cargo.lock` 以锁定新 dev-dependency，与 T8 加 `cel-interpreter` 时同样处理）。未改校验规则，未推翻 RESULT.md / RESULT-UPDATE-T8.md 已验证结论。

## ① 结论

**通过**。`insta` **1.49.0**（crates.io 非 yanked 最新稳定）作为 af-graph 的 dev-dependency 落地，显式打开 `yaml` feature。`afg validate` 的两路输出都有入库快照（`.snap`，不走 `cargo insta review`）：

1. **违规清单**：CLI 标准输出，`insta::assert_snapshot!`。文案仍是原先的 `PASS`/`FAIL` 逐行打印，抽到 `format_validate_stdout`，二进制与快照共用这一份。
2. **决策工件**：`ValidateDecision`（签发方固定 `afg-validate`，裁决 `accept`/`reject`，违例带 E200x），`insta::assert_yaml_snapshot!`。

覆盖：

| 场景 | 违规清单 | 决策工件 |
|---|---|---|
| 干净产线 `legal-1-pipeline-human` | PASS，零条，无 E200x | `verdict: accept`，`violation_count: 0` |
| E2001 引用断裂 `illegal-3-undeclared-ref` | 1 条 `[E2001 DANGLING_REFERENCE]`，点名 `notify_slack` | `verdict: reject`，`code: E2001` |
| 未知类型，开放扩展 `unknown-kind-accepted`（kind=`rerank`） | PASS，零条 | `unknown_kinds: [rerank]`，`verdict: accept` |
| 未知类型撞上既有类型约束 `unknown-kind-type-mismatch` | 1 条 `[E2002 VALUE_OUT_OF_RANGE]`，报文含 `kind=\`rerank\`` | `code: E2002`，`unknown_kinds: [rerank]` |

未知 kind **仍然不是违例**（RESULT.md：kind 开放、不设白名单；测试 `unknown_node_kind_is_open_extension` 仍在）。没有新增"未知类型"错误码。未知 kind 只有在撞上既有安全点类型约束时才拒绝，码是已有的 **E2002**，不是 E2001。

质量门：`cargo fmt --check` 干净；`cargo clippy --all-targets -p af-graph -- -D warnings` 零警告；`cargo test -p af-graph` **33 passed / 0 failed**（predicate 8 + validate 21 + 新快照 4；基线 29，+4）。同一命令再跑一遍，无 `updated snapshot`、无 `.snap.new`。

## ② 步骤

1. 读 `crates/af-graph/RESULT.md` 与 `RESULT-UPDATE-T8.md`。确认：五规则校验器、E2001–E2007 已赋码、未知 kind 可通过、CLI 违例打到 stdout。
2. 查 crates.io 稀疏索引：`insta` 非 yanked 最新 = **1.49.0**。`Cargo.toml` 加 dev-dependency `insta = { version = "1.49.0", default-features = false, features = ["yaml"] }`。关掉 default features，不拉只服务终端上色的 `console`。
3. `src/validate.rs` 增加 `format_validate_stdout` 与 `ValidateDecision` / `ValidateDecisionViolation`。`unknown_kinds` 只观察不在首发四型里的 kind，不产生违例。`src/bin/afg.rs` 改为打印这份清单，退出码逻辑不变。
4. 新 fixture：`unknown-kind-accepted.json`（合法产线，`qc.kind = rerank`）、`unknown-kind-type-mismatch.json`（受控循环安全点要求 `check`，体中节点 kind 为 `rerank`）。
5. `tests/validate_snapshots.rs`：每个场景跑真实 `afg validate` 二进制，断言退出码与错误码，并核对 stdout 等于 `format_validate_stdout`，再对清单和决策工件做快照。
6. `INSTA_UPDATE=always` 写入 8 个 `.snap` 后直接入库。显式快照名按场景分开（同名会写进同一个文件）。
7. `cargo fmt`、clippy、全量测试，再无 `INSTA_UPDATE` 复跑一遍。

## ③ 证据

**依赖解析**

```
$ curl -s https://index.crates.io/in/st/insta | （非 yanked 版本）
latest: 1.49.0
has 1.49.0: True
```

`Cargo.lock` 新增：`insta` 1.49.0（依赖 once_cell / serde / similar / tempfile，后三者里 once_cell、serde、tempfile 原本已在锁里）、`similar` 2.7.0。无其他包版本变动。

**三门 + 第二次测试（af-graph 作用域）**

本机 `rust-toolchain.toml` 指向的独立工具链目录 `1.99.1-x86_64-unknown-linux-gnu` 缺 `rustc`（组件安装冲突，`bin/cargo` 已存在导致 rustup 回滚）。`stable` 工具链的 rustc 就是 **1.99.1 (58cf8f9c3 2026-10-13)**，与钉住的版本一致，故下列命令在 `source /data/af-factory/env.sh` 之后使用 `cargo +stable`。

```
$ cargo +stable fmt -p af-graph -- --check
FMT-CLEAN

$ cargo +stable clippy --all-targets -p af-graph -- -D warnings
    Checking af-graph v0.1.0 (.../crates/af-graph)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.18s   # 零警告

$ cargo +stable test -p af-graph -j 4
     Running tests/predicate.rs          ->  8 passed
     Running tests/validate.rs           -> 21 passed
     Running tests/validate_snapshots.rs ->  4 passed
   （lib / bin / doc-tests 各 0 tests, ok）
# 合计 33 passed / 0 failed

$ cargo +stable test -p af-graph -j 4    # 第二次，未设 INSTA_UPDATE
# 同样 33 passed / 0 failed；输出里没有 "updated snapshot"
# find crates/af-graph -name '*.snap.new' -o -name '*.pending-snap'  → 空
```

**快照内容摘要（`.snap` 已入库）**

- 干净产线清单：`PASS fixtures/legal-1-pipeline-human.json  graph_version=7 name=\`article-pipeline\` nodes=6 edges=5 subgraphs=0`
- 决策工件：`verdict: accept` / `violation_count: 0` / `violations: []`
- E2001 清单：`FAIL fixtures/illegal-3-undeclared-ref.json  (1 条违例)` 下接 `[E2001 DANGLING_REFERENCE] graph: 边 publish.done -> notify_slack.event 引用了未声明节点: notify_slack`
- 未知 kind 接受：`unknown_kinds: [rerank]`，`verdict: accept`，`violation_count: 0`
- 未知 kind 类型不符：`[E2002 VALUE_OUT_OF_RANGE] ... kind=\`rerank\` ...`，决策工件 `code: E2002`

## ④ 反馈

1. **两路输出分开钉是值得的**：文本清单是人读的 CLI 契约（含路径与 `PASS`/`FAIL`），YAML 决策工件是下游可解析的裁决（错误码、类名、规则、作用域、报文）。文本一行改了，YAML 快照不必跟着碎；码或字段改了，文本快照也不掩盖。
2. **未知 kind 不能为了"有一条 E200x"去拒绝**。RESULT.md 已验证开放扩展。本次把"未知类型"拆成两条快照：单独出现仍接受；撞上安全点类型约束时走既有 E2002。这样快照既覆盖任务卡的 E200x，也不改语义。
3. **决策工件不是工单域的 `DecisionArtifact`**。没有角色、权限、签名。签发方固定为编译器。不要把它接进 af-workorder 的四层校验。
4. **insta 的显式快照名是全局的**（文件名是 `模块__名字.snap`，不含测试函数名）。四个测试如果都叫 `violation_list`，会互相覆盖同一个文件。名字必须带场景。
5. **`yaml` feature 在 1.49.0 只打开 insta 自带的 serde YAML 序列化**，不再额外依赖 `serde_yaml`。`default-features = false` 避开了 `console`。

## ⑤ 遗留

- `edge.output/input` 的工件存在性/类型表仍未建（T8 ⑤ 已记录）。决策工件只转写现有 `Report`，没有新的检查。
- 没有"未知类型"专属错误码。若最终方案要把未知 kind 从开放扩展改成拒绝，需要新码，并改 `unknown_node_kind_is_open_extension` 与本次 `unknown_kind_open_extension_zero_violations` 快照。本次按已验证结论保持接受。
- `cargo deny` 未对 insta 的依赖树再跑一遍（`similar` / `tempfile` / `once_cell` 的许可证相对 `deny.toml` allow 列）。insta 自身是 Apache-2.0。
- 环境：命名工具链目录 `1.99.1-x86_64-unknown-linux-gnu` 缺 `rustc`，rustup 修复时因 `bin/cargo` 冲突回滚。本次用同版本的 `stable` 工具链完成全部门禁。修复那个共享工具链目录不在本任务改动范围内。

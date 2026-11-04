# G6 现状核清 · compiler_version 落点与逐消费点清单

基线：`b8fc5ac`（= 开工时 origin/product/main，style(order) 提交，af-compiler 版本 0.1.0）。
核清方法：全仓 `grep -rn "compiler_version" --include="*.rs" crates/` + 消费函数
（`verify_package_manifest` / `decode_plan` / `planbind_bind`）调用方全列举 + 改前探针
（probe-before-output.txt）行为实证。

## 一、产出面（compiler_version 写进哪些产物，文件:行）

| # | 产物面 | 字段 | 写入点 | 值来源 |
|---|--------|------|--------|--------|
| 1 | 任务包 `package_manifest.json` | `TaskPackageManifest.compiler_version` | `crates/af-compiler/src/package.rs:48`（字段）、`:127`（build_package_manifest 赋值；write/build 两入口同值） | `env!("CARGO_PKG_VERSION")`（af-compiler 自身版本） |
| 2 | 计划 `plan.json` | `CompiledFrom.compiler_version` | `crates/af-compiler/src/plan.rs:77`（字段）、`:459`（generate_plan 赋值） | 同上 |
| 3 | （产物落盘，非代码）golden 计划 `ci/out/g4/*.plan.json` 等携带该字段 | — | 由上述入口产出 | — |

组包链：`assemble_construction_package`（construction.rs:224）→ `write_package_manifest`
（construction.rs:244）——同盖 af-compiler 版本戳。

## 二、消费点清单（谁消费这些产物；改前是否有任何版本核对）

| # | 消费点 | 消费的产物面 | 改前已有的核对 | 改前核对 compiler_version？ |
|---|--------|--------------|----------------|------------------------------|
| 1 | `af_compiler::verify_package_manifest`（package.rs:156）——由 af-order 提交门 `preflight_package_manifest`（crates/af-order/src/cli.rs:1051-1079，:1063 调用；正式 submit 与 --dry-run 两路同点，cli.rs:336） | package_manifest.json | format_version=1 拒未知、task.json/plan.json 内容 sha256 重算比对（ContentTampered）、缺席=Absent 由调用方 fail-closed 拒（G3） | **否**（package.rs:47 注释自认「溯源面，不参与核对」） |
| 2 | `af_execline::planbind::bind`（crates/af-execline/src/planbind.rs:47，执行面消费唯一正本·裁决9）——调用方三处：af-order 干跑装订预检（cli.rs:583 `planbind_bind`）、图执行器 setup（graph_unit.rs:926）、报告单元 setup（suite/report_unit.rs:1631） | plan.json（decode_plan → CompiledFrom） | 信封 v=1 + deny_unknown_fields、E9004 registry_digest 重算比对、graph_version 三面一致、line 绑定、node_exec/graph 对齐 + 重影拒、边级重影拒、id/端口路径安全、模板装载 MANIFEST.sha256 | **否**（核对清单四项无版本面） |
| 3 | `af_order::cli::read_plan_graph_version`（cli.rs:1114-1124） | plan.json（原始 serde_json 指针读） | 只读 graph_version 单键（WorkOrder 绑定值域），非校验门 | 不适用（不读版本字段） |
| 4 | 测试面 `plan_test.rs:87` | generate_plan 产物 | 断言产出值 == `env!("CARGO_PKG_VERSION")`——产出侧等值断言 | 非消费门（不测消费侧行为） |

**「零核对」结论**：生产代码全仓无一处将产物携带的 compiler_version 与任何判据比较；
两道真门（manifest preflight / planbind）的核对清单均不含版本面。行为实证见
probe-before-output.txt：场景 A（manifest rogue 值）与场景 B（计划 rogue 值 + 摘要
自洽）真门**照过**，A 还把 rogue 值原样回显进干跑留证面。

## 三、改前探针读数（真门 = af_order::cli::run(submit --dry-run true)，与生产 main 同一入口）

| 场景 | 构造 | 改前结果 |
|------|------|----------|
| A | 正常组包后仅改 manifest.compiler_version="0.0.1-rogue"（摘要不动） | **照过**（dry_run=true，checks 全绿，留证面回显 rogue 值）|
| B | 改 plan.json compiled_from.compiler_version="0.0.1-rogue" 后重组 manifest（摘要自洽） | **照过**（planbind 四核对全绿）|
| C | manifest 删 compiler_version 键 | 拒：OR1001 坏形 `missing field compiler_version`（schema 必填——改前即 fail-closed）|
| D | plan 的 compiled_from 删 compiler_version 键（摘要自洽） | 拒：装订预检 decode 拒 `missing field compiler_version`（改前即 fail-closed）|

A/B 即破窗实证（拿「旧编译器/别的编译器」产的包照过消费门）；C/D 确立缺字段口径
（坏形拒，非「显式未核对」）在改前已成立、本批不放松不改变。

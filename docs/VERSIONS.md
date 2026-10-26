# af 版本表（docs/VERSIONS.md）

> 2026-10-25 · G5 文档版本化批。**当前形态的版本面快照**——每行注明版本值、
> 载体与核定方式。不写工期、不写未来承诺；演进项一律标注「v0.2 议程」在档。
>
> 2026-10-26 · 红队修复批（W1）追加注记：本表口径不变；修复批改动
> （AFB 语义收紧五项 + golden 7 份随批再生 + 执行面/CI 收口）见
> `docs/adr/011-redteam-w1-fixes.md`，crate 版本仍 0.1.0（阶段内修复，
> 不进位）。

## 一、crate 版本（22 crate，全部 0.1.0）

第二阶段主线五件（ADR-002；`af-plan` 折入 af-compiler，独立建位列开放问题
——软件工程包设计 §7-2）：

| crate | version | 内容锚 |
|---|---|---|
| af-dsl | 0.1.0 | AFB v0.1+fix1/fix2 前端（定版说明 `crates/af-dsl/SYNTAX.md`） |
| af-compiler | 0.1.0 | 四阶段管线；golden 21；`PLAN_VERSION = 1`（`src/plan.rs`） |
| af-execline | 0.1.0 | swev/graph/report 三单元 + planbind + af-trace-tree-verify（swev 扁平与 report 环双布局） |
| af-order | 0.1.0 | 下单门面（`submit --unit swev\|graph\|report`） |
| af-engine | 0.1.0 | 订单持久化（ledger 9-op + MES 四类跃迁 + voucher） |

底座件 17 个（af-bench/af-calendar/af-credential/af-dspy-ir/af-effect-gateway/
af-eval/af-graph/af-harness/af-mes/af-meter/af-release-gate/af-sampling/
af-sandbox/af-station/af-stop/af-trace/af-workorder）均为 0.1.0，语义结论
在各 crate `RESULT.md`。

## 二、语法与语料

| 项 | 版本 | 载体 |
|---|---|---|
| AFB 语法 | v0.1 + fix1 + fix2（+ 契约族钉死） | 正本《DSL-语法提案-B》+ 择优裁决 2026-10-24；定版说明 `crates/af-dsl/SYNTAX.md` |
| golden 语料 | 21 份 IR 快照（逐字节+篡改检测回归） | `crates/af-compiler/testdata/golden/` |
| 对齐语料 | 15/15 走通 L1-L4 全链（G3 矩阵 v2，2026-10-24） | `计划/研究组/G3-全链闸门-L1L4-v2-20261024.md`；快照正本 `计划/研究组/对齐计划快照/` |
| 逃逸舱记账 | 97 条 / 15 稿（稿均 6.47，七类 kind） | golden `body.escapes` 与 plan 快照 `escapes` 交叉同值 |
| 对抗农田钉死 | 39 份样本 13 测试（39/39） | `crates/af-dsl/testdata/farm-anomalies/` + `tests/farm_anomalies_test.rs` |

## 三、包与组件（af/packages/）

| 项 | 版本 | 说明 |
|---|---|---|
| sweng 图 | `sop_full@1.1`（token_cap 1M 校准）/ `sop_no_qa@1.0` / `sop_minimal@1.0` / `fix_swe_task@1.0` | F4D 真单 Closed（sop_full@1.1 四节点全链） |
| reportkit 图 | `paper_report@1.0` | F5c 真单 Closed（cite_check+rubric 双 passed） |
| 组件声明 | 63 条（根级 44 + 包级 19），`registry_digest` 入计划对象 | 根级 `components.json` + 各包 `components.json`；装载器 `af-compiler::components` |
| 判分链 | reportkit citation/rubric 双验证器（sha256 冻结锚）+ swebench 冻结链 | `af/packages/reportkit/units/`；F4/F5 线订单实证 |

## 四、CI 与验证

| 项 | 版本/值 | 载体 |
|---|---|---|
| CI 管道 | 九门 + step9 FV advisory | `ci/run.sh`；每跑次 SUMMARY 落 `ci/out/` 随分支入档 |
| 中立性门红名单 | 22 项登记 | `ci/neutrality-expected-fail.txt`（19→21 执行器合流批 381945c9；+1 预算校准批 81ac6e6 `sop_full_v1_1_token_cap_calibrated_to_f4c_measurement`——本表初版误记 21，复核指出后按文件亲数改 22） |
| 形式验证 | FV-1 第一波：Kani 32+17 / Verus 29 / Rocq 陈述库 10/10 | `proofs/` + `proofs/README-形式验证.md` |

## 五、开放议程（在档，非承诺）

- AFB 规范 v0.2 议程：提案-B §1.2 `budgets.scope` 记法显式化、E1008/E1054
  码表正本回填、theorems v0.2（A-03/A-07 陈述修订）。
- `af-plan` 独立建位（类型搬迁去重）——软件工程包设计 §7-2。
- trace 工具对 graph 单元 suite-summary 形 records（无 evidence.run_dir）的
  父子边支持——G5 正式批登记。

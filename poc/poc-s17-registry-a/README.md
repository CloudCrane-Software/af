# POC · S17 供给注册批 A（strategies17 ①类 7 条真注册）

- 工件：卡 `s17-registry-batch-a`（债号 D-58，父债 P3-B1；设计稿
  `30-工件/strategies17-供给面设计-20261106.md` §五.1–§五.7）。
- 分支：`feature/s17-registry-batch-a`（从 `origin/product/main`
  = `04b5a5ea` 显式 refspec 取回后切出）。
- 门跑点：anolis 工作树 `/data/bench/s17-registry-batch-a-wt`
  （HEAD `ee270886`，与 origin 分支同点）；`CARGO_TARGET_DIR=/data/bench/targets/s17-registry-batch-a`。
- 工件：`README.md`（本件）/ `registry-diff.md` / `raw/`（原始读数）。

## 注册面 diff（119 → 127）

| 面 | 前 | 后 | 增量 |
|---|---|---|---|
| `af/packages/components.json`（根级） | 96 | 104 | **+8**（unit 7 + tool 1） |
| adaskit / autogen / reportkit / swarmkit / sweng（包级） | 23 | 23 | 0 |
| **合计** | **119** | **127** | **+8** |
| `sweng/prompts/` 模板文件 | 6 | 20 | +14（+ MANIFEST 14 行） |

8 条声明（C126 一条出两件：M 记录型包装器 + S 服从度单元）：

| 组件 | 类 | impl_ref | 策略 |
|---|---|---|---|
| `pkg.iface.mcp_client@1` | tool | `executor:builtin/pkg.iface.mcp_client@af-execline`（新 builtin） | st-b-C126（M） |
| `pkg.retrieval.ndcg@1` | unit | `executor:builtin/pkg.sweng.test_unit@af-execline` | st-b-C058 |
| `pkg.skill.pair_delta@1` | unit | 同上 | st-b-C121 |
| `pkg.review.bug_capture@1` | unit | 同上 | st-b-C098 |
| `pkg.cost.efficiency@1` | unit | 同上 | st-b-C118 |
| `pkg.ab.placebo@1` | unit | 同上 | st-b-C087 |
| `pkg.iface.adherence@1` | unit | 同上 | st-b-C126（S） |
| `pkg.ablate.matrix@1` | unit | 同上 | st-b-C105 |

registry_digest：`f5a6de7c9faa7859ab2b8c206a7517b996e770a78099baea9ac7cd1e565f7103`（119）
→ `4a49856fb7d650258a9f5fbb5c122dcac7031c8f959b98d61edde6c3c04f0d44`（127）
（后值由 `pkg_reportkit_package.rs` 字面量钉实测反填，非手写）。

## 四段链（seam `compile-all`，对生产注册表 af/packages）

| | compilable | pass_four_stage（load+validate+lower+bind+plan） |
|---|---|---|
| E9001 前（63 条注册表，历史读数） | 44 | 10 |
| E9001 后（119 条，历史读数） | 44 | 44 |
| **本批后（127 条）** | **51** | **51** |

本批 7 条新 compilable **全部过四段全链**（plan_id 见
`raw/compile-all.s17.json`）：st-b-C058/C087/C098/C105/C118/C121/C126。

## 三门（anolis 实跑，exit 全 0）

| 门 | 命令 | 读数 |
|---|---|---|
| strategies_check | `python3 -B ci/strategies_check.py` | exit 0；61 条目 compilable=**51** not_applicable=**10**；R5 跨树集合一致（65 件，基线 HEAD ee270886） |
| compiler+dsl | `cargo test -p af-compiler -p af-dsl` | exit 0；38 suites / **403 passed / 0 failed** |
| workspace | `cargo test -j4 --workspace` | exit 0；162 suites / **1634 passed / 0 failed / 9 ignored** |
| 装载逐条 | `cargo test -p af-dsl --test strategy_library_test -- --nocapture` | 51 COMPILE-OK / 0 NEEDS-SYNTAX / 10 NOT-APPLICABLE，零失败 |

## 判据面零弱化

- `ci/strategies_check.py` / `ci/criteria.yaml` / `ci/baselines/gate-inputs.sha256`
  钉面（含 `ci/debt-register.md`）**零改动**（`git diff` 无这些路径）。
- 计数钉同步与包落地同 commit：`registry_decl_test.rs`（总量 119→127 +
  `DECLS_S17_A` 8 名逐名/类/impl_ref 断言）、`bind_test.rs`（119→127）、
  `pkg_reportkit_package.rs`（119→127 + digest 字面量钉）。
- 债务登记册未动：D-58 行登记属运营者/主代理面（设计稿 §七.5 前置），
  本批不写仓内镜像 `ci/debt-register.md`（改它须同批
  `bash ci/gate_inputs_check.sh update` 重钉，超出本卡 allowed_paths）。

## 如实登记的边界

1. **声明数 8 而非 7**：设计稿 §五 preamble 写「计数断言 119→126」，但 §五.6
   自身规格列出**两件**（M `pkg.iface.mcp_client` + S `pkg.iface.adherence`）。
   本件按规格执行 8 条 → 119→**127**；卡面硬约束写「119+N」与此一致（N=8）。
   7 是策略条数（①类 7 条），不是声明条数。
2. **执行面（环③）仍未闭合**：四段链全过 = 编译到 plan；`pkg.iface.adherence@1`
   等 unit 的运行期语义 = 容器内跑判据命令（`test_unit` builtin，E9001 同型），
   本批不接「点名级调用序列 → 容器命令」的输入桥（属 p3-a1 接缝断点 (b)/(c) 面）。
   `IfaceLog` 的进程内记录在模型环结束后落节点证据目录
   `nodes/<id>/iface-calls.json`（`graph_unit.rs` 收尾段）。
3. **模板存量缺口未清偿**：既有 78 个 `template_ref` 中 76 个无盘文件（44 条
   compilable 全体）不在本批（设计稿 §八.4）；本批新增 14 个引用全部落盘登记。

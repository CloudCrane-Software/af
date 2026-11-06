# 训练轨演化产物的注册回注接口（af 侧切片 · D-56）

> 工件身份：wave-02 卡 `s17-training-artifact-registry`（债号 D-56，父债 P3-B1；
> 通道 anolis-build；分支 `feature/s17-training-artifact-registry`；合线归 runner
> merge-check 范式，落账 `计划/阶段欠账台账.md:119`）。
> 判定依据：设计稿 `30-工件/strategies17-供给面设计-20261106.md` §七.3
> （st-b-C075 ③ 跨面，>L——训练面本体不做，af 仓只承接「演化产物注册回注」切片）。

## 一、跨面判据与切片边界

st-b-C075 的 na_reason（`strategies/st-b-C075.yaml:13`）：训练流程的被演进对象是
**训练过程中的工具/hook 参数**（RL 搜索本体在训练轨，不在本仓）；AFB 是运行时蓝图，
`uses`/组件装订**只能引用演化产物**，不表达演化搜索本身。

| 面 | 归属 | 说明 |
|---|---|---|
| RL 搜索本体 / 训练调度 / 工具-hook 参数演化 / 评测执行 | 训练轨（外派，不在本仓） | depends_on「训练轨主体派单（主代理/外派，本卡不阻塞）」——本卡交付时该前置未满足 |
| 演化产物 → 组件声明面的**回注接口** | **本仓（本切片）** | 登记册 + 映射规约（本文）+ 机器校验三件 |
| 提示词/hook 形态产物（模板资产） | 提示词资产面（本仓既有机制） | `<包>/prompts/<名>.md` + `MANIFEST.sha256` 逐件 sha256 核对（`af/packages/README.md` §包内提示词资产；闭集测试 `pkg_sweng_package.rs::sweng_prompt_manifest_is_closed_over_files_on_disk`）——**不进本登记册** |

## 二、登记册数据规约（`af/packages/training-artifacts.json`）

非目录条目：`af-dsl::registry` 的目录扫描与 `af-compiler::components` 的清单装载均天然
跳过（装载器只读根级/包级 `components.json`——`crates/af-compiler/src/components.rs:213-248`），
零装载面影响、零 `registry_digest` 影响。

| 字段 | 层 | 约束 |
|---|---|---|
| `schema` | 顶层 | 钉 `af.training-artifact-registry/1`（升版须同步测试常量 `REGISTER_SCHEMA` 与本文） |
| `note` | 顶层 | 必填非空自述注记 |
| `artifacts` | 顶层 | 映射条目数组；**空集合法** = fail-closed（无产物即无回注） |
| `artifacts[].artifact` | 条目 | 产物 id（训练轨全限定名 `<轨>/<run>/<形态>/<名>` 形），非空、不含 `@`、册内唯一 |
| `artifacts[].kind` | 条目 | 闭集：`tool_wrapper`（工具面演化产物）/ `unit_judge`（判定单元演化产物） |
| `artifacts[].declaration` | 条目 | 回注目标声明名（`uses.*` 引用串去 `@版本` 段），非空、不含 `@` |
| `artifacts[].version` | 条目 | 回注目标声明版本（须为该声明已声明的版本键） |
| `artifacts[].impl_ref` | 条目 | 实现定位串，与目标声明的 `impl_ref` 逐字相等 |
| `artifacts[].evidence` | 条目 | 读数定位串（poc 证据目录 / 训练轨读数坐标），必填非空 |

条目与文件两层均 `deny_unknown_fields`（未知字段即拒）。

## 三、映射规则（产物名 → 声明名 / impl_ref）

| 规则 | 内容 | 违反后果 |
|---|---|---|
| R1 | 产物 id 非空、不含 `@`（`@` 为 `name@version` 组合引用保留段）、册内唯一 | 测试红，回注不成立 |
| R2 | `kind` 闭集两形态，且与目标声明类一致：`tool_wrapper`→tool、`unit_judge`→unit | 同上 |
| R3 | 回注目标声明必须**已在** `af/packages` 组件清单注册（先落声明、后登记映射——禁「有映射无声明」） | 同上 |
| R4 | `version` 必须是该声明已声明的版本键 | 同上 |
| R5 | 册内 `impl_ref` 与声明 `impl_ref` 逐字相等；声明侧必填；且为执行面 builtin 形态 `executor:builtin/<组件名>@<crate>`，其 `<组件名>` 自身须已注册（锚已接线实现——禁 P2-L12 式预留包路径 `packages/<包>@<ver>/...` 作训练产物锚） | 同上 |
| R6 | `evidence` 必填非空：每次回注须同批登记读数定位串 | 同上 |
| R7 | 凡新增/改删 `components.json` 声明，必须**同 commit** 同步计数钉（`registry_decl_test.rs:144-146`、`bind_test.rs:44-46`、`pkg_reportkit_package.rs:37-38` 及 digest 字面量钉 `:100`——设计稿 §二 硬约束）；本接口不豁免 | 既有字面量钉测试红 |

## 四、回注流程（训练轨 → af 仓）

1. 训练轨产出产物与读数证据（训练轨侧，不在本仓）；
2. af 侧在 `components.json`（根级或包级）落声明：`class`/`name`/`version`/`impl_ref`，
   tool 类带 `effect_class`（`af/packages/README.md` §components.json 字段约束）；
3. 同 commit：同步计数钉（R7）+ 在 `training-artifacts.json` 追加映射条目（R1–R6）；
4. 跑门（anolis）：`python3 -B ci/strategies_check.py` 全绿 +
   `cargo test -p af-compiler`（含本接口测试与字面量钉）通过；
5. 合线归 runner（merge-check 范式：fmt+strategies_check+全量 test+clippy）。

## 五、机器校验（可执行规约）

`crates/af-compiler/tests/training_artifact_registry_test.rs`：

| 测试 | 覆盖 |
|---|---|
| `repo_register_conforms_to_mapping_rules` | 仓库正本登记册盘面对话（schema+note+逐条 R1–R6） |
| `repo_register_schema_and_note_are_pinned` | schema/note 钉（静默升版或丢失自述即红） |
| `repo_registry_loader_count_matches_manifest_files` | 装载计数 vs 盘面 `components.json` 声明数之和的独立对账（计数钉读数面） |
| `mapping_rules_accept_conforming_entries` | 仓库正本上的 unit/tool 两形态正例 |
| `mapping_rules_reject_nonconforming_entries_on_repo_registry` | R1（空/含@）、R2、R3（空/含@/未注册）、R4、R5（分歧）、R6 负例 |
| `fixture_register_conforms_on_nonempty_register` | 合成注册表 + 非空合成册的盘面对话（册空时不 vacuous 的保证之一） |
| `fixture_registry_rejects_unwired_reserved_and_malformed_impl_refs` | R5 全部分支（builtin 目标未注册 / 预留包路径 / 声明无 impl_ref / 缺 `@crate` 段） |
| `fixture_register_rejects_duplicate_artifact_ids` / `fixture_register_rejects_schema_drift` | R1 册内重复、schema 漂移 |

夹具数据：`crates/af-compiler/tests/fixtures/training-artifact-registry/`（合成
`components.json` 5 条 + 合成登记册 1 条；非仓库正本）。

## 六、读数登记（D-56 交付时点）

| 读数 | 值 | 来源 |
|---|---|---|
| 分支 base | `origin/product/main` `04b5a5ea` | `git log --oneline origin/product/main -1` |
| 声明总量 | 119（根级 96 + 包级 23：adaskit 2 / autogen 5 / reportkit 6 / swarmkit 1 / sweng 9） | 逐文件 dump；装载对账测试独立复算 |
| 计数钉 | 119，本卡零改动、与声明同步 | `registry_decl_test.rs:144-146`、`bind_test.rs:44-46`、`pkg_reportkit_package.rs:37-38`+`:100` |
| 本卡新增声明 | **0 条**（训练轨未派单，depends_on 未满足——禁造空转件） | `git show --stat`（分支 diff） |
| 登记册条目 | 0 条（空集 fail-closed，如实登记） | `af/packages/training-artifacts.json` |
| strategies_check | 44 compilable / 17 not_applicable **不劣化**（本卡零 `strategies/` 改动） | anolis 实跑 exit 0：`dsl_states={'compilable': 44, 'not_applicable': 17}`，R5 跨树一致（基线 HEAD `b437b263`，only-in-worktree=0 / content-diff=0） |
| anolis 门（worktree detached `b437b263`；`CARGO_TARGET_DIR=/data/bench/targets/s17-training-artifact-registry`） | **五门全绿**：`cargo fmt --check` exit 0（零 diff）；`cargo test -p af-compiler -p af-dsl` exit 0（本接口测试 `training_artifact_registry_test` 9 passed / 0 failed）；`cargo test -j4 --workspace` exit 0（162 suites / 1632 passed / 0 failed / 9 ignored）；`cargo clippy --workspace --all-targets -j4` exit 0（本卡文件零警告；4 条存量基线警告非本卡所生） | anolis 实录：脚本 `/data/bench/s17-training-full-gates.sh`，证据落 `/data/bench/s17-training-evidence/`（首轮红读数存档 `-round1/`） |

## 七、边界与未决

1. **册空集是时点状态而非不变式**：训练轨派单后首批回注按 §四 流程落地，届时
   `artifacts` 非空、计数钉随声明增量同步（R7）——本测试随条目增加继续逐条校验，
   无需改测试。
2. **首批回注声明落位**（根级暂驻 vs 建包）按 P2-L12/E9001 纪律：无图可声明不建空包
   目录（`af/packages/README.md` §P2-L12 扩容）。
3. **hook/提示词形态产物不进本册**（§一边界）：走提示词资产面 MANIFEST 闭集，
   既有测试覆盖，本接口不重复承载。
4. **不新建 README 节**：本规约以独立文档为正本（避免与 `s17-registry-batch-a`
   的 README 尾节在合流面上冲突）；发现路径由本文件与登记册 `note` 自述。
5. **产物 id 命名约定**（`<轨>/<run>/<形态>/<名>`）为本接口工作定义；训练轨侧若有
   既有命名，以训练轨派单时对齐为准（未决，不预设）。

## 指纹

- 判据命令（anolis）：`python3 -B ci/strategies_check.py`（cwd=仓根）；
  `cargo test -p af-compiler` / `cargo test -j4 --workspace`；
  `cargo clippy --workspace --all-targets -j4`；`cargo fmt --check`。
- 分支：`feature/s17-training-artifact-registry`（base `origin/product/main`
  `04b5a5ea`）；本卡零 `crates/` 源码改动（仅新增测试文件与夹具数据）、零
  `strategies/` 改动、零判据件（`ci/strategies_check.py`、`ci/criteria.yaml`、
  `ci/baselines/gate-inputs.sha256`）改动。

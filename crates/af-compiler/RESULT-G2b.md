# af-compiler G2b 施工结果（P2-L11 · 编译器收口：stage 3 组件绑定 + stage 4 计划生成）

- 分支：`phase2-g2b`（基 `phase2-g2b@824c4b2`——含 af-dsl 全量 + registry + golden21 + af-compiler G2a）。
- 范围：①`components`——组件注册表装载器（`af/packages` 根级/包级 `components.json`，确定性 `registry_digest`）；②`bind`——stage 3 绑定：IR `uses.*` 引用 × 注册表对照，E9001-E9003 一次收齐 fail-loud，effect_class/envelope 溯源回填，AgentSpec 四元组可装载面（endpoint），逃逸注入点透传；③`plan`——stage 4 计划生成：落地契约 §7.1 草案顶层结构 + 自含增补，确定性派生 `plan_id`/`dsl_digest`，拒绝式信封 decode；④CLI `af-compiler-plan`（stage 1-4 全链，`--registry` 一根两用）。
- 权威输入：提案 B §3.3/§3.5/§码表 1572 行（E900x 段）、落地契约 §2.2（组件注册 + domain pack digest）/§7.1（计划对象草案）/§7.2-1（计划自含）、ADR-010 §5（四一等字段）、af-harness `spec.rs`（AgentSpec 四元组 + EndpointSpec + validate_endpoint）、af/packages 四示例包现实形态。
- 未做（禁止项遵守）：未实现真实执行（计划是编译产物，执行归 af-execline）；af-dsl 源码与 golden 零改动（`git diff 824c4b2..HEAD --stat -- crates/af-dsl crates/af-compiler/testdata` 为空）；未写 计划/变更日志.md。

## 验收命令与结果（实跑）

| 检查 | 命令 | 结果 |
|---|---|---|
| 全量测试 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-compiler` | **53 通过 / 0 失败**（golden 11 + inline_body 4 + ir_schema 8 + lower_error 2 + route_guard 4 = G2a/G3 既有 29 零回归；新增 bind 17 + plan 7 = 24） |
| 静态检查 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-compiler --all-targets` | **零新警告**（基线 824c4b2 实跑对比：前后均 1 条 G2a 既有 doc 缩进警告，位于未改动的 golden_test.rs:256） |
| 红绿序列 | `git log --oneline 824c4b2..HEAD` | RED `9e2eb46`（23 例：22 红/1 绿对照，红均 panic 于 `todo!` 桩，实跑确认）→ GREEN `4de114d`（53/53）。严格先红后绿 |
| CLI 全链 ×3 | `af-compiler-plan --registry af/packages <yaml>`（sample2/sample3/GPTSwarm） | **三份计划 JSON 产出**（13221/13595/33499 字节，rc=0）；每样例**双跑 cmp 逐位一致**（SAME） |
| 逃逸注入 | 同上（GPTSwarm）+ `jq .body.escapes` | **6 条 EscapeSpec 进计划**（`search.dispatch`/`search.execute_map.py_node`/`search.graph_edit`/`search.graph_value`/`search.optimize`/`search.reward`），`impl_ref` 逐字保留（如 `packages/swarmkit@1.0.0/hatches/graph_edit.py`）、kind/owner/share_budget_pct 透传 |
| fail-closed 负例 | `af-compiler-plan --registry af/packages Voyager.yaml`；`af-compiler-plan GPTSwarm.yaml`（无注入） | 前者 rc=1 `E9001 节点 "explore.observe" 引用 "pkg.openworld.env@1" 未注册`、stdout 0 字节（无残缺计划）；后者 rc=1 E2001（selector 候选图拒绝）——注入链与组件链都是承重墙 |
| G2a 零回归 | `af-compiler-snapshot sample2.yaml \| cmp - golden/sample2.ir.json` | 逐字节相等（快照 CLI 行为不变） |
| af-dsl 不受影响 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl` | **244 通过 / 0 失败**（components.json 三文件对图清单装载面零影响——manifest.json 未动，`package_count()==4` 既有断言原样过） |
| 磁盘纪律 | 开工 `df -h /` 244G 可用；收工 243G 可用 | 全程远超门槛；构建仅 `~/afbuild-phase2d`；未触其他工作树 |

门样例（G2 门自证第一件）：golden 源 `sample2.yaml`（metagpt_sop，裸校验链）+ `sample3.yaml`（debate_kv，裸校验链）+ `GPTSwarm.yaml`（对齐稿，`--registry` 注入链——topology_selector 三候选图 `pkg.swarmkit.graph.*` 经 af-dsl 图清单核对）。双层确定性（第三件）库内测试 `gate_samples_ir_and_plan_deterministic_bitwise`：三样例 ×（IR 层两次独立编译逐位一致 + 计划层两条独立全链逐位一致）。

新增测试覆盖明细（24 例）：

- **bind_test（17）**：仓库正本注册表装载（7 条声明/双跑 digest 一致/未注册名不可达）；图清单-组件清单解耦对照（af-dsl `package_count()==4` 复核）；sample2 全字段绑定（三工具 effect_class 回填 persist/pure/pure、未版本化引用升格 `@1`、route.main/route.cheap 分流、AgentSpec endpoint 实数据、QA unit 绑定、pm `envelope<prd>`→provenance.envelope_of 回填、transform 节点组件面全空）；sample3 嵌套体内节点（`rounds.debate_map.debater`）共享路由绑定；E9001（未注册/`bad@@ref` 不可解析）；E9002（`@2` 未声明/未版本化命中双版本）；E9003（unit 当 tool 用）；逃逸注入点（kind/impl_ref/reason/owner/share_budget_pct 五字段透传断言）；空注册表 × 无引用文档通过；装载纪律五反例（tool 缺 effect_class、model_route 缺 endpoint、端点三违例 ftp-url/userinfo/sk-key、跨文件重复引用、name 内嵌 '@'）；违例一次收齐（两条未注册同时点名）。
- **plan_test（7）**：sample2 计划形状（信封 v=1/plan_id 派生形+12 位 hex/compiled_from 三摘要+registry_digest/line+order 骨架/graph 原形/自含四块/node_exec 路径字典序+体内节点+工具 effect_class+unit 绑定+终态工件在 build.report+pm envelope 溯源/预算三账/安全点收集）；GPTSwarm 六逃逸入计划+impl_ref 保留+工具节点 external_once；三样例 IR+计划双层确定性；decode 往返；信封四反例（v=2 版本先行/未知顶层键/缺 v/body 未知字段）；未绑定拒绝生成（Unbound 一次点名≥4 条）；`plan_json_pretty` 单入口=分步产物逐位一致。

## 结构体清单（新增 29 型 + 1 常量 + 1 CLI）

| 模块 | 型 | 说明 |
|---|---|---|
| `components` | `ComponentClass`（6 值） | model_route/loop_policy/memory/tool/context_compaction/unit |
| | `ComponentDecl` / `ComponentsFile` | 单条声明 / 清单文件正本（`deny_unknown_fields`） |
| | `EndpointSpec` + `Protocol`/`ReasoningTier`/`AuthStyle` | af-harness `spec.rs:16-95` 字段级镜像（自持有，不引依赖） |
| | `ComponentRegistryError`（5 值） | 装载 fail-loud 首错即停（承 af-dsl `RegistryError`） |
| | `ComponentRegistry` | `name→version→声明` BTree 双层 + digest |
| `bind` | `BindCode`（3 值）/`BindViolation` | E9001/E9002/E9003 稳定码 + 节点/引用/信息三元组 |
| | `ResolvedComponent` | 解析后版本化引用（name/reference/class/version/effect_class/impl_ref/endpoint） |
| | `BoundComponents` / `BoundAgent` / `BoundNodeExec` | §7.1 components 面 + 四元组 endpoint 面 + 节点投影 |
| | `BindReport` | bindings + escapes 透传 + registry_digest + violations（一次收齐） |
| `plan` | `PLAN_VERSION=1` / `ExecutablePlan` / `PlanBody` | 拒绝式信封（键 `v`）；body 十四块（七正位+自含增补） |
| | `CompiledFrom` / `LineSlot` / `OrderSlot` / `OrderLine` / `SlaSlot` / `PlanPriority` | 溯源三摘要+registry_digest / 执行面占位 / 工单骨架 / 词表=Normal |
| | `PlanNodeExec` / `UnitBinding` / `PlanComponents` / `PlanToolRef` / `NodeBudget` / `PlanArtifact` / `PlanRetry` | §7.1 node_exec 面（点号路径键） |
| | `PlanBudget` / `ResourcesSlot` / `ObservationSlot`（+Otel/Events/Mes 三槽） / `RecoverySlot` / `PlanSafetyPoint` | 三账原样 / 空占位 / 草案默认骨架 / 失败账+安全点+三账本 |
| | `PlanError`（3 值）/ `PlanParseError`（5 值） | Unbound/Lower/Serialize；信封四情形+形状 |
| bin | `af-compiler-plan` | stage 1-4 全链 CLI（`--registry` 一根两用：图清单注入 + 组件清单装载） |

## 新错误码（E900x 段分配——提案 §码表 1572 行「部署期 4 条」）

| 码 | 语义 | G2b 生产者 | 触发（实跑验证） |
|---|---|---|---|
| E9001 | 组件引用缺失 | `bind::resolve` | 引用名未注册（Voyager `pkg.openworld.env@1` 实跑拒绝）；引用串不可解析（空串/内嵌 `'@'`） |
| E9002 | 版本不符 | `bind::resolve` | 名已注册但请求版本未声明（`pkg.kit.hammer@2` vs 已声明 `@1`）；未版本化引用命中多版本声明（解析歧义按版本不符拒） |
| E9003 | 组件类别不符 | `bind::resolve` | 名+版本已注册但类别与 uses 面不符（unit 组件被 `uses.tools` 引用） |
| E9004 | registry digest 不匹配 | **编译期无生产者**（码位保留） | digest 由 bind 产出、随计划 `compiled_from.registry_digest` 携带；执行面装载时对本地注册表重算核对，不符即拒启动（§2.2） |

绑定违例**一次收齐**（逐条点名节点点号路径+引用串，承 af-dsl Report 范式）；组件清单**装载**错误（JSON 非法/静态校验/重复引用）不带 E 码——静态资产首错即停（承 af-dsl `RegistryError` 纪律，两类错误范式有意不同）。

## 设计裁决（本模块内的取舍与依据）

1. **组件清单格式 = `components.json` 新文件面，不动 manifest.json**。落地契约 §2.2 的 `component.json` 单文件信封与 `c-<类>.<名>` id 形是执行面侧草案；仓库现实（af/packages 四包 manifest 闭包、af-dsl registry 测试锁 `package_count()==4`）不允许新增包目录或改 manifest schema。故：根级（跨包共享——模型路由）+ 包级（自有 tool/unit）两位置；根级文件是**非目录条目**，af-dsl 目录扫描天然跳过——两套清单零耦合（bind_test 有解耦对照测试）。引用串保持 golden 源现实形态（`route.main@1`/`pkg.sweng.locate`），未强改 `c-*` 形。
2. **引用切分 = 最后 `'@'`**。`name@version` 切分后 name 内嵌 `'@'` 即不可解析 → E9001（不猜语义）。未版本化引用绑定唯一声明版本并在产物中升格为 `name@version`（计划面全版本化）；多版本歧义 → E9002。
3. **ADR-010 四一等字段的 G2b 兑现度**：`effect_class`（tool 类注册表回填，计划面可见）与 `provenance.envelope_of`（`envelope<T>` 声明机械解析——非第二份类型推导器）**真实回填**；`capability_refs`/`trust` v0.1 无数据源（凭证/信任注记的注册面未开），保持空占位——诚实不伪造。
4. **AgentSpec 四元组只交付可装载面**。`endpoint` 实数据（model_route 声明原样直通）；`context_template`（需模板组件注册面——DSL 的 `template_ref` 是包资产非 `uses` 组件）与 `policy`（需 loop_policy 组件）v0.1 无声明面，不伪造。任务书「task 节点的绑定产物之一」的完整兑现转后续（见遗留 3）。
5. **`plan_id` 确定性派生**：`plan.<图名>@<图版本>+<dsl_digest 前 12 位>`。§7.1 草案只说「本计划标识」未定派生式；编译期无时钟无随机（§3.5-4），取内容寻址——同一 DSL 同一编译器版本恒同 id，改一个字即变。
6. **dsl_digest = SHA-256(IR compact 规范序列化)**，与 `CelRef.digest`/`registry_digest` 同一口径（`lower::digest_of` 单点实现）；`registry_digest` = 全量声明按 `(name,version)` 字典序规范序列化取 SHA-256，双跑逐位一致。
7. **bind 输入是 IR 不是 Document**（任务书「stage3 从 IR 出发」）——`plan_json*` 便捷入口内部 lower 一次后分步；逃逸注入点 = `BindReport.escapes`（IR 记账视图原样透传），计划 `escapes` 块是其直接落点——「escape 构造 → EscapeSpec 进计划」的注入链在绑定层真实存在（GPTSwarm 六逃逸实跑为证）。
8. **EndpointSpec 镜像 + 端点校验同源复制**（`validate_endpoint`：http(s) 前缀/无 userinfo/`key_ref` 引用名非 `sk-`）。不引 af-harness 依赖（拖运行面），双实现不可避免——字段级注记 + 此处登记（与 G2a「IR 镜像 af-graph」同一纪律）。

## 偏差表（与落地契约 §7.1 草案的出入）

| # | 偏差 | 理由 |
|---|---|---|
| 1 | `node_exec.components.tools` 为对象数组 `{reference, effect_class}`（草案=字符串数组） | ADR-010 effect_class 回填必须在计划面可见（执行面副作用路由的声明依据） |
| 2 | 增补 `stores`/`types`/`interface`/`escapes` 四块（草案无） | §7.2-1「计划对象必须自含：单文件即编排真相」——草案缺状态槽/类型词典/IO 契约/逃逸记账，执行面无从装载；增补块全部 IR 原样，零新语义 |
| 3 | `budget` 块 = IR 三账（budgets/stop/failures）原样，非 §5.2 voucher/dimensions 形 | voucher 是执行面签发物（§5.3 开放项 1），编译期无来源；三账是 DSL 声明面的全部预算事实 |
| 4 | `resources` 恒空 `{gates:[],limits:[]}` | DSL v0.1 无资源声明面（并行度在 graph 的 `max_fanout`/`max_iterations`）；槽位占住待 §3.2 面开 |
| 5 | `observation` = 草案默认值骨架（otel disabled/512、`AF_OBS_ROOT`、`env:MES_CH_URL`/`af_mes`） | 观测配置数据源是执行面；以草案示例值为编译期占位，line_id/station_id 空串（bind_line 落位） |
| 6 | `recovery` = 失败账 + 安全点收集 + 三账本恢复输入，`node_retry` 恒 null | 失败分类账是类级非节点级；§6.2 的幂等键命名式/dedupe 是运行期构造；安全点是 DSL 声明面里唯一可静态收集的恢复材料 |
| 7 | `node_exec[].retry`/`artifact_contract[].qualified_by` 恒 null | 节点级重试无声明面；`qualified_by`（close 门材料）需数据流推导，v0.1 无静态推导源 |
| 8 | `order` 块仅 `graph_version`/`state:"Created"` 为实数据，id/stable_id/run_id/sla/supersedes 等空值占位；`line` 三槽 null | 工单签发与产线绑定是执行面事项（§5 顺序、§2.2）；草案注释本身即「release 前材料由执行面补齐」 |
| 9 | `sla.priority` 词表闭于 `Normal` 单值（deny 未知） | 草案仅示例 Normal；扩展词表待执行面对接 |
| 10 | 组件引用串 = DSL 现实形（`pkg.sweng.locate@1`），非草案 `c-tool.<名>@<版本>` | af/packages 四包 + golden 21 源的现实形态是正本；`c-*` 形是 §2.2 草案 id 约定，未落地 |
| 11 | `plan_id` 派生式自定（草案未定） | 裁决 5——内容寻址确定性 |

## 遗留（转 G2c/执行面对接）

1. **E9004 执行面核对器**：计划已携带 `registry_digest`，装载侧重算-比对-拒绝的实现在 af-execline/执行面装载路径。
2. **四一等字段后两位**：`capability_refs`（af-credential Cedar/biscuit 衰减凭证登记）与 `trust` 注记的注册面/声明面未开——字段位已在序列化面总在场。
3. **AgentSpec 四元组补全**：`context_template`（模板组件注册——`template_ref` 目前不绑定）与 `policy`（loop_policy 组件、stop 条件编译）开声明面后回填；`BoundAgent.endpoint` 已就位。
4. **`qualified_by` 静态推导**（close 门材料：工件 → 下游 check 节点）需数据流分析，v0.1 未做。
5. **组件清单的 `unit_bindings`/`domain pack` 聚合面**（§2.2 的 line 级快照）——`line` 槽位已占住，待 af-workorder `bind_line` 对接。
6. **G3 golden 计划快照入库**：`af-compiler-plan` 三样例输出可作第二批 golden（本包已验证 CLI 可用与逐位确定性；入库动作归 G3 任务包，避免本包动 golden 禁区）。
7. af/packages 首批组件声明 = 三门样例引用闭包（7 条）；其余 golden 源引用（crewkit/reactkit/reflexkit 等）未注册——按需扩清单即放行，E9001 fail-closed 语义已验证（Voyager 实跑拒绝）。

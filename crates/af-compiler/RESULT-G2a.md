# af-compiler G2a 施工结果（P2-L06 · 首模块：IR schema v1 + 确定性 lowering）

- 分支：`phase2-compiler`（基 `phase2-dsl@73ccfb2`——含 af-dsl 全量 M1-M4 + fix1）。模块边界即审计边界（ADR-007）。
- 范围：①crate `af-compiler` 落成（IR schema v1：拒绝式信封 `IR_VERSION=1`、节点/边/类型表/槽/node_exec/策略全 BTreeMap 确定序、内联 loop/map 体降维为命名子图、四一等字段占位）；②lowering 纯函数（已过全量校验的 `Document` → `IrProgram`）+ 确定性测试（六 golden 逐位一致）；③两份文档交付（`docs/保证边界矩阵.md` + `docs/执行语义规范-v0.md`，ADR-010 三义务之前二）；④快照入口（库函数 `snapshot_json`/`snapshot_json_pretty` + CLI `af-compiler-snapshot`）。
- 权威输入：ADR-010 §2/§5、提案 B §1.2/§1.5/§3.2/§3.4/§3.5、落地契约 §5/§6/§7、af-dsl `src/model.rs`（AST 正本）+ `testdata/sample1-6.yaml`（golden 输入）。
- 未做（禁止项遵守）：stage3 绑定 / stage4 计划生成（G2b）；af-dsl 源码零改动（`git diff 73ccfb2..HEAD -- crates/af-dsl` 为空）；未写 计划/变更日志.md。

## 验收命令与结果（实跑）

| 检查 | 命令 | 结果 |
|---|---|---|
| 全量测试 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-compiler` | **23 通过 / 0 失败**（golden 5 + inline_body 4 + ir_schema 8 + lower_error 2 + route_guard 4），0 警告 |
| 静态检查 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-compiler --all-targets` | **0 警告**（`grep -cE "^warning|^error"` = 0） |
| 红绿序列 | `git log --oneline 73ccfb2..HEAD` | RED `d4b978f`（21 例：20 红/1 绿，红均 panic 于 `lower.rs` 桩）→ RED 补 `87cab12`（错误路径 2 例，均红）→ GREEN `5a90891`（23/23 全绿）。严格先红后绿 |
| CLI 实跑 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo run -q -p af-compiler --bin af-compiler-snapshot -- crates/af-dsl/testdata/sample1.yaml` | **stdout 输出 IR JSON**（`ir_version: 1`；四一等占位在序列化面：`capability_refs: []` / `effect_class: null` / `provenance: null` / `trust: {}`；budgets 按 id 排序；CelRef 带 roots/digest） |
| 磁盘纪律 | 开工 `df -h /` 20G 可用；收工 18G 可用 | 全程 ≥15G 门槛，未触其他工作树与主仓 target |

正例门槛：六 golden 样例（`af-dsl/testdata/sample1-6.yaml`，单一事实源不复制）全量校验零违例 + lowering 确定性**逐位一致**（`serde_json::to_string` 比较三种口径：两条独立编译管线 ×2、同一 Document 两次 lowering ×2、管线产物与文档直降相等）× 六样例。

测试覆盖明细（23 例）：

- **确定性/往返**（golden 5）：六样例逐位一致 ×3 口径；序列化→`decode_ir`→与 lowering 产物相等（往返无损）；样例 1 全字段投影抽查（interface/stores/policies 三账/node_exec：components/artifacts/context/writes）；样例 4/5 抽查（eval_input 槽、check 节点 unit 投影、非 LLM 直读、checkpoint 边、human 构造、from_body 保真、体内 LLM 节点 context 文档序）。
- **内联体降维等价**（inline_body 4）：六样例节点/边总量守恒（DSL 递归计数 == IR 根图+全子图计数）+ 全子图版本 stamp 父图版本；样例 3 命名子图（`debate_kv/rounds`、`debate_kv/rounds/debate_map`）+ loop/map_spec 反指 + entry=体首元素 + 扩展四字段（args/exit_when/on_exhausted/carried 排序）；样例 6 双层嵌套 loop + 跨层安全点 `acquire.admit` + L4 逃逸拓扑位与记账视图同数据；样例 2/4 子图形状。
- **route 归一化**（route_guard 4）：guard 公式逐字断言（§1.5 确定性重构形：`!((format_check.passed == false))` 等 ×5 位点）+ goto:exit 不产边 + default:exit 的 exit 守卫缺席（结构性收束）+ CelRef（roots 集合/digest 64 位小写十六进制）跨编译稳定 + gate assert/loop exit_when 投影。
- **信封与一等字段**（ir_schema 8）：版本非 1（含 body 篡改不掩盖版本拒绝——版本先行）/缺 version（含根非对象与无效 JSON 退化）/非整数/未知顶层键/未知 body 字段与嵌套字段（deny_unknown_fields 全层）共 5 反例；`ir_version==1`；四一等字段六样例全在场（键存在性逐节点/逐工具/逐工件断言）+ 占位值空默认（`[]`/`null`/`{}`）。
- **错误路径**（lower_error 2）：未过校验输入返回显式 `LowerError` 不 panic——route 悬空（E2102 面，前提核对：stage 1 确实拒绝该文档）与空构造体（E1007 面，af-dsl M1 不查、lowering 兜底）。

## 设计裁决（本模块内的取舍与依据）

1. **IR schema 自持有，不依赖 af-graph**。提案 §3.2 草图写 `graph: GraphTemplate`（af-graph 原样），但 §3.4 预算表需要给 af-graph 加 8 可选字段 + ADR-010 四一等字段还要再加——G2a 不改 af-graph 源码（避免跨 crate 波动），IR 类型在 `af-compiler/src/ir.rs` 自定义、**字段级对应** af-graph 形状（node 四字段/edge 四字段/subgraph/human/loop/map 逐结构注记）。§3.4 预算表因此转为「IR schema 字段清单」的对照账（`ir.rs` 模块文档有逐条对应）。
2. **`agents` 表推迟到 G2b**。§3.2 草图的 `agents: BTreeMap<String, AgentSpec>`（af-harness 四元组）是绑定期投影；G2a 的 `node_exec`（components/context/artifacts/ops/assert）已无损携带全部声明面（LLM/工具/评估器分野在 uses 不在 kind，提案 §1.2 判别表），四元组在 stage 3 由组件注册表解析生成。IR 无信息丢失。
3. **`InterfaceSpec` 增 `intent` 字段**（预算表为 3 字段）。`meta.intent` 的 IR 落点在提案 §1.2 写「计划对象元数据」——stage 4 消费 IR，若 lowering 不携带则丢失；故加为第 4 可选字段。
4. **`SubGraph.graph_version` 用 `String` 非 `Option<String>`**（af-graph 原形是 Option）。DSL 面一律 stamp 父图版本（§3.2），Option 的「未审定草稿」态在 DSL 面不可表达——E2007「map 体未版本化」保留给 IR 直写路径。
5. **`Document.version` 与 `meta.topology_selector` 不入 IR**（有意非丢失）：前者是 DSL 侧信封（IR 自带 `ir_version`）；后者编译期解析为单一确定图后不进运行时（裁决 §二-1），v0.1 单文档无候选图声明面。
6. **route 归一化复用 `af_dsl::route::normalize`**——同一算法同一重构形，无第二实现（消除双实现漂移风险）。
7. **from_body 不复制类型推导器**。`ArtifactSpec` 携带声明面（`from_body` 路径串原样），map 自动包 `list<>` 的推导规则与 af-dsl validate 同源、由执行面/绑定期求值——G2a 复制推导器会引入双实现。
8. **`is_final` 判定规则**：`节点路径 == meta.output.from && (类型串相等 || from_body 在场)`。多端口同型匹配时全部标 final（六样例无此情形，规则文档化待 G3 golden 固化）。

## 偏差表（与提案/任务书文面的出入）

| # | 偏差 | 理由 |
|---|---|---|
| 1 | `agents` 表未建（草图有） | 裁决 2——绑定期投影，G2b 生成 |
| 2 | `InterfaceSpec` 4 字段（预算表 3） | 裁决 3——intent 必须过 lowering 才能达 stage 4 |
| 3 | IR 自定义类型而非 af-graph 复用（草图原样复用） | 裁决 1——预算表改动不落 af-graph 源码 |
| 4 | 无效 JSON 归并为 `MissingVersion`（af-dsl 把 YAML 语法错误同样归并 E0001 的先例） | 码表无独立码位；RESULT 登记纪律 |
| 5 | route 守卫/exit 守卫在测试中为逐字断言（重构形含双括号 `!((…))`） | af-dsl M4 既定重构形，IR 原样落位非重新设计 |
| 6 | 测试侧三处断言笔误在 GREEN 期修正（BTreeMap 字典序工具名/样例 1 预算 scope/嵌套路径 `explore.acquire.code_exec`） | 均系测试期望错误（对照样例原文核实），非实现让步；修正在 GREEN 提交内逐条注明 |

## 遗留（转 G2b/后续）

1. `keep_best` 选择函数、map 未声明 `max_fanout` 的默认并发——绑定期组件契约/对接会（执行语义规范 §8）。
2. `checkpoint` 边的快照存储语义（当前仅声明位；落地契约 §6.3 开放项）。
3. 一等占位四字段的回填机制（capability 衰减凭证 / effect_class 注册表直通 / envelope 血缘+eval_sha256 / trust 注记）——stage 3 绑定期落地，字段位已占住（ADR-010 §5「现在最便宜」的兑现）。
4. G3 golden 入库：以 `af-compiler-snapshot` 对六样例的输出为 golden 文件（本模块已验证 CLI 可用与逐位确定性；入库动作归 G3 任务包）。
5. 间接自验证不查（INV-3 已知缺口）与 eval_input 沙箱层——见《保证边界矩阵》§4 诚实边界。

# RESULT — poc-graph：图模板 schema 与编译期校验器原型（crates/af-graph）

## ① 结论

**通过**。schema（serde）+ 五条规则的校验器全部实现并真实执行：`cargo test -j 4 -p af-graph` **16 通过 / 0 失败**；`afg validate` CLI 跑 8 个 fixtures，3 个合法 PASS、5 个非法各报恰好 1 条违例且各踩各的规则。ask 要求的 3 合法 + 3 非法之外，补了 2 个非法 fixture 使规则④⑤也有 fixture 级覆盖（其余 6 个边界用例在测试内用代码构造）。

## ② 做了什么

**schema（`src/model.rs`，serde 定义）**
- `GraphTemplate`：`graph_version`（一等键，修正案 7）、`name`、`entry`、`nodes`、`edges`、`subgraphs: BTreeMap<String, SubGraph>`。
- `Node`：`kind` 开放为 `String`（首发四型 task/gate/check/transform 仅作常量，不设白名单，未知 kind 可通过校验——测试 `unknown_node_kind_is_open_extension` 断言）；`human: Option<HumanSpec>`（一等人工节点：station/role/quorum）；`loop_spec: Option<ControlledLoop>`；`map_spec: Option<MapSpec>`。三个构造与 kind 正交。
- `ControlledLoop`：`body`（独立子图名）+ `max_iterations`（>0）+ `carried_state`（max_items/max_bytes 至少一个，即"携带状态有界"）+ `safety_points`（必须非空，且每个安全点要落在循环体内并类型匹配：check 节点/gate 节点/带 human 标记的节点）。
- `MapSpec`：`body` + `over`（工件集合引用 "节点.工件名"）+ `max_fanout`；子图必须已版本化（对标 Airflow Dynamic Task Mapping，"组件已审定则实例化不算改拓扑"）。
- `Edge`：`from.output → to.input` 四元组（边=工件引用）；`SubGraph`：自带 `graph_version`/`entry`/`nodes`/`edges` 的独立命名空间。

**校验器（`src/validate.rs`）**：`validate(&GraphTemplate) -> Report { violations: Vec<Violation> }`，一次收集全部违例（编译期检查器应一次报完，不首错即停）。规则：① `Cycle` 静态图必须 DAG（三色 DFS，报完整环路径）；② `Reachability` 从入口 BFS，孤立节点点名报错；③ `UndeclaredReference` 边端点/入口/子图名/安全点指向/节点 id 重复；④ `LoopNotControlled` 缺安全点、max_iterations=0、携带状态无界、安全点类型不符；⑤ `MapSubgraphUnversioned` map 体缺 `graph_version`。
**关键设计裁定**：DAG 检查对循环体子图豁免（否则合法循环不存在），但 map 体不豁免（每个元素只跑一遍，体内环=无界循环）——测试 `loop_body_cycle_exempt_but_map_body_cycle_rejected` 双向断言。

**CLI（`src/bin/afg.rs`）**：`afg validate <json...>`，违例逐条打印、任一失败退出码 1。

**fixtures（8 个 JSON）**：`legal-1-pipeline-human.json`（人工 gate+transform 节点的 DAG）、`legal-2-loop.json`（人工安全点的受控 revise 循环）、`legal-3-full.json`（人工+循环+map 同图）；`illegal-1-cycle`（回边→①）、`illegal-2-orphan`（孤立节点→②）、`illegal-3-undeclared-ref`（幽灵节点→③）、`illegal-4-loop-no-safety`（缺安全点→④）、`illegal-5-map-unversioned`（map 体未版本化→⑤）。

**测试（`tests/validate.rs`）**：16 个用例——3 合法通过；5 非法 `only_rule()` 强断言（只踩目标规则、不踩别的）；规则④⑤的其余侧面（无界状态/零迭代/安全点在体外/类型不符）；serde round-trip 后仍合法；未知 kind 开放性；缺必填字段在 serde 层被拒（结构归 serde、语义归校验器的两层分工）。

## ③ 证据

```
$ cargo build -j 4 -p af-graph
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.22s   # 0 warning

$ cargo test -j 4 -p af-graph
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ ./target/debug/afg validate crates/af-graph/fixtures/legal-*.json illegal-*.json; echo exit=$?
PASS legal-1-pipeline-human.json  graph_version=7 nodes=6 edges=5 subgraphs=0
PASS legal-2-loop.json  graph_version=3 nodes=3 edges=2 subgraphs=1
PASS legal-3-full.json  graph_version=12 nodes=5 edges=4 subgraphs=2
FAIL illegal-1-cycle.json  (1 条违例)
  - [cycle(环检测)] graph: 静态图存在环: draft -> review_gate -> revise -> qc -> draft
FAIL illegal-2-orphan.json  (1 条违例)
  - [reachability(入口可达性)] graph: 孤立节点（从入口不可达）: legacy_export
FAIL illegal-3-undeclared-ref.json  (1 条违例)
  - [undeclared-reference] graph: 边 publish.done -> notify_slack.event 引用了未声明节点: notify_slack
FAIL illegal-4-loop-no-safety.json  (1 条违例)
  - [loop-not-controlled] graph: 受控循环未声明安全点（safety_points 为空），拒绝编译
FAIL illegal-5-map-unversioned.json  (1 条违例)
  - [map-subgraph-unversioned] graph: map 体子图 `map_body` 未版本化（graph_version 缺失）：…必须先审定
exit=1        # 仅跑 legal-*.json 时 exit=0
```

环境备注：clippy 未装（toolchain=minimal profile），只验证了 rustc 零警告；crates.io 稀疏索引/静态下载可达（首次拉取 serde 1.0.229 正常），未离线。

## ④ 对最终方案的反馈

1. **kind 与构造正交被验证顺手**：人工/循环/map 做成节点上的 Option 字段而非新 kind，扩展类型（注册新 kind 字符串）零 schema 改动。建议最终方案固化这条：四型是"执行语义"，三构造是"控制流语义"，别让扩展类型和它们抢命名空间。
2. **DAG 豁免的判定依据是个坑**：豁免按"该子图被某个 `loop_spec.body` 引用"判定——循环体被改指不存在的子图时（测试 `loop_body_must_be_declared_subgraph`），原循环体失去豁免，其内部环同时报③+①。两个错误都真实，但说明**最终方案应先做引用完整性、再判 DAG**（或豁免按声明意图），否则错误报告的条数会随报错顺序抖动。
3. **map 体必须 DAG、循环体可有环的不对称是有原则的**，建议写进设计纪律：map 体每元素只执行一遍，环=无界循环；循环体的环=受控迭代，其"无界风险"由④（安全点+有界）接管。这正好承接设计文档 §2.1.1"循环触发事件历史分片写成编译期静态检查"的占位。
4. **安全点必须验"落在体内 + 类型匹配"**，只验非空不够——否则声明一个不存在的安全点就能绕过④（对应测试覆盖了两条绕路）。
5. **两层校验分工**：serde（含必填字段）管结构，校验器管语义，PoC 里工作良好；`Report` 收集全部违例（非 fail-fast）对工作台编辑体验明显更好，建议保留。

## ⑤ 遗留问题

- **工件名级引用完整性未做**：`edge.output/input` 与节点实际产出、`map.over` 的 "节点.工件" 前缀都未校验（ask 规则③只要求节点引用）。最终方案需要一张"节点产出工件 → 消费工件"的类型/存在性表。
- `kind`↔`human` 搭配未约束：设计文档说人工修订是 Gate+Transform 双语义，PoC 未强制 human 只挂这两型（保留开放性，待定夺）。
- 子图"未被任何循环/map 引用"未检测（死子图）；未声明 entry 时②跳过（避免噪声），是否要降级为警告未定。
- 事件数估算器（估算循环事件总量超限即编译失败）未做，④里的 `max_items/max_bytes` 只是承载字段。
- `Violation` 无源位置（JSON 无行号），最终方案要配 source map 才能做好编辑器内报错。
- schema 演进策略（只加不改、N-1 兼容）与 `deny_unknown_fields` 的取舍未定；clippy 待安装后补跑。

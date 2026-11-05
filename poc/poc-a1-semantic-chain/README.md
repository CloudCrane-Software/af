# poc-a1-semantic-chain — 语义层名册全链正本 + sidecar 覆盖面改造（A1 波次）

日期：2026-11-05　施工树：`~/af-dev/af/wt/a1-semantic-chain`（anolis）　分支：
`feature/a1-semantic-chain`　工厂工作区：`~/workspace/agent黑灯工厂`（本机）

## ① 这笔做什么

任务书 `30-工件/外派任务书/A1-语义层全链施工.md`：把语义层做成「全链」——

1. **名册正本**：`otel/semantic-roster.json`（机读，schema `af.otel.semantic-roster.v1`）
   + `otel/semantic-roster.md`（人读节）。管三件事：哪些 SpanName 算语义层、
   每个的订单级键契约（order_domain_keys / resource_order_keys /
   span_order_keys）、归属腿。生产者（af-execline drive，include_str）、
   投影器（运行时读+roster_findings 对账）、断言器（一致性对账）三面消费。
2. **sidecar 覆盖面改造**：emit-sidecar 新增源⑤语义层腿——对最新可投影订单
   跑投影器，四层树缓存 `状态观测/otel-sidecar-semantic.json`；
   `--sidecar` 回放（断言器以四实参复演 emit_round）**恒**复原语义树
   （_post 本地截获零网络）⇒ 语义层名册 0/4 → 4/4，VACUOUS=10 → 0。
   真库中继按 45min 节流（树变或到期），时间戳改写为发射时刻（RT-E138
   同款）落入 --production 近窗；emit.semantic_spans 记当轮实发。
3. **生产者语义化**：投影器 L1 补 §2.2 L1 载体键 `af.unit`（records.unit_id
   归一 swev/report/graph）+ `af.order.stage`（ledger final_state→词表）；
   不可归一/在途 ⇒ 省略+finding（不猜）。crate drive 的层→名改由正本供给
   （正本缺席 ⇒ 零行 fail-closed，不 panic 进 drive）；af-trace 在线 lane
   未铺腿，评估登记于正本 `unpaved_legs` + 人读节 §5。

## ② 三面消费落点

| 面 | 位置 | 机制 |
|---|---|---|
| 生产者 | `crates/af-execline/src/span_produce.rs` | `include_str!("../../../otel/semantic-roster.json")`；`roster()`；`materialize` 只从正本取层→名 |
| 投影器 | 工厂 `tools/otel/af-otel-projector.py` | `load_roster`（AF_SEMANTIC_ROSTER → `<WS>/af/otel/`）；`roster_findings`；selftest D 绿/红腿（红=篡改正本必炸）；project 命令遇 ROSTER-VIOLATION exit 1 |
| 断言器 | 工厂 `tools/otel/otlp-assert.py` | `main()` 载约定块后 `roster_consistency_findings`（名册键集/kind/父链/order_domain_keys/resource_order_keys/span_order_keys 全等）；漂移 FATAL exit 1；正本缺席 ⇒ stderr 注明、行为等价（不红） |

## ③ 夹具与读数

- `raw/three-side-fixture.sh`：绿路三面全绿 + 红路（篡改正本：L1 改名 +
  order_domain_keys 缩编）三面全红。运行位=工厂机（crate 面 ssh anolis）。
- 2026-11-05 实跑读数（波次 A1 施工会话）：
  - crate-green rc=0 / crate-red rc=101（契约测试红）
  - projector-green rc=0（8/8 符合预期）/ projector-red rc=1（D 红腿检出）
  - assertor-green rc=0（SELFTEST PASS 全探针）/ assertor-red rc=1（FATAL 两行漂移）
- C1 转绿：`otlp-assert.py --sidecar` 施工前 rc=2（PASS=16 VACUOUS=10，语义层
  名册 0/4）→ 施工后 rc=0（PASS=26 FAIL=0 VACUOUS=0，语义层名册 4/4）。
- C2 保持绿：`--production` rc=0（P1–P5 全 PASS，真库语义层行 96）→
  `--production-check` rc=0（status=green）。

## ④ 边界

- 名册正本合流前，工厂侧 af/ 检出位无此件：投影器/断言器对账 SKIP（不猜绿），
  断言器行为与旧版等价；合流 pull 后自动生效。
- 中继只读订单产物、只写 状态观测/ 缓存与快照；STOP/AF_OTEL_DISABLE 语义不变。
- 快照对外形状：metrics 十三项与顶层键集零变化（看板/ recon B1 不扰）；
  新增 sources.semantic_leg 与 emit.semantic_spans 两个子键。
- af-trace 在线 lane 语义化未铺（D-1~D-9，另行立项）；crate 状态面 schema
  （af.otel.production.v1）不变，unit/stage 不进状态面行（无消费面）。
- img-gc HALT 哨兵致 obs-runtime-guard-gate 红 = 在役人工停手态，与本批无关。

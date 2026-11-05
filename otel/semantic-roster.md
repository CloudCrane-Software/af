# semantic-roster.md — OTel 语义层名册全链正本（人读节）

> 机读正本：同目录 `semantic-roster.json`（schema `af.otel.semantic-roster.v1`）。
> 入库：af 仓（本文件与 JSON 同批提交）。三面消费：**生产者**（af-execline drive /
> af-trace 在线 lane）、**投影器**（工厂 `tools/otel/af-otel-projector.py`）、
> **断言器**（工厂 `tools/otel/otlp-assert.py`）——名册/订单级键契约/归属腿以
> 本正本为唯一源，任何一面不得另抄一份。

## 1. 正本管什么（与判据面的边界）

| 内容 | 权威位置 |
|---|---|
| 哪些 SpanName 算语义层（名册） | **本正本** `semantic_layers` 键集 |
| 每个 span 的订单级键契约 | **本正本** `order_domain_keys` / `resource_order_keys` / `span_order_keys` |
| 归属腿（哪个生产腿产此 span） | **本正本** `legs` |
| 层/kind/父链（结构契约） | 本正本（与 span-conventions.md §6 一致，断言器对账） |
| 可执行键集全表 / allowed_keys / 树形判据 / production 下界 | span-conventions.md §6（断言器判据面，不动） |

判据语义（P1~P5、A/S/R/I/K/N 断言族）不在本正本；本正本动，
判据面行为不变（断言器只做「正本 vs 约定块」一致性对账，一致即行为等价）。

## 2. 名册（语义层四名，L1→L4）

| 层 | span 名 | kind | 父 | 订单级键契约 | 归属腿 |
|---|---|---|---|---|---|
| L1 | `invoke_workflow` | INTERNAL | 根 | span  attrs：`af.order_id`+`af.work_order_id`+`af.line`；Resource 挂订单三键 | af-execline-drive / af-otel-projector |
| L2 | `invoke_agent` | INTERNAL | L1 | Resource 挂订单三键（span 级身份经 `af.run_id` 链，run 级键见约定块） | af-execline-drive / af-otel-projector |
| L3 | `chat` | CLIENT | L2 | 同上 | af-execline-drive / af-otel-projector |
| L4 | `execute_tool` | BY_EFFECT（pure→INTERNAL；persist/external_once→CLIENT） | L2 | 同上 | af-execline-drive / af-otel-projector |

- **order_domain_keys**（订单域判定锚，P2 口径）：`af.order_id`、`af.work_order_id`。
- **resource_order_keys**（每条语义 span 的 Resource 必有）：`af.order_id`、
  `af.work_order_id`、`af.line`（与 span-conventions.md §6
  `resource.af_required_order_keys` 逐字一致，断言器对账）。
- 工厂族五名（`factory.*`）**不是**语义层：不进入本正本，不进四层树断言。

## 3. 三面消费落点（file:line 随提交定锚）

| 面 | 落点 | 消费方式 |
|---|---|---|
| 生产者（crate drive） | `crates/af-execline/src/span_produce.rs` | `include_str!` 编译进二进制；`materialize` 只从正本取层→名；正本缺席/坏 ⇒ 零行（非关键车道 fail-closed，不 panic 进 drive） |
| 生产者（在线 lane，未铺） | `crates/af-trace/src/otel.rs` | 见 §5 评估；本批零改动 |
| 投影器 | 工厂 `tools/otel/af-otel-projector.py` | 运行时读正本（`AF_SEMANTIC_ROSTER` 可覆盖；缺省 `<工厂工作区>/af/otel/semantic-roster.json`）；`roster_findings` 核「投影名 ⊆ 正本 ∧ Resource 三键 ∧ L1 span 三键」；selftest D 腿绿/红 |
| 断言器 | 工厂 `tools/otel/otlp-assert.py` | `main()` 载约定块后对账：名册键集/kind/父链/order_domain_keys/resource_order_keys/span_order_keys 须一致；不一致 FATAL exit 1；正本缺席 ⇒ stderr 注明、行为等价于旧版（不红） |

## 4. 变更纪律（谁改谁红）

1. 改正本 ⇒ 三面全红：crate 契约测试（层名钉死）、投影器 selftest D 红腿、
   断言器一致性 FATAL。夹具见 `poc/poc-a1-semantic-chain/raw/three-side-fixture.sh`。
2. 正本与 span-conventions.md §6 的共享投影（名册/kind/父/订单键）**必须同批改**：
   断言器对账是两份的最终防线，漂移即红。
3. 只增不减：新增语义层 span 名 = 正本增键 + 约定块增 `semantic_layers` 条目 +
   三面各自 producer 落地，缺一面不合并。
4. `unpaved_legs`（未铺腿评估）只是登记面，不参与任何对账。

## 5. 未铺腿评估：af-trace 在线 lane

亲读 `crates/af-trace/src/otel.rs`（RT-E138 poc README §① 与 span-conventions.md
附录 A 同口径）：worker 级单 trace_id、span 名=event.kind、属性五件套、绕过
TracerProvider 直喂 exporter（Resource 空）、kind 恒 Internal。语义化差距 =
D-1~D-9（kind→层名映射 / 身份上下文上提 / Resource 合成 / gen_ai.* 映射 /
kind 分派 / kind 白名单 / 点跨度口径 / 双 scope）。全部落在 af 核心 Rust 面，
本批**只评估不铺**（任务书：未铺的腿评估）；在线 lane 落地时以本正本为唯一
名册源，其 L3/L4 与投影器同 §1.0 派生式（同 run 重投影 ID 逐字节一致）。

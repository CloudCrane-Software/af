# poc-router-adjudicate — event-router 消费腿：af.trigger.v1 信封 → 裁决环

> 落地「adjudicate.rs 触发链」缺的一环。`crates/af-order/src/adjudicate.rs`
> 模块文档第 6 行的链是：
> `af-execline complete → MES 事件 → (event-router 消费腿，PoC-D 面) →
> generate_adjudication_doc → Station::enqueue → claim → seal → admit →
> complete`。本批把括号里那环接上：**一张真实的 af.trigger.v1 信封**（Trigger
> 收件箱真产出）→ 读侧验证 → 裁决目标**从信封派生**（判据：替代
> `examples/adjudicate_loop.rs` 腿①的「自己造一张源任务单」）→ 既有 steward
> 闭环事件驱动地跑通。
>
> 纯接线，零新机制：不新增 crate（root Cargo.toml members 未动）、不新增
> 依赖（af-order 零外部依赖纪律）、不造第二套队列/签名/状态机，全部复用
> adjudicate.rs / admit.rs / af-station / af-workorder 既有件。

## 1. 触发条件（这条链何时走）

```text
腿⓪ 上游   af-order CLI submit（真实 bin，--unit stub，真引擎门控）：
            源任务单 create→…→complete→close，MES 正本 mes-events.jsonl 落
            complete 事件。〔上游生产，不属消费腿——消费腿不造单〕
腿A 收件   mes_adapter.py 把 MES 正本的 complete 行包装成 af.trigger.v1
            webhook body（HMAC-SHA256 签名）→ POST 127.0.0.1:<port> 的
            trigger-inbox.py（PoC-D 收件箱工具，真跑起服）→ 验签/白名单/
            幂等全过 → 202 accepted，信封落
            <triggers-home>/inbox/mes-execline/<delivery-id>.json
腿B 消费   af-order example router_consume_leg：读信封 → 验证 → 派生裁决
            目标 → 防重放游标 → run_steward_loop（裁决单 → 入队 → 认领 →
            真签 → admit → complete）→ consumed.jsonl 落游标
腿C 重放   同信封再消费一次 → consumed 游标命中 → fail-closed 拒派 exit 1
```

## 2. 消费腿输入输出契约

**输入**（`router_consume_leg` 全显式参数，无隐式环境）：

| 参数 | 语义 |
|---|---|
| `--envelope <path>` | 收件箱落盘的 af.trigger.v1 信封（唯一事件入口） |
| `--state <dir>` | 源单 state 根（orders/<id>/ledger.jsonl、mes-events.jsonl、adjudications/） |
| `--station-db <path>` | af-station 队列库 |
| `--triggers-home <dir>` | triggers 根（registry.json 运行位 + consumed.jsonl 游标） |
| `--registry <path>` | 注册表（缺省 `<triggers-home>/registry.json`，缺席=红，fail-closed） |
| `--workspace/--target-dir/--actor` | 引擎定位与签字 actor（复现面固定种子） |

**输出**：stdout 逐腿 JSON 证据（9 腿）+ state 内裁决单/受理账本 +
consumed.jsonl 一行 + station 队列行。

**读侧纪律**（`crates/af-order/src/router.rs`，af.trigger.v1 契约 §4）：

- 唯一可信载荷 = `body_b64`（`payload_sha256` 自证；严格 base64 解码为
  模块内自实现——af-order 禁新增外部依赖）；
- 派单决策输入（order_id / stable_id / mes_event）一律从已验 body_b64
  **重解析**取；信封冗余字段（event_type / delivery_id / body 解析态）
  逐项与重解析核对，任一不符=拒读零派单；
- thread_key / dedup_key 按收件箱 `envelope_for` 兜底分支（非 git 事件
  `域:repo` / `etype:repo`）从注册表 + 已验载荷**重推导**核对；git 域
  事件（push/pull_request）fail-loud 拒收（归 PoC-D event-router）；
- 裁决目标与 state 对账：order_id 过 `validate_order_id`；stable_id 与
  账本行一致；mes_event 须 kind=complete、work_order_id=stable_id（MES
  事件按稳定身份记账）且在源单 mes-events.jsonl 正本**逐字存在**；
- 防重放两道：consumed.jsonl 游标（delivery_id 网络重试语义 + dedup_key
  业务重放语义，任一命中拒派）+ Station 入队幂等键 UNIQUE（第三道）。

## 3. 落地件清单

| 件 | 路径 | 说明 |
|---|---|---|
| 消费腿模块 | `crates/af-order/src/router.rs` | 信封验证/目标派生/消费游标/严格 base64；单测 7 例（含篡改三态全拒）。`lib.rs` 挂载 + 再导出，crate 集合未变 |
| 运行器 | `crates/af-order/examples/router_consume_leg.rs` | 9 腿 JSON 证据运行器（信封驱动；不造源单） |
| 集成测试 | `crates/af-order/tests/router_consume.rs` | 2 例：全链（真引擎+真信封同款口径）+ 篡改拒读零派单 |
| MES 适配器 | `poc/poc-router-adjudicate/mes_adapter.py` | MES 正本 complete 行 → webhook body → HMAC → POST |
| 驱动 | `poc/poc-router-adjudicate/run_poc.py` | 腿⓪/A/B/C 全链驱动 + 证据收口（sha256 清单/队列转储/元数据） |
| 证据 | `poc/poc-router-adjudicate/run/` | 见 §4 索引（随分支入库） |

## 4. 真跑一次：逐腿证据索引（run/ 下，相对本目录）

运行：2026-11-04T02:34Z，anolis-gpu-01，工作树 = product/main(83bb71c)
+ 本分支未提交改动（`run/run-meta.json` 有 HEAD 记录）。复现：
`CARGO_TARGET_DIR=/data/bench/af-target python3 poc/poc-router-adjudicate/run_poc.py`。

| 腿 | 证据文件 | 关键读数 |
|---|---|---|
| ⓪ 上游 submit | `run/logs/phaseA-submit.json`、`run/wo-pocr-leg-1-spec.json`、`run/state/orders/wo-pocr-leg-1/{ledger.jsonl,mes-events.jsonl}` | state=Closed；stable_id=sid-38f431a4…dad4a（由 order_id 确定性生成，跨跑同值）；账本 8 行、MES 4 事件（release/start/complete/close） |
| A 收件箱起服 | `run/logs/trigger-inbox.log`、`run/logs/phaseB-healthz.json` | 127.0.0.1:18765，healthz 200 ok（secrets_missing=[]，fail-closed 反面证据） |
| A 适配器投递 | `run/logs/phaseB-adapter-202.json` | 202 accepted；payload_sha256=405bb11e9c93d4db…（601 字节 body） |
| A 信封落盘 | `run/trigger-home/inbox/mes-execline/mes-wo-pocr-leg-1-complete-1.json` | 七字段+外层齐；thread_key=mes:line-poc/wo-pocr-leg-1、dedup_key=order_complete:line-poc/wo-pocr-leg-1 |
| A 探针×2 | `run/logs/phaseB-probe-duplicate-200.json`、`run/logs/phaseB-probe-badsig-401.json` | 同单重投 200 duplicate（不落第二份）；坏签名 401（fail-closed 验签） |
| B-1 信封验证 | `run/logs/phaseC-consume-legs.jsonl`（leg=envelope_verify） | body_b64 自证过、冗余字段核对过、注册表重推导过 |
| B-2 目标派生 | 同上（leg=adjudication_target） | order=wo-pocr-leg-1（**从信封派生**）、state=Closed、mes_event_id=sid-38f4…:complete:7:454520bd1da26908、与 MES 正本逐字对账 |
| B-3 游标 | 同上（leg=dedup_cursor / consumed_appended）、`run/trigger-home/consumed.jsonl` | fresh → 消费后落一行（契约 §4 六字段） |
| B-4/5 裁决单+入队 | `run/state/adjudications/wo-pocr-leg-1/adj-wo-pocr-leg-1-1.json`、`run/station-tasks.json` | doc_kind=裁决单、template=tpl-adjudication-v1、doc_sha256=7f63c4f19ff68fedaa711ebba6be85b992a15fe39f7987b605f46f9f7e7bb5d0（跨跑同值=可复算）；队列行 done/claimant=steward-1/幂等键=sid-38f4…/adj-wo-pocr-leg-1-1 |
| B-6 受理+闭环 | `run/state/orders/wo-pocr-leg-1/admissions.jsonl`、同上（leg=receipt_and_completion） | receipt 11 checks（含 basis_recheck/digest_recheck/credential_intersection）、decision=Approve、task done、CLI show 对拍 Closed/8 ops/4 events |
| C 重放拒派 | `run/logs/phaseC2-replay-reject.json` | rc=1，stderr `{"leg":"dedup_cursor","ok":false,…已于 … 消费…}` |
| 对账 | `run/evidence-sha256.txt` | run/ 下 26 文件 sha256 清单（voucher-key.seed 除外） |

**独立复算**（不入库命令，供验收）：`base64 -d` 信封 body_b64 →
sha256 == payload_sha256（本跑已复算一致：405bb11e…）；裁决单去
doc_sha256 字段 canonical JSON 重算 == doc_sha256（verify_doc 同口径）。

## 5. 门读数（最终树，2026-11-04）

| 门 | 命令 | 读数 |
|---|---|---|
| 测试 | `cargo test --workspace` | exit=0，**1483 passed / 0 failed**（含本批新增 router 单测 7 + 集成 2） |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | exit=0（初跑 2 条新 lint——needless_borrows_for_generic_args / manual_is_multiple_of——已修，复跑零警告） |
| 基线门 | `python3 -I ci/clippy_gate.py <clippy.jsonl>` | **CLIPPY GATE: PASS**（0 警告/0 签名；基线 0 签名未动，`ci/` 无 diff） |

## 6. 做不到的与需要人类裁决的（如实）

1. **裁决单线格式是 JSON，不是任务书写的 YAML**。既有件
   `AdjudicationDoc::persist` 落 pretty JSON（adjudicate.rs 既有线格式，
   属禁改面）；「YAML 正本」指配方层模板常量 `TEMPLATE_BODY`（YAML 形
   文本，adjudicate.rs 配方层留档），不是生成物线格式。本批未改。
   若确要 YAML 生成物线格式，属配方层决策，需人类裁决后另批改两侧。
2. **MES→webhook 适配器是「读正本」式接线，不是事件钩子**。mes_adapter.py
   从 mes-events.jsonl 正本取 complete 行再投递（语义=正本里确有的事件才
   可投递）；生产形态（af-execline MES 缝实时推送 / CH 对账面驱动）不在
   本批，需架构裁决。
3. **thread_key/dedup_key 兜底口径是数据约定**：MES 源把
   repository.full_name 定为 `<line_id>/<order_id>` 使业务对象单粒度。
   收件箱 envelope_for 兜底分支若改口径，消费腿重推导核对会 fail-closed
   红（镜像漂移即拒读）——需同批改两侧，这是有意的防漂移设计。
4. **非持驻消费**：消费腿是单信封一次性运行器（example），不是
   event-router 常驻 poll_triggers 循环（PoC-D 在役件，两批不共改一函数）。
   常驻化/批量消费不在本批。
5. **真跑只走 Approve 分支**：Reject/Revise/Arbitrate 分支由既有
   tests/adjudicate_loop.rs 全覆盖；消费腿不新增决策分支，只接线。
6. **凭证种子不入库**：`run/state/orders/*/voucher-key.seed`（submit 生成
   的 0600 秘密面）已 .gitignore；复算以 payload_sha256 / doc_sha256 /
   账本行为准（均跨跑可复算，见 §4）。
7. **沙箱密钥固定值**：`AF_TRIGGER_SECRET_MES_EXECLINE` 写死在 run_poc.py
   （127.0.0.1 本机 PoC 复现面；绝非生产秘密，生产走环境变量注入）。
8. **station.db 不入库**（根 .gitignore 全仓 `*.db` 惯例，含 -shm/-wal 侧车）；
   队列行证据 = `run/station-tasks.json`（sqlite 全表转储：task 行 id/
   required_role=steward/state=done/claimant=steward-1/幂等键），其哈希在
   `run/evidence-sha256.txt`（本机 run 树对账用）。

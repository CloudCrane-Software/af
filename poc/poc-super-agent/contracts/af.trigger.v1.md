# af.trigger.v1 — Trigger 收件箱事件契约（PoC-D 首版）

> 归属：v0.3 §二 阶段1 PoC D「事件层分工」；T5.2（收件端=独立 trigger-inbox.py，
> 不复用 portal——最小暴露面）、T6.3（inbox 契约合并版）。
> 三个落点：`tools/trigger-inbox.py`（收件/验签/落盘）、`tools/event-router.py`
> poll_triggers 消费腿（读侧）、本目录（契约与注册表正本）。
> 语义基线：Fast Path（webhook→收件箱，不入决策）与 Slow Path（事件→订单→
> DecisionArtifact）在此分家；webhook 只进收件箱，**不执行门套件**（PoC-D 纪律）。

## 1. 事件信封字段（七字段，缺一即 422）

| 字段 | 类型 | 语义 | 生成方 |
|---|---|---|---|
| `event_id` | string | 事件全局唯一 id（`<source>-<delivery_id>`） | 收件箱落盘时生成 |
| `thread_key` | string | `域:业务对象id`（§2） | 收件箱按注册表 domain 生成 |
| `payload_ref` | string | 原始 payload 落盘相对路径（`inbox/<source>/<delivery-id>.json`） | 收件箱 |
| `payload_sha256` | string | 原始 payload 字节流的 SHA-256 小写 hex | 收件箱 |
| `dedup_key` | string | 业务去重键（§3，防重放/幂等派单的第二道） | 收件箱按注册表从 payload 抽取 |
| `source` | string | 事件来源（注册表 `sources[].name`，如 `gitea-webhook`） | 请求方声明+注册表校验 |
| `received_at` | string | 收件时刻（ISO-8601 UTC） | 收件箱 |

补充信封外层字段（实现细节，非契约七字段）：`delivery_id`（幂等主键，取
payload 内 `delivery_id` 或 HTTP 头 `X-AF-Delivery`，校验 `^[A-Za-z0-9._-]{1,128}$`）、
`event_type`（白名单事件类型，如 `push`）、`body`（原始 payload 原样内嵌，字节精确）。

## 2. thread_key = 域:业务对象 id

沿用 `.router-state.json` 业务键惯例（`rr55-<task>`、`<order>:<status>:<reward>`
——前缀/域 + 业务对象 id，冒号分层）。本版两域：

- `git:<owner>/<repo>@<sha12>` —— push 事件，业务对象=仓库@提交
- `git:<owner>/<repo>#<pr_number>` —— pull_request 事件，业务对象=仓库#PR 号

域来自注册表 `sources[].domain`；同一业务对象多次事件共享 thread_key，
Slow Path 归并判定按 thread_key 聚合（同 thread_key 只挂同一订单线）。

## 3. dedup_key 与防重放（两道，正交）

1. **delivery 幂等**（收件箱层）：同 `<source>/<delivery_id>` 重复投递 → HTTP 200
   `{"status":"duplicate"}`，不重写文件、不重复派单。这是网络重试语义的幂等。
2. **业务去重**（消费腿层）：`dedup_key` = `event_type:repo:对象id:commit-sha`
   （push 用 after、pull_request 用 head sha）；消费腿把已派单 dedup_key 记入
   状态文件 `trigger_dedup`，重放投递（不同 delivery_id、同业务内容）不重复派单。
   派单侧第三道：`order_id` 确定性生成 = `sha256(repo + suite + subject + recipe_digest)`
   前 16 hex，`<SUITE>-<repo>-<hash16>`——同内容必然同单号，状态文件外仍可对账。

## 4. 存储形态（收件侧一文件一投递 + 读侧一行一事件游标）

- 收件侧（T5.2 最小暴露面 + 原子性）：`af-state/triggers/inbox/<source>/<delivery-id>.json`
  一投递一文件，tmp + `os.replace` 原子落盘（与 event-router save_state 同款）。
  文件内容 = 本契约信封（七字段 + delivery_id/event_type/body 外层）。
- 读侧（T6.3 glm 版 JSONL 游标语义）：消费腿每消费一投递，向
  `af-state/triggers/consumed.jsonl` 追加一行
  `{"delivery_id","source","event_type","dedup_key","order_id","consumed_at"}`；
  重启/重放时读侧以该 JSONL 为游标（防重放真源），状态文件 `trigger_consumed`
  为加速缓存——两者不一致以 JSONL 为准。

## 5. HTTP 面（tools/trigger-inbox.py）

- `POST /inbox/<source>`，`Content-Type: application/json`；
- 体积上限 1 MiB（超限 **413**）；HMAC-SHA256 验签头 `X-AF-Signature: sha256=<hex>`
  对原始 body 字节计算，密钥按 source 从环境变量 `AF_TRIGGER_SECRET_<SOURCE>`
  解析（注册表只登记 `key_id`，**秘密不落注册表**——T1 Ref 化纪律）；坏签名/缺头 **401**；
- 白名单：`source` 必须在注册表；`event_type`（body 内 `event` 字段）必须在
  `sources[].events`；`delivery_id` 可解析——违者 **422**；
- 合法 **202** `{"status":"accepted","delivery_id":...,"path":...}`；
  重复 delivery **200** `{"status":"duplicate"}`；
- 只读注册表 + 写 inbox 目录（+consumed 由消费腿写）：收件进程无其他写面。
  监听 `127.0.0.1:<port>`（默认 8765），不暴露公网（portal 三公网入口的前科）。

## 6. 注册表（配方/注册表落映射，不硬编码）

正本：本目录 `../registry/trigger-registry.v1.json`（版本控制）；运行位：
`af-state/triggers/registry.json`（PoC 安装副本，运行工具默认读运行位，
`AF_TRIGGER_HOME` 整体重定向供沙箱自检）。内容：

- `sources[]`：name/domain/events 白名单/key_id/hmac 算法；
- `repo_suites[]`：仓库 glob → 事件映射（`push`→`F1`，
  `pull_request opened|synchronize`→`F2`）——**映射表是数据不是代码**；
- `liveness`：收件箱活性安灯阈值（`inbox_silent_after_min`，超过即安灯，
  治"建成没事件可路由"——事件流 267/273.9h 静默前科）；
- `recipe_digest`：注册表内容自身的 SHA-256（order_id 确定性生成的输入之一）。

## 7. 边界与禁项（PoC-D 显式声明）

- webhook 只进收件箱：收件箱不派单、不执行门套件、不碰 MES——派单是
  event-router 消费腿的职责，执行是 F 门单/门套件的职责；
- MES complete→裁决单那条消费腿**不在本契约**（PoC-C 职责，两批不共改一函数）；
- 坏签名 401/超限 413/坏消息 422/合法 202/重复 200 是**验收口径**（五态全覆盖）；
- 收件箱对 payload 内容零信任：只验签与白名单，语义解释全部留给读侧。

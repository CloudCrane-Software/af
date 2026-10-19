# af-engine

无状态工单判定进程。编排（Temporal 上的 Python workflow）把订单快照整单送进来，进程整单送出去，然后退出。进程不保存订单，不读系统钟。

schema 来自 `af-workorder` 的 serde 派生。本进程不重定义 `WorkOrder`、`GateError`、`DecisionError`。

## 调用

stdin 一行 JSON，stdout 一行 JSON。只读第一行。

协议内的放行和拒绝都退出 0。判定看 `ok`。只有 stdout 写失败才退出非 0。stderr 不承载协议。

`engine_version` 是编译期 `CARGO_PKG_VERSION`（crate 版本）。运行时设置同名环境变量不会改响应。

## 请求

```json
{"op":"release","order":{}}
```

`order` 是完整的 `WorkOrder` 快照。各 op 的额外参数见下表。未列出的顶层字段被忽略。结构体上的未知字段按 `af-workorder` 的 serde 处理（派生实现不拒绝未知字段）。

检查顺序固定，命中即返回：空行 → 非法 JSON → 非对象 → `op` → `order` → 该 op 的参数 → 领域判定。

## 响应

放行：

```json
{"ok":true,"order":{},"engine_version":"0.1.0"}
```

拒绝：

```json
{"ok":false,"error":{},"engine_version":"0.1.0"}
```

`order` 与 `error` 不同时出现。

### error

| 情况 | 形状 |
|---|---|
| 未知 op、非法 JSON、缺字段、字段类型错、订单快照反序列化失败 | `{"kind":"BadRequest","message":"..."}` |
| 状态机、门控、登记窗口拒绝 | `{"kind":"Gate","gate":<GateError 的 serde>}` |
| 决策工件校验拒绝 | `{"kind":"Decision","decision":<DecisionError 的 serde>}` |

`gate` 与 `decision` 是 `af-workorder` 派生序列化的原样结果（外部标签枚举）。变体名和字段不在本进程里改写。`Missing` 仍是类型化清单，顺序保持库的固定顺序：产线绑定在前，预算凭证在后。

## op

映射到 `af-workorder` 的现有公共方法。薄适配只在参数名上：`attach_voucher` 调用 `WorkOrder::attach_budget_voucher`。因此 `MutationForbidden.op` 在这个 op 上仍是 `"attach_budget_voucher"`。

### 必经引擎

这些 op 带判定。编排层把判定交给本进程。

| op | 库方法 | 参数 | 判定 |
|---|---|---|---|
| `release` | `WorkOrder::release` | 无 | 已绑定产线版本，且预算凭证存在并 `signed` |
| `close` | `WorkOrder::close` | 无 | 存在 `is_final && qualified` 的工件 |
| `approve_decision` | `DecisionArtifact::validate_against` | `artifact`（`DecisionArtifact`）、`payload`（字符串） | 四层校验：payload 哈希、角色权限上限、决策所需权限、占位签名 |

`approve_decision` 不修改订单。工单类型上没有决策字段，`release` 的门控也不读决策。校验通过时，响应里的 `order` 与请求快照相同。校验接受库所接受的任何 `Decision`（包括 `Reject` / `Revise` / `Arbitrate`），本进程不另加「必须是 Approve」的规则。

`InProgress` 上调用 `close` 得到 `IllegalTransition`。这条边不存在，所以不会变成 `NoQualifiedFinalArtifact`。先看状态机边，再看门控。

### 允许编排层本地执行

这些 op 没有门控清单，只有状态机边或登记窗口。编排层可以不调用本进程，按 `af-workorder` 的同一规则改自己的快照。本进程仍然实现它们，编排层也可以选择全部走引擎。

| op | 库方法 | 参数 | 窗口 |
|---|---|---|---|
| `bind_line` | `WorkOrder::bind_line` | `line`（`LineBinding`） | 仅 `Created` |
| `attach_voucher` | `WorkOrder::attach_budget_voucher` | `voucher`（`BudgetVoucher`） | 仅 `Created`。未签名凭证可以登记，`release` 时才被点名 |
| `start` | `WorkOrder::start` | 无 | `Released` → `InProgress` |
| `attach_artifact` | `WorkOrder::attach_artifact` | `artifact`（`WorkArtifact`） | `InProgress` 或 `Completed` |
| `complete` | `WorkOrder::complete` | 无 | `InProgress` → `Completed` |
| `cancel` | `WorkOrder::cancel` | 无 | `Created` / `Released` / `InProgress` → `Cancelled` |

本地执行表示允许跳过本 bin。规则的权威实现仍是 `af-workorder`。之后再把快照送进 `release` 或 `close` 时，引擎只按快照里的事实做门控。

## 本进程不做的事

- 不改 `graph_version`。换版走变更单，不在本 op 集里。
- 不判定 SLA 逾期。`is_overdue` 要调用方注入时刻，本进程不提供对应 op。
- 不把「已审批」写进 `release` 门控。审批校验是 `approve_decision`，放行生产是 `release`，两步都由编排层依次调用。

## 样例

`tests/contract/` 下每个 `*.request.json` 是一行请求，同名的 `*.response.json` 是 stdout 的 JSON。

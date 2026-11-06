# PoC-bridge：Temporal 工作流 ↔ Rust 订单状态机的职责分界裁决

- id：`poc-bridge`，目录：`af/poc/poc-bridge/`
- 环境：Temporal CLI 1.9.1（dev server `--port 7240 --ui-port 8240`，Server 1.32.0，文件库 `data/temporal-dev.db`）+ Python temporalio 1.34.0（复用 `poc-temporal/.venv`，Python 3.14）+ rustc 1.99.1（引擎 `cargo build -j4`，debug）
- 全部结论来自本机真实执行（命令与退出/输出摘录见③）；结束后本任务所启 dev server 与全部 worker 已杀掉并确认 7240/8240 释放；AF 根 Cargo.toml md5 与开工前快照一致（`Cargo_root_snapshot.md5`，`b6d74b68e2df1e0d950eb90c61976587`）；未执行任何 git commit。

## ① 结论

**通过（裁决有明确赢家，附一个未实测的折中形态）**。

两种形态各实现同一个最小订单（1 个审批门 = signal `approve`；2 个 activity = `sign_voucher`、`forge_artifact`；1 个门控前置条件点 = release 门控，材料为产线绑定+已签凭证），各自真实跑完 4 组实验：

| 实验 | 形态A 薄壳（判定在 Rust bin） | 形态B 镜像（状态机+门控翻译进 Python） |
|---|---|---|
| E1 正常流 | ✅ Completed，履历 Created→Released→InProgress→Completed | ✅ 同左，结果逐字段一致 |
| E2 kill -9 worker 后续跑 | ✅ failover ≈5.9s（heartbeat_timeout 5s 路径），重放不重跑已判跃迁 | ✅ failover ≈5.5s，同常数 |
| E3 加新前置条件（release 需 SLA deadline） | ✅ workflow 代码 **0 行改动**；引擎二进制热替换（不重启 worker）：已过门在飞 run 无感完成（混版本履历 v1 判 release、v2 判 complete）；未到门在飞 run 与新 run 被类型化拒绝 `[SlaDeadlineNotSet]` | ⚠️ 代码 1 文件 +3 行，但**已过门控点的在飞 run 重放直接中毒**：`WORKFLOW_TASK_FAILED [TMPRL1100] Nondeterminism error`，run 卡死（>1min 无新事件，只能 terminate）；未记录点的在飞 run 干净拒绝 |
| E4 CAN 续接 | ✅ 同 workflow id 换 run_id，`runs_done=1`，sign_voucher 跨 run 恰好 1 次，旧 run 尾事件 `WORKFLOW_EXECUTION_CONTINUED_AS_NEW`，query 路由新 run | ✅ 同左 |

**裁决：状态机+门控的"判定权"归 Rust（形态A），编排归 Python workflow。** 决定性证据是 E3：门控规则变更是运营常态，形态A把这类变更的爆炸半径锁在一个无状态 bin 里（Python 侧零改动、在飞 run 不中毒、引擎可热替换、可按需路由引擎版本），形态B的等价变更让已记录门控点的在飞 run 直接变成不可恢复的毒 run。形态A的可量化代价见③-5（代码 +101 行；每单事件历史 57 vs 21 个）。折中形态（只有**门控**跃迁才调引擎，无门控跃迁留在编排层）未实测，见⑤-1。

## ② 步骤

1. **读前置材料**：`af-workorder` 的 `RESULT.md` + `src/{lib,state,order,error,sla}.rs`（门控结构：`transition()` 先查状态机边再查门控，`Missing` 类型化清单，`WorkOrder` 全量 serde）；`poc-temporal` 的 `RESULT.md`④ + `VERIFY.md`（replay 纪律、CAN 参数类型坑、worker kill 两个时间常数、心跳纪律）。
2. **形态A 引擎**（`engine/`，独立 cargo 工程、空 `[workspace]` 表防并入 AF workspace，path 依赖 `af-workorder`）：stdin 收 `{"op","order","transition"/"line"/"voucher"/"artifact"}`，stdout 回 `{"ok","order"/"error","engine_version"}`；每次调用无状态，订单快照整体进整体出，schema 完全由 af-workorder 的 serde 派生（`GateError` 本身 `derive(Serialize)`，错误 JSON 原样透传）。
3. **两形态 Python**（`form_a/`、`form_b/`，各 4 文件，同一外部系统模拟 `ledger.py` 仅数据文件不同）：`workflow.py`（流程编排：bind_line→A1 sign_voucher→审批门→release 门控→start→A2 forge_artifact（8s 心跳活动）→complete；`use_can=True` 时 release 后 continue-as-new）、`worker.py`、`client.py`（start/approve/status/wait）。形态B的 `workflow.py` 另含状态机全量翻译（`ALLOWED`/`TO_STATE` 边表、`release_missing`/`close_missing`、先查边再查门控的 `transition()`、三类 `GateFailure`）。
4. **实验**：dev server 7240/8240；两形态各自独立 task queue（`bridge-queue-a/b`），互不串队。E1 正常流各一单；E2 各在 A2 在飞时 `kill -9` worker 后起新 worker；E3 新前置条件=「release 需 SLA deadline 已设置」（形态A改引擎 v1→v2 并 `cargo build`，形态B改 `workflow.py` 的 `release_missing` 并重启 worker），每形态观察 4 个 run：已过门的在飞 run、停在门前的在飞 run、换后新 run（无 deadline）、换后新 run（带 deadline）；E4 各跑 `--can` 一单。涟漪测量以 `exp/v1-baseline/` 的 v1 副本为基线做 diff。

## ③ 证据

### 1) E1 正常流（命令 + 输出）

`client.py start e1a / e1b` → `approve` → `wait`，两形态 result 相同（节选）：

```json
{"final_state": "Completed",
 "history": [{"from": "Created", "to": "Released"},
             {"from": "Released", "to": "InProgress"},
             {"from": "InProgress", "to": "Completed"}],
 "trace": ["started","line_bound","voucher_signed","approved_by=signal:alice",
           "released","started_work","artifact_forged","completed"], "runs_done": 0}
```

`temporal workflow list`：`Completed e1a BridgeOrderA`、`Completed e1b BridgeOrderB`。
历史文件：`exp/e1a-history.json`（**57 事件 / 40686 B**）vs `exp/e1b-history.json`（**21 事件 / 13275 B**）——薄壳每次跃迁/登记都是一次引擎 activity。
引擎侧轨迹 `data/engine_calls.log`（e1a 共 6 次调用，全部 `engine_version:v1, ok:true`）：bind_line→attach_voucher→release→start→attach_artifact→complete。

E1 期间两次真实门控拦截（流程 bug 被门控抓住，即门控有效的证据）：首版流程漏了 `Start` 跃迁，两形态在同一位置同一规则拒绝——
形态A：`ApplicationError: MutationForbidden: engine rejected attach_artifact: 状态 Released 下禁止操作 attach_artifact`（engine JSON，对应 `order.rs:174-183`）；
形态B：`ApplicationError: GateFailure: MutationForbidden: {"state": "Released", "op": "attach_artifact"}`。

### 2) E2 kill -9 worker（命令 + 事件历史）

过程：`status` 轮询到 A2 在飞 → 记录 kill 时刻（`exp/e2a2-kill-time.txt`：`11:19:16.111`）→ `kill -9 <worker-pid>` → 立即起新 worker → `wait` 终态。
`exp/e2a2-history.json`（时间 UTC，本地=UTC+8）：

```
e39 03:19:15.992 ACTIVITY_TASK_SCHEDULED  act=forge_artifact
e40 03:19:21.999 ACTIVITY_TASK_STARTED    attempt=2 identity=557621  lastFailure="activity Heartbeat timeout"
e41 03:19:30.045 ACTIVITY_TASK_COMPLETED
（终态 COMPLETED；e45-53 为 kill 后由新 worker 首次执行的 attach_artifact/complete 引擎调用）
```

kill→attempt2 = **5.9s**（heartbeat_timeout 5s + 重试 1s）。形态B `exp/e2b2-history.json`：kill `11:20:41.557` → attempt2 `03:20:47.096` = **5.5s**，同常数，终态 COMPLETED。ledger 计数 `attempts=1`（attempt1 在产出前被杀，未触达外部系统；两形态一致）。
**反例证据（无 heartbeat_timeout 的代价）**：首版未配 `heartbeat_timeout`，`exp/e2a-history.json` 显示 forge attempt2 于 kill 后 **60.4s** 才起飞（`lastFailure="activity StartToClose timeout"`，03:16:58.981→03:17:59.409）——活动里调了 `heartbeat()` 但不配超时毫无用处，与 VERIFY.md ⑤-5 完全一致。两形态随后统一补上 `heartbeat_timeout=5s`（`_long_activity_options()`）。
**形态A"重放不重判"的直接证据**：`data/engine_calls.log` 中 e2a2 的 release(11:19:15.696)/start(11:19:15.857) 由被杀 worker 执行且恰好一次；kill 后新 worker 只执行了 attach_artifact/complete 两次新调用。

### 3) E3 新前置条件「release 需 SLA deadline」（命令 + 输出）

改动内容 diff（基线 `exp/v1-baseline/`）：
- 形态A：`exp/ripple-A-engine.diff`——仅 `engine/src/main.rs` 1 文件，+14/−5=19 行（含版本号 v1→v2 与注释）；**`form_a/workflow.py` 与基线 diff 为空**（`diff -q` 无差异）；`cargo build -j4` 0.28s；引擎热替换，worker 不重启。
- 形态B：`form_b/workflow.py` 1 文件 +3 行（`SlaDeadlineNotSet` 检查），需重启 worker 加载新代码。

**形态A**（`data/engine_calls.log`）：
```
e3a-old: 11:23:19.936 release ok=true  engine_version=v1   ← 在飞 run 已过门，v1 判定入历史
         11:23:28.520 attach_artifact ok=true engine_version=v2  ← 热换后同一 worker 进程调用 v2
         11:23:28.690 complete ok=true engine_version=v2   → 终态 Completed（混版本履历，零中断）
e3a-mid: 11:23:39.561 release ok=false v2 missing=["SlaDeadlineNotSet"] → run Failed（干净拒绝）
e3a-new: 11:24:02.642 release ok=false v2 missing=["SlaDeadlineNotSet"] → run Failed
e3a-ok (--deadline): release ok=true v2 → Completed
```

**形态B**：
- `e3b-old3`（v1 代码过门、A2 在飞时换 v2 代码+杀 v1 worker）：forge attempt2 于 5.5s 后由 v2 worker 接管完成，随后重放撞已记录门控点——
  `exp/e3b-old3-history.json` 事件 20：`WORKFLOW_TASK_FAILED`，消息 `[TMPRL1100] Nondeterminism error: Fail workflow machine does not handle this event: HistoryEvent(id: 15, ActivityTaskScheduled)`；
  `exp/e3b-old3-describe.txt`：SearchAttributes 出现 `TemporalReportedProblems: category=WorkflowTaskFailed, cause=WorkflowTaskFailedCauseNonDeterministicError`；worker 端日志 `logs/worker-b8.log` 同文。
  观察 >1min 事件数停在 20、无任何推进（run 卡死），只能 `workflow terminate`（已执行）。
- `e3b-mid`（门控点**未记录**时已在门前等待，v2 worker 上审批）：`ApplicationError: GateFailure: PreconditionsNotMet: {"transition": "Release", "missing": ["SlaDeadlineNotSet"]}` → run Failed，**干净拒绝、不中毒**。
- `e3b-ok`（--deadline，v2）：Completed。
- 构造过程如实记录：前两次尝试（e3b-old、e3b-old2）因 kill 落在 8s 活动窗口之外（pgrep 误中 bash 包装进程、换码动作链路过长），run 在旧代码下正常完成，未构成对照；`e3b-old3` 用"预置文件+单命令内 cp/kill/nohup"把换码压进窗口内成功命中。

### 4) E4 CAN 续接（命令 + 输出）

`client.py start e4a --can --deadline` → approve → result：`runs_done=1`，trace `... released → started_work → continue_as_new | artifact_forged → completed`（CAN 前后各段），终态 Completed；`e4b` 同。
```
temporal workflow list --query 'WorkflowId="e4a"'：
  Completed       e4a  BridgeOrderA   ← 新 run
  ContinuedAsNew  e4a  BridgeOrderA   ← 旧 run（首 run_id 01a15236-9816...）
旧 run 尾事件：e39 WORKFLOW_EXECUTION_CONTINUED_AS_NEW（旧 run 共 39 事件）
最新 run 共 23 事件（历史重新开始）
ledger：sign_voucher attempts=1（跨 CAN 只签一次，A/B 同）
query status：run_id=52bbfcb0...（新 run），phase=2，order_state=Completed
```
两形态 CAN 参数都走各自 `StateA`/`StateB` dataclass（run 签名同类型，规避 poc-temporal RESULT.md④ 的 CAN 参数坑），订单快照作为字段随身携带。

### 5) 代码量与涟漪（wc -l / diff 实测）

| 项 | 形态A 薄壳 | 形态B 镜像 |
|---|---|---|
| 判定逻辑 | Rust `engine/src/main.rs` 182 行 + `Cargo.toml` 12 行 | Python（在 workflow.py 内，边表+门控+登记守卫） |
| 编排层 | `workflow.py` 198 | `workflow.py` 303（含状态机翻译） |
| worker/client/ledger | 29+42+53 = 124 | 30+35+47 = 112 |
| **合计** | **516 行（Python 322 + Rust 194）** | **415 行（纯 Python）** |
| 每单事件历史（E1） | 57 事件 / 40.7 KB | 21 事件 / 13.3 KB |
| 加 1 条前置条件的涟漪 | 1 文件（引擎）19 行；Python 0 文件 0 行；无需重启 worker（实测热替换）；需 `cargo build` | 1 文件 +3 行；必须重启 worker；已记录点的在飞 run 中毒（需 patched/versioning 兜底） |
| IDL 形态 | 显式 JSON 契约 = af-workorder 类型的 serde 派生（`GateError`/`Missing`/`OrderState` 全部 serde 化）；Python 零解释，坏请求由引擎 BadRequest 兜底；无独立 schema 文件 | 无跨语言 IDL；"契约"= Temporal payload 的 dataclass 类型提示（按 poc-temporal 踩坑②，类型提示随历史固化，换代码即毒，与 E3-B 的 TMPRL1100 同一纪律） |

产物清单：`engine/`（引擎 v1/v2）、`form_a/`、`form_b/`、`exp/`（v1 基线、ripple diff、E1-E3 历史 JSON、kill 时刻、e3b-old3 describe）、`data/`（ledger-a/b.json、engine_calls.log、temporal-dev.db）、`logs/`（server/worker 全部日志）。

## ④ 对方案的反馈/决策输入

1. **职责分界建议（本批最高优先级裁决）**：判定权（状态机合法边 + 门控前置条件）归 Rust、编排（审批门/timer/步骤次序/CAN）归 Python workflow，即形态A。理由不是代码量（A 多 101 行），而是**变更语义**：门控规则会随运营演进，E3 实证 A 的规则变更 = 换一个无状态 bin（Python 零改动、在飞 run 永不中毒、旧判定永久留在历史里可审计），B 的规则变更 = 一次必须配 patched()/Worker Versioning 的 workflow 代码发布，漏配即在飞 run 变毒 run（TMPRL1100，实测卡死无自愈）。B 并非不可运维（poc-temporal VERIFY 的 B 项证明 patched() 能解），但每一次门控变更都要人记得做版本决策，认知负担是永续的。
2. **af-workorder 仍是唯一模型权威**：引擎只是它的壳（本 PoC 用 path 依赖实证可行，`engine/Cargo.toml` + serde 全自动契约）。新前置条件在真实方案中应加在 `af-workorder::WorkOrder::release_missing`（`Missing` 枚举 1 变体 + 1 行检查）；本 PoC 因"只在任务目录写文件"纪律加在引擎层（`exp/main_v2.rs` 内有注明），改动量同类。
3. **薄壳的正确粒度是"每次门控"而不是"每次跃迁"（最有价值的折中，未实测）**：本 PoC 把 bind/attach/无门控跃迁也送进引擎（6 次调用/单），事件历史膨胀到 2.7 倍。真实方案建议只有带门控的跃迁（release/close）走引擎，无门控跃迁与登记类操作留在编排层本地——判定语义零损失，引擎调用与历史事件显著下降。该折中的具体收益数字需要补一个实验（⑤-1）。
4. **薄壳仍有一个跨语言读点**：编排层要读 `order.state` 决定"下一步做什么"（`workflow.py` 中 `s.order.get("state") == "Created"` 等三处）。这不是门控泄漏（能不能做仍全问引擎），但 `state` 字符串本身成为 A 的 JSON 契约一部分——af-workorder 的 `OrderState` serde 形态一旦变，Python 编排要跟着改。建议在引擎响应里保持显式 `state` 字段（已做）并在真实方案把它列为契约字段。
5. **心跳纪律第 3 次实证（对全批有效）**：`heartbeat()` 调用不配 `heartbeat_timeout` 时崩溃检测退化为整段 StartToClose 白等（实测 60.4s vs 配置后的 5.9s/5.5s）。底座 activity 编写规范应强制"长活动必须同时有 heartbeat 与 heartbeat_timeout（≈2-3 倍心跳间隔）"。
6. **引擎进程形态**：PoC 用每调用一子进程（stdin/stdout），编排侧只需 `subprocess.run`，实现成本最低；本 PoC 未对单次调用延迟做基准（⑤-5）。若真实方案嫌子进程开销大，可平滑升级为常驻引擎 + Unix socket，协议不变。
7. **对 IDL 决策**：A 的 JSON 契约靠 serde 派生即可，不需要手写 schema 文件（实测 serde 的 unit variant→字符串、struct variant→单键对象形态 Python 侧零摩擦）；但注意 serde 默认忽略未知字段，Python 侧传多余字段不会报错，跨语言防呆靠引擎的 BadRequest 路径（已实现，含"订单快照不合法"消息）。

## ⑤ 遗留

1. **折中形态未实测**：「只有门控跃迁调引擎」的引擎调用数/事件数/涟漪对比没有跑，④-3 的收益是推断（基于 E1 的 6 次 vs 2 次门控调用结构），需要补实验落地数字。
2. **E3-A 的事件历史未逐 run 归档**：e3a-old/mid/new/ok 的证据以引擎轨迹（`data/engine_calls.log`）+ workflow list 终态为准，历史 JSON 只归档了 e1a/e1b/e2a/e2a2/e2b2/e3b-old3。
3. **重投递细节沿 VERIFY 结论未重测**：纯 workflow 等待中被杀的 10s 粘性队列惩罚（VERIFY A3）在本任务未复现（E2 都杀在 activity 在飞窗口）。
4. **形态B中毒的 patched()/Worker Versioning 解法**未在本任务重测，沿用 poc-temporal VERIFY 的结论（patched() 按代码点判定、v2 Worker Deployments 是现行版本化形态）。
5. **引擎单次调用延迟未基准化**（子进程 fork + serde 的量级未测）；ledger 只模拟签发/产出，无对账语义。
6. **环境遗留**：poc-temporal 早前启动的 7233 dev server（pid 3389440，db=/tmp/temporal-dev.db）非本任务所启，未动；本任务所有进程（7240 dev server、全部 worker）已退出并确认端口释放。form_b/workflow.py 与 engine/src/main.rs 当前停留在 v2 内容（v1 基线在 `exp/v1-baseline/`）。

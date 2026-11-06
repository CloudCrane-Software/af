# VERIFY：Temporal PoC 遗留项实测（worker 崩溃重投递 + 工作流版本化）

- 目录：`af/poc/poc-temporal/verify/`（代码）、`af/poc/poc-temporal/verify-logs/`（日志/事件历史）
- 环境：Temporal CLI 1.9.1 独立 dev server（`--port 7234 --ui-port 8234 --db-filename /tmp/verify-temporal-dev.db`，Server 1.32.0）+ Python temporalio 1.34.0（.venv，Python 3.14）
- 全部结论来自本机真实执行；命令与退出码关键处已注明。结束后已杀掉本验证起的 dev server 与全部 worker（原 PoC 的 7233 server 未动）。

## ① 结论

| 验证项 | 结果 | 一句话 |
|---|---|---|
| A：worker `kill -9` 后 activity 任务重投递 | **通过** | activity 层靠 heartbeat timeout（杀后 ~2.4-5.8s 重投递，attempt 递增）；无心跳时靠 StartToClose（整段 SLC 白等）；双 worker 接管与"第一个 activity 不重跑"均实证 |
| A-附：workflow task 层被杀（两个 activity 之间） | **通过** | timer 触发的 workflow task 会先落进死 worker 的粘性队列，吃满一个 10s 的 `WORKFLOW_TASK_TIMED_OUT(SCHEDULE_TO_START)` 才 failover 到新 worker——这是实测出的隐藏延迟 |
| B：`workflow.patched()` / `deprecate_patch()` 生命周期 | **通过（语义与文档直觉有重大出入，见④）** | patched() 的判定粒度是"这个代码点是否还在重放历史"，不是"run 是否旧代码启动"：在飞旧 run 在未定决策点会被**补丁进新逻辑**；已定决策点则安全沿旧分支续跑。两者都不中毒，PoC 踩坑③的"只能 terminate"确实被 patched() 解掉 |
| 加分项：Worker Versioning | **通过（v2 Worker Deployments 形态）** | v1 build-id API 在 CLI 1.9.1/Server 1.32 已不可用；用 v2 `deployment_config` 实证双版本并存、新 run 路由 current、在飞 run 钉原版本（PINNED）、旧版本自动 drainage=drained |

## ② 步骤

环境：`bin/temporal server start-dev --port 7234 --ui-port 8234 --db-filename /tmp/verify-temporal-dev.db --log-level error`（后台，pid 存 `verify-logs/server.pid`）。所有 workflow/activity 代码在 `verify/`：`kill_wf.py`+`kill_worker.py`+`drive_kill.py`（A 项）、`patch_v1/v2/v3.py`+`patch_worker.py`+`drive_patch.py`（B 项，同一 workflow 类型名 `PatchWorkflow` 同 queue，worker 按代换加载模块模拟"换部署代码"）、`wv_worker.py`+`drive_wv.py`（加分项）。activity 与 workflow 都向 `verify-logs/activity_trace.log` / `patch_trace.log` / `wv_trace.log` 写带毫秒时间戳的轨迹（activity 行含 `attempt/worker角色/pid`，workflow 行含 `unsafe.is_replaying()`）——为此 worker 用了 `UnsandboxedWorkflowRunner()`（生产禁用，仅 PoC 观测用；首次运行确实被默认沙箱拦下 `time.time`，worker log 有完整栈）。

**A1 杀后重启**：起 worker w1 → 启动 `kill-a1`（step1 activity 2s + step2 activity 10s，heartbeat 每秒、heartbeat_timeout=3s、SLC=60s、RetryPolicy initial 1s/backoff 1.5/maximum 10）→ step2 在飞 2.5s 时 `kill -9` w1（记录进 trace）→ ~5s 后起新进程 w1b → 等 result、取 `workflow show --output json`。
**A2 双 worker 接管**：队列上常驻两个 worker（w2a/w1a）→ 启动 `kill-a2c`（step2=14s）→ 从 trace 动态取真正执行 attempt=1 的 worker pid，`kill -9` 后**保持死亡**→ 死亡窗口内 `workflow describe` 取证 → 由另一个常驻 worker 接手。
**A3 两个 activity 之间被杀**：`kill-a3` 在 step1 与 step2 之间插入 4s 纯 `workflow.sleep` 定时器窗口（把"之间"变成可精确命中的窗口）→ 在窗口内 `kill -9` w3a → 1.5s 后重启 w3b → 观察重放轨迹与事件历史。
**A4 无心跳/StartToClose 路径**：`kill-a4`（heartbeat=False，SLC=25s，step2 sleep=25s）→ 在飞 5.3s 时杀 w4a（不再回来）→ t+12s 提前加入 w4b（验证能否"偷"任务）→ 死亡窗口两次 `describe` → 等终态。
**B patched 三代**：v1 worker 启动 `patch-run-old`（sleep 20s 换码窗口）→ t+5 杀 v1 worker、上 v2 worker → timer 触发后由 v2 代码接管完成；`patch-run-old2` 同构造，但杀 worker 的时机选在**分支 activity 已在飞 1.7s**（决策点已落旧命令）；`patch-run-new` 直接由 v2 worker 新启；切 v3 worker 跑 `patch-run-v3`；负例 `patch-run-mix`：v1 run 分支 activity 在飞时杀 worker、直接换 **v3** worker（故意违反 deprecate 前提）。每个 run 都存 `--output json` 历史并解码 `core_patch` marker。
**加分项**：`worker deployment describe/list`、`set-current-version`（CLI 交互确认需管道喂 `y`）、`deployment_config=WorkerDeploymentConfig(version=..., use_worker_versioning=True, default_versioning_behavior=VersioningBehavior.PINNED)` 双 worker（build 1.0/2.0）并存同 queue；`wf-wv-pin`（切流前启动、持信号）与 `wf-wv-c`（切流后新启）对照路由。

## ③ 证据（均注明来源文件）

**A1 杀后重启，attempt 递增、重投递延迟 ~5.8s** — `verify-logs/activity_trace.log`（时间线摘录）：
```
[07:03:05.844] ACT started  tag=step2 attempt=1 worker=w1  pid=3843421
[07:03:08.380] OPS kill -9 worker w1 pid=3843421          ← 死亡，无 worker 在队列
[07:03:13.421] OPS restart worker w1b
[07:03:14.145] ACT started  tag=step2 attempt=2 worker=w1b pid=3844487
[07:03:24.160] ACT completed tag=step2 attempt=2 worker=w1b
[07:03:24.200] WF run wid=kill-a1 replaying=True           ← 新 worker 上重放
[07:03:24.204] WF schedule tag=step2 replaying=True
```
重投递延迟 = kill→attempt2 开始 = 5.77s ≈ heartbeat_timeout 3s + 重试 initial 1s + 无 poller 空等 ~1.7s。
`temporal workflow show --workflow-id kill-a1` → **COMPLETED**，result：`step1 attempt=1 worker=w1 / step2 attempt=2 worker=w1b`。历史（`verify-logs/kill-a1-history.json`）：
```
event 12 ACTIVITY_TASK_STARTED attempt=2
  lastFailure: "activity Heartbeat timeout" TIMEOUT_TYPE_HEARTBEAT   ← 被杀的 attempt1 只以此形式留痕
event 13 ACTIVITY_TASK_COMPLETED
```
细节：attempt=1 的派发在历史里**没有留下任何事件**（无 Started/TimedOut），心跳超时以 attempt=2 Started 事件的 `lastFailure` 记录。

**A2 双 worker 接管（受害者保持死亡）** — `verify-logs/activity_trace.log`：
```
[07:07:58.137] ACT started tag=step2 attempt=1 worker=w1a pid=3850514
[07:08:00.780] OPS kill -9 victim worker=w1a pid=3850514 (stays dead)
[07:08:03.160] ACT started tag=step2 attempt=2 worker=w2a pid=3850278   ← 常驻第二 worker 接手
```
延迟 2.38s（纯 heartbeat_timeout，队列上始终有 poller）。死亡窗口取证 `verify-logs/kill-a2c-describe-deadwindow.txt`：`State=Started Attempt=1 LastHeartbeatTime 2 seconds ago LastHeartbeatDetails ["progress=1.0/14.0"]`。终态 COMPLETED，result：`step2 attempt=2 worker=w2a`。

**A3 两个 activity 之间被杀 → 从第二个 activity 继续** — `verify-logs/kill-a3-history.json`（时刻为 UTC）：
```
 5 23:09:52.401 ACTIVITY_TASK_SCHEDULED (step1)
 7 23:09:54.413 ACTIVITY_TASK_COMPLETED  (step1，只此一次)
11 23:09:54.424 TIMER_STARTED            ← 两 activity 之间的纯定时器
12 23:09:58.426 TIMER_FIRED
13 23:09:58.426 WORKFLOW_TASK_SCHEDULED
14 23:10:08.428 WORKFLOW_TASK_TIMED_OUT timeoutType=SCHEDULE_TO_START   ← 挂满 10.002s！
15 23:10:08.428 WORKFLOW_TASK_SCHEDULED
16 23:10:08.435 WORKFLOW_TASK_STARTED identity=3858954(w3b)
19 23:10:08.455 ACTIVITY_TASK_STARTED attempt=1 (step2，全新 attempt)
```
对应 trace（`verify-logs/activity_trace.log`，grep kill-a3）：`WF schedule tag=step1 replaying=True`（重放经过 step1 代码但**没有**新的 ACT started 行 → activity 未重跑）→ `WF schedule tag=step2 replaying=False` → `ACT started tag=step2 attempt=1 worker=w3b`。终态 COMPLETED，result：`step1 attempt=1 w3a / step2 attempt=1 w3b`。同一惩罚在 B 项 `patch-run-old2-history.json` 复现（事件 13→14：SCHEDULED 23:18:20.115 → TIMED_OUT SCHEDULE_TO_START 23:18:30.117）。

**A4 无心跳：StartToClose 驱动 + 重试耗尽** — `verify-logs/kill-a4-describe-t8.txt`（杀后 3.5s）：`State=Started Attempt=1 LastWorkerIdentity 3862587(死者)`，无心跳信息，server 全盲；提前加入的 w4b **抢不走**任务；`verify-logs/activity_trace.log`：
```
[07:11:44.815] ACT started tag=step2 attempt=1 worker=w4a   ← SLC 从此起算
[07:11:47.492] OPS kill -9 worker w4a (stays dead; NO heartbeat)
[07:12:10.815] ACT started tag=step2 attempt=2 worker=w4b   ← 精确 = attempt1 开始 +25s(SLC) +1s(重试)
```
我方设计缺陷：step2 sleep=25s == SLC=25s，此后每次重试同样踩线超时（trace 可见 attempt=3/4 相继 started），10 次（maximum_attempts=10）耗尽后 `kill-a4-history.json` 终态 `WORKFLOW_EXECUTION_FAILED "Activity task timed out"`。教训（真实执行得出）：SLC 必须显著大于活动实际耗时，否则重试策略变成确定性失败发生器。

**B-① 在飞旧 run 在未定决策点被"补丁进新逻辑"** — `patch-run-old`（v1 启动，杀 v1 worker 后 v2 worker 接管 timer 触发点），`verify-logs/patch_trace.log`：
```
[07:16:02.553] WF[v1] run wid=patch-run-old replaying=False
[07:16:22.591] WF[v2] patched('add-fastpath') -> True wid=patch-run-old replaying=False  ← 注意 replaying=False
[07:16:22.606] ACT started path=v2-fastpath worker=pv2
```
result：`{"paths":["recorded:v2-fastpath"]}`；历史 `verify-logs/patch-run-old-history.json` 事件 10：
`MARKER_RECORDED markerName=core_patch patch-data={"id":"add-fastpath","deprecated":false}` —— **旧 run 自己的历史里被写入了 PATCH marker**。

**B-①' 决策点已定（分支 activity 在飞被杀）→ patched() 返回 False，安全沿旧分支续跑（防中毒实证）** — `patch-run-old2`，`verify-logs/patch_trace.log`：
```
[07:18:11.072] ACT started path=v1-path attempt=1 worker=pv1b
[07:18:12.762] OPS kill -9 v1 worker MID-BRANCH-ACTIVITY
[07:18:16.100] ACT started path=v1-path attempt=2 worker=pv2b   ← activity 无版本概念，重试照跑
[07:18:30.141] WF[v2] patched('add-fastpath') -> False wid=patch-run-old2 replaying=True  ← 重放中且无 marker
[07:18:30.145] WF[v2] branch-taken=v1-path(old-run)
```
result：`{"paths":["recorded:v1-path"]}`；`patch-run-old2-history.json` 全程 **0 个 marker、0 个 WorkflowTaskFailed**，COMPLETED。

**B-② 新 run（v2）**：`patch-run-new` → `patched -> True` → `v2-fastpath`，marker `{"id":"add-fastpath","deprecated":false}`（`verify-logs/patch-run-new-history.json`）。

**B-③ v3 deprecate**：`patch-run-v3` → `deprecate_patch('add-fastpath') called` → 无条件 `v2-fastpath`，marker `core_patch {"id":"add-fastpath","deprecated":true}`（`verify-logs/patch-run-v3-history.json`）。

**B-负例 deprecate 过早（v3 撞上在飞无 marker 旧 run）**：`patch-run-mix`（v1 分支 activity 在飞时杀 worker、v3 worker 接管），`verify-logs/patch_trace.log`：
```
[07:20:48.231] WF[v3] deprecate_patch('add-fastpath') called wid=patch-run-mix replaying=True
[07:20:48.233] WF[v3] branch-taken=v2-fastpath(unconditional)
```
`patch-run-mix-history.json`：total_events=18，**WorkflowTaskFailed_count=0，marker_count=0**，COMPLETED，result `{"paths":["recorded:v1-path"]}`——v3 代码的 `execute_activity("v2-fastpath",1.0)` 在重放中直接消费了历史里旧分支 `v1-path` 的 recorded 结果（temporalio 1.34 的重放按活动槽位匹配、不比对 args），**静默错位而非响亮失败**。

**加分项 Worker Deployments** — `verify-logs/wv_trace.log`：
```
[07:26:30.331] ACT b executed by build=1.0     ← current=1.0 时新 run 路由 1.0
[07:27:01.977] ACT c executed by build=2.0     ← set-current-version 2.0 后的新 run → 2.0
[07:27:05.227] ACT pin executed by build=1.0   ← 切流前启动、切流后才放行的在飞 run → 仍钉 1.0
```
`verify-logs/wv-deployment-describe.txt`：`CurrentVersionBuildID 2.0`，版本表 `2.0 unspecified / 1.0 drained`（旧版本被自动判定排水完成）。坑（真实踩到）：无 current version 时启动的 workflow task 无法路由、挂在队列（wf-wv-a 首个 WorkflowTaskStarted 迟迟不出现），`set-current-version` 后自动解卡跑完；CLI `set-current-version` 有交互确认（需管道喂 `y`），且 `describe` 用 `--name`、`set-current-version` 用 `--deployment-name`，flag 不统一。

## ④ 对原 PoC 结论的修正

1. **（对⑤-3 的直接回答，且修正"文档直觉"）`patched()` 能救踩坑③，但机制与"旧 run 走旧分支"的普遍说法不同**。temporalio 1.34 实测：patched() 按**代码点是否处于重放**判定——旧 run 走到"历史还没写下任何决策"的点时 `is_replaying()=False`，patched() 返回 **True** 并把 PATCH marker 写进这个旧 run 的历史（patch-run-old：旧 run 最终以新逻辑完成）；旧 run 走到"历史已记录旧分支命令"的点时返回 **False**，沿旧分支对账续跑（patch-run-old2）。两种情况都不会像裸热修那样 WorkflowTaskFailed 反复中毒——"换代码必须 terminate"确实被 patched() 解掉，但代价是：**未定决策的在飞 run 会被直接切进新逻辑**（生产上若新旧分支语义有副作用差异，这本身就是一次热变更，需按此设计）。
2. **（新增）worker 崩溃 failover 有两个不同的时间常数**：activity 在飞被杀 → heartbeat_timeout（A2 实测 2.38s，前提是 activity 有心跳；无心跳则整段 StartToClose 白等，A4 实测 25s+）；纯 workflow 等待（timer/signal）中被杀 → 新 workflow task 先落死 worker 的粘性队列，吃满一个 WorkflowTaskTimeout（默认 **10s**，`WORKFLOW_TASK_TIMED_OUT SCHEDULE_TO_START`）才 failover（A3、run-old2 两处实测）。原 PoC⑤-2 的"未测"现在有数了：生产容量规划不能按"秒级恢复"估 workflow 等待中的崩溃。
3. **（对④"版本化是受版本管的资产"的细化）activity 不是版本化对象**：run-old2 里旧分支 activity 在 v2 worker 上以 attempt=2 照跑不误——版本纪律只约束 workflow 代码；activity 保持参数向后兼容即可。
4. **（对⑤-3 加分项的更正）原 PoC 设想的 `temporal operator task-queue` build id 集合 API 已不存在**（CLI 1.9.1 无该子命令）；现行形态是 Worker Deployments v2（`deployment_config` + `temporal worker deployment`），生产版本策略应按 v2 写，v1 build-id 资料可直接作废。

## ⑤ 新发现的遗留问题

1. **重放对同构 activity 不校验 args（temporalio 1.34.0 + Server 1.32.0 实测）**：patch-run-mix 里 v3 代码请求 `record_path("v2-fastpath",1.0)` 却消费了历史中 `("v1-path",4.0)` 的结果，无任何报错。若新旧分支对同一 activity 槽位传不同参数、且参数影响外部副作用语义，会静默错账。边界：仅验证了"槽位形状相同"；形状不同时原 PoC 已证会响亮失败（踩坑③）。建议后续用 activity 类型不同的变体补一个边界。
2. **粘性队列 10s 惩罚未被任何文档参数直接控制**：worker 死亡时若有 workflow 等待中，failover 延迟 ≈ 一个 WorkflowTaskTimeout。能否调小（worker `sticky_queue_schedule_to_start_timeout`?）及其与生产的取舍未测。
3. **A4 暴露的 SLC 设计纪律**：SLC 必须 > 活动实际耗时 + 调度余量，否则重试策略退化为确定性失败发生器（本验证 kill-a4 即以 FAILED 收场，真实证据）。原 PoC 的 ACT_TIMEOUT=30s 对真实外部调用偏紧，底座设计应强制"SLC ≥ p99 耗时 × 安全系数"。
4. **Worker Deployments 的运行前提未完全摸清**：无 current version 期间新 workflow 无法路由（wf-wv-a 卡住，set-current-version 后自愈）；首版本不会自动成为 current（需显式 set）。部署流水线必须保证"先 set-current 再放流量"，该顺序约束值得在底座文档里固化。
5. **心跳是 activity 崩溃检测的唯一快速通道**：A 组四个 run 证明无心跳 activity 在 worker 死亡后 server 全盲（连提前加入的新 worker 都拿不回任务）。底座的 activity 编写规范应默认要求周期 heartbeat（尤其长 activity），否则 SLA 无法承诺。

— 产物：`verify/`（kill_wf.py、kill_worker.py、drive_kill.py、patch_v1/v2/v3.py、patch_worker.py、drive_patch.py、wv_worker.py、drive_wv.py）；日志与事件历史：`verify-logs/`（activity_trace.log、patch_trace.log、wv_trace.log、*-history.json、kill-a2c-describe-deadwindow.txt、kill-a4-describe-t8/t33.txt、wv-deployment-describe.txt、各 worker log/pid、server.log/pid）。

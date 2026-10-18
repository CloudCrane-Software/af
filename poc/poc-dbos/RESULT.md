# PoC-dbos：DBOS Transact 实测（对标 poc-temporal 同口径）

- id: `poc-dbos`，目录：`af/poc/poc-dbos/`
- 环境：Python 3.14.4 + SDK `dbos` **3.3.0**（注意：`dbos-transact` 这个包名已在 PyPI 消失，实测 404，现名 `dbos`）+ PostgreSQL **18.6**（apt 安装，unix socket /tmp，端口 5544，数据目录 `/tmp/dbos-pgdata`）
- 对标：`poc-temporal/RESULT.md` ④节的裁决——"需要长流程编排+人工审批+外部系统幂等+审计，Temporal 语义收益 > 纪律成本；仅是'函数级 crash 恢复'则 DBOS 更省心。且 DBOS 本身未实测。"本次把 DBOS 补实测。

## ① 结论：**通过，且 ④节裁决维持（并加一个重要修正）**

全部验收点在本机真实执行通过（每条都有 SQL/日志/副作用文件三方证据）：

| 验收点（对标 Temporal PoC） | 结果 | 对标 |
|---|---|---|
| 安装可用（Python 3.14 + Postgres 18.6） | ✅（两次环境降级，见②-1） | dev server |
| kill -9 进程 → 重启同程序 → **已完成 step 不重跑、未跑 step 补跑**、workflow SUCCESS | ✅（step1/2/3 各 EXEC 恰 1 次） | kill -9 server 续跑 |
| step 中途被杀 → **该 step 整体重跑**，副作用追加 4 次（line=1/3 出现两遍） | ✅（at-least-once） | activity 执行 2 次扣款 1 次 |
| 审计面：13 张系统表 + `dbos workflow list/get/steps` CLI，普通 SQL 可查 | ✅ | 事件历史 |
| 人工审批：workflow 停在 `DBOS.recv`，**独立进程** `DBOSClient.send` 审批后继续 | ✅ | signal/update |
| durable timer：`DBOS.sleep(3)` 中途杀 → 重启只睡剩余 ~2s | ✅ | durable timer |
| 在飞 workflow 热改代码 | ✅（见下，DBOS 有**自己的一套纪律**） | replay 中毒 |
| 管理工具：`workflow fork`（从任意 step 复制记录重放尾部） | ✅ | - |

**热改代码（与 Temporal replay 中毒的直接对比，本轮最有信息量的发现）**——DBOS 不是"没有纪律"，而是**纪律形态不同**：

1. **只改 step 实现源码**（workflow 函数源码不动）：正常恢复，已执行 step 用**记录的旧输出**（不重跑），未执行 step 跑新实现。无需任何 versioning 仪式。`app_version` 只对 **workflow 函数源码**做 MD5（源码级确认 `compute_app_version()`），step 改动不触发版本变化。
2. **改 workflow 函数源码**（在两 step 间插入新 step）：`application_version` 哈希改变 → 新进程启动日志 `No workflows to recover from application version <新哈希>`，**在飞 run 卡在 PENDING，不被恢复也不被污染**（不会像 Temporal 那样拿新代码反复回放到中毒）。
3. 显式钉回旧版本号重启 → 恢复执行新代码 → 在 step 2 处**显式报错**：`DBOS Error 11: function step_a was recorded when step_new_hot was expected. Check that your workflow is deterministic.`，workflow → ERROR，无静默错配（重放按 `(workflow_uuid, function_id)` 匹配并校验 `function_name`，源码级确认 `DBOSUnexpectedStepError`；step 的**输入不参与校验也不入库存**）。

**裁决修正**：原结论"DBOS 更省心、没有 replay 纪律"只对了一半。省心的是运维形态（无独立 server、恢复全自动、审计就是 SQL）和 step 级代码迭代；但 **workflow 源码同样是受版本管的资产**——DBOS 用"源码哈希=应用版本+恢复按版本钉住"把纪律从"回放期"前移到"部署期"：越界改动不会毒死在飞 run（更安全），但默认会把它**晾在 PENDING**，需要你显式 fork（`fork -v <version>`）或钉版本接管。对"长流程+审批+幂等+审计"的订单场景，Temporal 的收益判断不变：事件级审计、update 带返回值、CAN 订单身份续接、多语言 worker 这些 DBOS 侧没有对等物。**→ 裁决维持。**

## ② 步骤

1. **安装（两次降级路径，均实测）**：
   - `pip install dbos-transact` → **404**（清华镜像与 pypi.org 均无此包，已改名）。装 `dbos` 3.3.0 成功（Python 3.14：greenlet/psycopg-binary 均有 cp314 wheel）。
   - `pgserver` → **无 cp313/cp314 wheel**（PyPI 最新 0.1.4 只到 cp312），降级 `sudo apt-get install postgresql-18` 成功；5432/5433 被占，起在 **5544**；`initdb -D /tmp/dbos-pgdata`。
2. **基线**：`app.py serve wf_3step:baseline`（3 个 `@DBOS.step()` 向副作用文件追加 EXEC 记录）跑通，系统库自动建 13 张表。
3. **实验 1（step 间崩溃）**：workflow 在 step2 与 step3 之间停等 gate 文件（纯 Python 轮询，不占 function_id）→ 确认 `GATE-ENTER` 后 `kill -9` → 查库（PENDING、仅 2 条 operation_outputs）→ 开 gate、重启同一程序（`DBOS.launch()` 自动恢复）→ 查最终状态与 EXEC 计数。脚本 `exp1_crash_between_steps.sh`。
4. **实验 2（step 中途被杀）**：`step_charge` 每隔 0.6s 追加 1/3、2/3、3/3；追加完 1/3 即 `kill -9` → 重启 → 数副作用行。脚本 `exp2_kill_midstep.sh`。
5. **实验 3+4（审计+审批）**：同库造 ERROR（`wf_fail`）与 PENDING（`wf_approve` 停在 `DBOS.recv("approval")`）→ 跑 `audit.sh` 全套 SQL + `dbos workflow list/get/steps` → 另一进程 `approve.py`（`DBOSClient`，只连系统库、不加载任何 workflow 代码）发审批 → 验证完成与 notifications 表。脚本 `exp3_exp4_audit_approve.sh`。
6. **补充（timer+fork）**：`wf_timer` 停在 gate → 开 gate 后 1s（sleep 进行 1s/3s）杀 → 重启测醒来耗时；对已完成的 workflow `dbos workflow fork -S 3`。脚本 `exp5_timer_fork.sh`。
7. **实验 5（热改，三轮）**：脚本在两阶段之间用 `sed` **物理修改 app.py** 再重启（保证 kill 前后是真代码差异）：
   - 第一轮 `exp6_hotcode.sh`（运行时开关式改法，意外证明：非源码改动不触发版本变化，恢复即撞名字不匹配）；
   - 第二轮 `exp6b_hotcode_stepimpl.sh`（sed 改 step 实现：`else ""` → `else "-IMPL2"`）；
   - 第三轮 `exp6c_hotcode_wfsource.sh`（sed 改 workflow 源码：`if v == "2":` → `if v != "2":` 插入新 step；先自动版本后钉旧版本 `POC_APP_VERSION=<旧哈希>`）。

## ③ 证据（关键输出摘录；全量在 `logs/`）

**1) 实验 1：step 间 kill -9（`logs/exp1_run.log`）**
```
杀前 fx.log:  EXEC crash1 step1 pid=3858055 / EXEC crash1 step2 pid=3858055   (停在 gate)
杀后查库:     wf_3step-crash1 | PENDING | recovery_attempts=1
              operation_outputs: function_id 1,2 (step_a) ← 仅 2 条
重启日志:     Recovering 1 workflows from application version 635cb526...
杀后 fx.log:  EXEC crash1 step3 pid=3858181  ← 新进程补跑，pid 已变
终态:         SUCCESS, recovery_attempts=2;  EXEC 计数 step1=1 step2=1 step3=1
client 解码:  status=SUCCESS result='crash1-done'
```

**2) 实验 2：step 中途被杀，at-least-once（`logs/exp2_run.log`）**
```
杀时 fx.log:  CHARGE midkill line=1/3 pid=3859148   ← 1/3 后即杀
杀后查库:     operation_outputs 仅 function_id=1     ← 未完成的 step 不入账
重启后 fx.log: CHARGE midkill line=1/3 pid=3859230  ← 整个 step 重跑
               CHARGE midkill line=2/3 / line=3/3 / EXEC midkill step3
CHARGE 总行数: 4（line=1/3 出现 2 次 = 副作用重复）
终态:         SUCCESS result='midkill-done charge=charged'
```
与 Temporal 同构：引擎给 at-least-once，"扣款恰好一次"同样要靠业务幂等键在 step 内去重（step 是重跑原子单元，与 activity 相同）。

**3) 实验 3：审计面（`logs/exp3_exp4_run.log`）**
```
dbos schema 13 张表: workflow_status / workflow_input / workflow_output / operation_outputs /
  notifications / workflow_events / workflow_events_history / streams / queues /
  workflow_schedules / application_versions / event_dispatch_kv / dbos_migrations
workflow_status 一行一 run: status,name,executor_id,recovery_attempts,application_version,
  created_at,updated_at,inputs,output,error,queue_name,parent_workflow_id,forked_from,...
operation_outputs 一行一 step: function_id,function_name,output,error,
  started_at_epoch_ms,completed_at_epoch_ms
ERROR 样例: wf_fail-boom1 | ERROR | error=pickle("intentional failure in boom1")（client 可解码）
CLI: dbos workflow get wf_approve-order7 → JSON 含 input/args、output="order7-approved-by=bob"
     dbos workflow steps → [{id:1,step_a,output:"received-ok"},{id:2,DBOS.recv,output:"approved:bob"},
                            {id:3,DBOS.sleep,...},{id:4,step_a,output:"settled-by=bob-ok"}]
```
与 Temporal 对比：粒度是"**每步一行**"而非事件流——能看到每步的输入缺省（step 入参不入库，只有 workflow 级 inputs）、输出、起止时间、报错；可见性即 Postgres SELECT 权限，做运维报表/对账比 Temporal 的事件历史直接得多；代价是默认 **pickle 序列化**（`gAWV...` base64），裸 SQL 看不懂内容（`DBOSPortableJSONSerializer` 可换）。

**4) 实验 4：审批门（`logs/exp3_exp4_run.log`）**
```
fx.log: APPROVAL-WAIT order7 pid=3861008            ← workflow 停在 recv, 库里 status=PENDING
另一进程: [approve] sent to wf_approve-order7 topic=approval msg='approved:bob'
fx.log: APPROVAL-RECV order7 msg='approved:bob' → EXEC order7 settled-by=bob
notifications 表: destination=wf_approve-order7 topic=approval consumed=t   ← 消息持久化且有消费标记
终态: wf_approve-order7 | SUCCESS
```
Temporal signal/update 的等价物是 `recv/send`（+`set_event/get_event` 做 workflow→外的事件）。差异：update 有同步返回值、signal 支持多事件源；recv 是"一 workflow 一 topic 一条消费"，审批人 UI 要回执得再查 workflow 状态。

**5) 实验 5：热改代码（`logs/exp5b_run.log`、`logs/exp5c_run.log`）**
```
[step 实现热改，良性] sed 改 step_a 返回 "-IMPL2"：
  H1==H2（app_version 只哈希 workflow 源码，step 改动不触发）
  Recovering 1 workflows → s1/s2 不重跑（无新 EXEC），s3 EXEC pid=新
  result = hotB1-hot-done s1=hot-s1-ok s2=hot-s2-ok s3=hot-s3-ok-IMPL2
                                        ↑已跑步骤: 记录的旧输出生效  ↑未跑步骤: 新实现生效
  终态 SUCCESS
[workflow 源码热改，插入新 step] sed 改 wf_hot 源码：
  H2=8d34618d... ≠ H1 → 新进程日志: No workflows to recover from application version 8d34618d...
  在飞 run: 仍 PENDING, fx.log 无任何新 EXEC  ← 默认不恢复、不污染，但卡住
  POC_APP_VERSION=<H1> 强制接管 → Recovering 1 workflows →
  workflow get → status: ERROR
    error: DBOS Error 11: During execution of workflow wf_hot-hotB2 step 2,
           function step_a was recorded when step_new_hot was expected.
           Check that your workflow is deterministic.
  steps 表仍为 id1=step_a('hot-s1-ok'), id2=step_a('hot-s2-ok')  ← 无静默错配
```

**6) durable timer + fork（`logs/exp_timer_run.log`）**
```
sleep(3) 进行 1s 时 kill -9；库中已有 function_id=2 "DBOS.sleep" 行（定时即入账）
重启→after-sleep EXEC 距重启 2.87s（≈剩余 2s + 进程启动），非全 3s → SUCCESS result='t1-timer-done'
fork wf_timer-t1 -S 3 → 新 workflow(01a1514b-...) forked_from=wf_timer-t1,
  记录的 step1/sleep 输出被复制（不重跑），尾部 step 重新 EXEC（副作用按设计重复），SUCCESS
```

## ④ 对最终方案的反馈（一手开发体验 vs Temporal）

- **心智负担 DBOS 明显更小**：装饰器一对（`@DBOS.workflow`/`@DBOS.step`）、函数就是普通函数（gate 轮询、time.sleep 随便写，只有 DBOS 调用被记录）、无独立 server（应用进程 + Postgres 即全部，`launch()` 自动建表+恢复）。从零到第一个 crash-recovery 通过约 30 分钟。Temporal 的 worker/starter/task-loop/事件回放心智高不少。
- **审计是 SQL 直查**：13 张表 + CLI（list/get/steps/cancel/resume/**fork**）。对账、排障、给 DBA/风控开只读账号都直接；Temporal 的事件历史表达力更强但生态封闭在它的 CLI/API 里。**注意默认 pickle**，要人读就配 `DBOSPortableJSONSerializer`。
- **审批门可用但比 signal/update 粗**：`recv` 消费型、无回执语义（审批人要结果得轮询 workflow 状态），`set_event/get_event` 更像 query。做"审批通过拿回执"的同步 UI，Temporal update 一步到位，DBOS 要自己组装。
- **热改代码的纪律前移了**：Temporal 在回放期炸（在飞 run 中毒重试）；DBOS 在部署期拦（源码哈希变 → 恢复按版本隔离，在飞 run 卡 PENDING 等同版本执行者，或显式 fork/钉版本，越界恢复则 `DBOSUnexpectedStepError` 指名道姓）。**两边都要求"workflow 源码是受版本管的资产"，谁也别想裸奔**；但 DBOS 的失败模式（stuck+显式报错）比 Temporal（中毒循环）好诊断，且 step 级迭代完全自由。要留意 `application_version` 是隐式 MD5——CI 里"只是改了 workflow 函数注释"也会让在飞 run 失去恢复者，建议生产显式配置 `application_version` 并配合 fork 流程。
- **工程细节坑**：`dbos-transact`→`dbos` 改名导致旧文档/教程全部失效（本次第一行 pip 就撞上）；`pgserver` wheel 落后于 Python 版本；关停 API 是 `dbos.destroy()` 不是 stop；`DBOSClient` 与 DBOS 类 API 名不对称（`retrieve_workflow()` 而非 `get_workflow_status`）。恢复把 workflow 置 `ENQUEUED` 再跑（轮询健康检查要把它算作未完成，本次踩过）。
- **定位建议**：如果场景收敛为"进程内长任务、函数级恢复、审计给内部人员"——DBOS 的运维成本确实远低于 Temporal，裁决中"DBOS 更省心"被实证；只要出现**跨语言 worker、事件级审计、审批回执、订单身份跨 run 续接（CAN）**任一需求，Temporal 的语义收益依然压过纪律成本——与 Temporal 侧裁决一致，**整体裁决维持**。

## ⑤ 遗留问题

1. **recv 中被杀未测**：审批等待中的崩溃恢复（recv 记录形态与 sleep 类似，预计安全，但未实证）。
2. `enable_patching`（`PATCHING_ENABLED`）与 `fork -v` 的多版本并存运维未测；`MAX_RECOVERY_ATTEMPTS_EXCEEDED` 行为未触发。
3. 队列（`Queue`）、child workflow、定时调度、Conductor 控制台（本次日志里出现 console.dbos.dev 链接，未使用）未覆盖。
4. 多执行者同版本竞争恢复、`use_listen_notify` 性能形态未测；系统库默认 **SQLite**（`system_database_url` 缺省即 sqlite）未测——生产若想省 Postgres 值得一验。
5. Python 3.14 兼容性仅验证了本 PoC 用到的同步 API 面；异步（`*_async`）路径未测。
6. 环境残留说明：apt 安装的 `postgresql-18` 系统包保留；PoC 专用数据目录 `/tmp/dbos-pgdata`（重启后 `pg_ctl -D /tmp/dbos-pgdata -o "-p 5544 -k /tmp" start` 可复现）；**Postgres 已停、5544 已释放**，无其它常驻进程。仓库内 `.gitignore` 排除 `artifacts/ logs/ .venv/ __pycache__/`。

— 产物：`app.py`、`common.py`、`approve.py`、`audit.sh`、`exp1_crash_between_steps.sh`、`exp2_kill_midstep.sh`、`exp3_exp4_audit_approve.sh`、`exp5_timer_fork.sh`、`exp6_hotcode.sh`、`exp6b_hotcode_stepimpl.sh`、`exp6c_hotcode_wfsource.sh`、`logs/`、`artifacts/`（副作用与 gate 记录）。

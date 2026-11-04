# F2 全量门「收件箱派单 + 独立 runner」执行面契约（v0.3-T7① F2 批新增节）

> 归属：v0.3 T7 终裁①（CI 执行器=Gitea Actions 先行，但**Actions 只跑 F1 快速门**；
> **F2 全量走收件箱派单 + 独立 runner，不许把全量门塞进 Actions**）。
> 本节是 **F2 执行面的契约面**：派单契约（§一）、runner 执行面（§二）、宿主供给契约
> 与 SKIP 口径（§三）、结论入账与 integrator 复核衔接（§四）、边界（§五）。
>
> **本文不改 `docs/CI-BOUNDARY.md` §一—§六 任何既有语义**，只在其分工表
> （`docs/CI-BOUNDARY.md` §三）所指"F2 走收件箱派单 + 独立 runner"处补上**契约正文**；
> 外部依赖与跳过档口径**仍以 `docs/CI-BOUNDARY.md` §一 为准**（本文只引用，不改写）。
>
> 机读落点：`ci/f2_dispatch.py`（派单侧）、`ci/f2_runner.py`（runner 侧）。
> 两者都是**一次性 CLI**（无常驻守护、不起监听、不用 sudo、不进 Actions）。

---

## 一、派单契约（`af.f2.task.v1`）

### 1.1 派单消息的 schema（复用 `af.trigger.v1`，不重造信封）

F2 任务**不是**新造的 webhook 协议：派单输入就是 `af.trigger.v1` 的**已验信封**
（七字段 `event_id`/`thread_key`/`payload_ref`/`payload_sha256`/`dedup_key`/`source`/
`received_at` + 外层 `delivery_id`/`event_type`/`body`，见
`poc/poc-super-agent/contracts/af.trigger.v1.md` §1）。派单侧做的是**翻译**：

| 层 | 件 | 谁写 | 内容 |
|---|---|---|---|
| 收件层 | `af-state/triggers/inbox/<source>/<delivery-id>.json` | `tools/trigger-inbox.py`（**在役面，只读**） | af.trigger.v1 已验信封（含 `body_b64` 字节精确原始 payload） |
| 派单层 | `af-state/triggers/dispatch/<order_id>.json` | `tools/event-router.py` 消费腿（**在役面，只读**） | `order_id`/`suite`/`repo`/`subject`/`dedup_key`/`delivery_id`/`recipe_digest`… |
| **F2 任务层** | `<队列>/tasks/<task_id>.json` | **`ci/f2_dispatch.py`**（本节新增件） | `af.f2.task.v1`（下表） |

`af.f2.task.v1` 字段：

| 字段 | 类型 | 语义 | 来源 |
|---|---|---|---|
| `schema_version` | str | 恒为 `af.f2.task.v1` | 工具 |
| `task_id` / `order_id` | str | 任务标识（**同值**，= 在役派单记录的单号） | 确定性生成 |
| `idempotency_key` | str(64) | **幂等键**（§1.3） | 工具自算 |
| `suite` / `gate_id` / `gate_command` | str | 恒 `F2` / `f2-full` / `bash ci/run.sh` | 工具 |
| `repo` / `remote` | str | 仓库全名 / 远端名（`origin`） | 信封重解析 + 工具 |
| `base_ref` | str | 合流目标 ref，恒 `origin/product/main` | PR base 分支（`docs/CI-BOUNDARY.md` §五：唯一合流目标是 `product/main`） |
| `base_sha` | str(40) | PR 事件时刻的 base 提交 | PR payload `pull_request.base.sha` |
| `source_sha` | str(40) | **被测提交** | PR payload `pull_request.head.sha` |
| `source_branch` | str | 候选分支名 | PR payload `pull_request.head.ref` |
| `subject` / `thread_key` / `dedup_key` | str | 与在役派单记录**逐字同值** | 工具重算并与在役记录对账 |
| `delivery_id` / `event_type` / `action` / `source` | str | 投递标识与事件类型 | 信封 |
| `recipe_digest` | str(64) | 注册表配方指纹（order_id 生成输入之一） | 注册表 |
| `payload_sha256` | str(64) | 原始 payload 字节流摘要 | 信封（复算校验） |
| `host_supply_requirements` | list | 宿主供给需求清单（`cosign`/`cargo-deny`/`advisory-db`/`netns`） | 工具 |
| `status` | str | `queued` / `claimed` / `done` / `failed` | 状态机 |
| `attempts` / `max_attempts` | int | 已领跑次数 / 上限（默认 3） | 状态机 |
| `next_attempt_at` / `backoff_sec` | str/int | 退避到点时刻与退避秒数 | 状态机 |
| `lease` | obj/null | `{runner_id, claimed_at, expires_at, ttl_sec}` | 领单时写 |
| `created_at` / `updated_at` | str(ISO-8601) | 时刻 | 工具 |

### 1.2 投递方式

* **不新增投递通道**：F2 任务的输入只有两个在役只读面 —— 收件箱信封与派单记录
  （`--triggers-home` 指 `af-state/triggers`）。**没有 webhook、没有队列服务、
  没有监听面**（收件仍由在役 `tools/trigger-inbox.py` 独占）。
* **落点**：runner 自有队列目录（`--queue`，缺省 `$AF_F2_QUEUE`，再缺省
  `$HOME/.local/state/af-f2`），与在役收件箱**物理分离**：
  ```
  <队列>/tasks/<task_id>.json      任务件
  <队列>/locks/<task_id>.lock      领单互斥锁（flock）
  <队列>/results/<task_id>.json    结果件（af.f2.result.v1）
  <队列>/reported.jsonl            回报行（结论回执流水）
  <队列>/dead/<task_id>.json       死信（超 max_attempts；留证不删）
  <队列>/logs/<task_id>.attemptN.log  门日志副本
  <队列>/supply/host-supply-<task_id>.json  宿主供给清单
  ```
* **写入面**：派单侧**只写队列目录**，对在役面**只读**（一个字节都不写）。
* **命令**：
  ```bash
  python3 -I ci/f2_dispatch.py ingest --triggers-home <af-state/triggers> --queue <队列>
  python3 -I ci/f2_dispatch.py list --queue <队列>
  python3 -I ci/f2_dispatch.py show --queue <队列> --task-id <id>
  ```

### 1.3 幂等键与去重（三道，正交）

| 道 | 键 | 落点 | 语义 |
|---|---|---|---|
| ① 投递幂等 | `<source>/<delivery_id>` | 在役收件箱（`af.trigger.v1` §3.1） | 同投递重复 ⇒ HTTP 200 duplicate，不重写文件 |
| ② 业务去重 | `dedup_key` = `pull_request:<repo>:<pr#>:<head sha>` | 在役消费腿 | 重放投递不重复派单 |
| ③ **派单幂等键** | `idempotency_key = sha256(order_id|source_sha|base_ref|base_sha|recipe_digest)` | **本节新增** | 同内容必然同键 ⇒ 重投不产生第二个任务（演练实测：`created=0, duplicate=1`） |

`order_id` 沿用**在役消费腿的确定性公式**（`sha256(repo|suite|subject|recipe_digest)[:16]`
⇒ `F2-<repo-slug>-<hash16>`）⇒ 本节算出的单号与在役派单记录**逐字同值**；
不一致 ⇒ **拒建单**（不覆盖、不改写）。
同 `task_id` 但幂等键不同 ⇒ 记 `conflict`，**不覆盖**，留给人工判读。

### 1.4 失败重试与退避

* 触发：runner 领单后**未得绿**（门红 / SUMMARY 非绿 / 槽位不齐 / 宿主缺件 / 入账被拒）。
* 计数：`attempts += 1`；`attempts >= max_attempts`（默认 3）⇒ `status=failed`
  + 写死信 `<队列>/dead/<task_id>.json`（**留证不删**）。
* 退避：`backoff = min(60s × 2^(attempts-1), 3600s)`，再乘 `1 ± 20%` 抖动；
  `next_attempt_at = now + backoff`；退避未到的任务**本轮不领**（不是失败）。

### 1.5 认领唯一写者与租约超时

* **唯一写者**：只有 `ci/f2_runner.py` 能把任务从队列领走（派单侧只建单，不领单）。
* **互斥**：`flock(LOCK_EX|LOCK_NB)` 锁 `<队列>/locks/<task_id>.lock`
  —— 进程死则内核自动释放（崩溃安全）。
* **租约**：领单时写 `lease{runner_id, claimed_at, expires_at, ttl_sec}`（默认 TTL 1800s），
  跑门期间由进程内线程续租（进程退出即停，不构成常驻守护）。
* **重领**：租约**未过期** ⇒ 第二个 runner 领不走（报 `ALREADY-CLAIMED`）；
  租约**已过期** ⇒ 允许重领（演练实测：未过期 ⇒ exit 1 拒领；过期 ⇒ exit 0 重领成功）。
* **状态与结果件不删**：任务件、结果件、门日志、死信一律留档。

---

## 二、runner 执行面（完整可跑序列）

```bash
# 0) 宿主供给核（只读；缺件 ⇒ SKIP，不得计绿）
python3 -I ci/f2_runner.py supply --repo <候选树>            # 缺件 ⇒ exit 1

# 1) 领单 → 跑门 → 落 SUMMARY → 结论入账 → 回报（一次性；非守护）
python3 -I ci/f2_runner.py run \
    --queue <队列> --task-id <id> \
    --repo <候选树> --worktree <门执行目录> \
    --gate-command "bash ci/run.sh" \
    --summary-path <候选树>/ci/out/SUMMARY.txt \
    --record-dir <候选树>/ci/out \
    --lease-ttl 1800 --gate-timeout 14400

# 2) 结论入账交接（只读实测侧复算 + rebase 漂移动作清单）
python3 -I ci/f2_runner.py handover --record <记录件> --repo <仓> \
    --base-ref origin/product/main --branch <候选分支>
```

`run` 的**执行顺序**（脚本级，`ci/f2_runner.py` `cmd_run`）：

1. 读任务件（schema 不符 ⇒ 拒，exit 2）；已完成任务 ⇒ **幂等返回既有结果**（不重跑门）。
2. 退避未到 ⇒ 本轮不领（exit 0，非失败）。
3. **领单**：`flock` + 租约（§1.5）。
4. **候选树绑定核**：`HEAD == task.source_sha` 且 `git status --porcelain` 为空
   ⇒ 否则 `WORKTREE-UNRESOLVED` / `SOURCE-SHA-MISMATCH` / `WORKTREE-DIRTY`，
   **不跑门、不写记录件**。
5. **宿主供给核**（§三）：缺件 ⇒ **不跑门**、`verdict=skip-host-supply-missing`。
6. **跑门**：`bash -lc "<gate-command>"`，cwd = 门执行目录；门日志落
   `<队列>/logs/<task_id>.attemptN.log`；门命令**原文**进结果件与记录件（不许改写）。
7. **读 SUMMARY**并按**复核器同款词表**判绿
   （`^step\S*\s+(\S+)\s+\d+ms\b`；白名单 `PASS`/`PASS-WITH-WARN`/`ADVISORY-PASS`/
   `ADVISORY-WARN`；红名单 = 其它一切，含 `FAIL`/`ADVISORY-FAIL`/`SKIP-*`）。
   **槽位覆盖**：14 个槽（step1–12、step13 共宿、step14）必须齐全 ⇒ 缺槽位 = 红。
8. **结论入账**：仅当「门 exit 0 + SUMMARY 全绿 + 槽位齐全」⇒ 调
   `ci/integrator_bind_check.py emit` 写记录件（§四）；否则**不写**。
9. **回报**：结果件 `<队列>/results/<task_id>.json`（`af.f2.result.v1`）
   + 回报行 `<队列>/reported.jsonl`。
10. **退租约 + 状态机**：绿 ⇒ `done`；红 ⇒ `queued`（带退避）或 `failed`（写死信）。

**退出码**：`0` = 绿/幂等无操作；`1` = 红（门红、缺件、领单失败、复核面拒）；`2` = 用法/环境错。

---

## 三、宿主供给契约与 SKIP 口径

### 3.1 供给契约（`af.f2.host_supply.v1`）

| 件 | 放在哪 | 怎么核 | 缺件怎么办 |
|---|---|---|---|
| `cosign` | `<候选树>/bin/cosign`（**不入库**：`.gitignore` 行 2 `/bin`） | 存在 + 可执行 + sha256 + `cosign version` | **SKIP**（`environment_unavailable`）⇒ 不跑门、不计绿、回队列重试；结果件点名缺件与投放路径 |
| `cargo-deny` | PATH（`~/.cargo/bin/cargo-deny`） | `which` + sha256 + `cargo-deny --version` | 同上 |
| `advisory-db` | `$CARGO_HOME/advisory-dbs/advisory-db-3157b0e258782691` | 目录存在 + 文件数 + （git 仓则）HEAD sha | 同上（刷新走 `ci/refresh-advisory-db.sh`，代理 `socks5h://100.64.0.3:1080`；GitHub 不可达时沿用旧库 WARN 降级） |
| `netns` | 内核能力 | `unshare -Urn true` 退出码 | 无非特权 user namespace ⇒ step6 断网隔离不可用 ⇒ 门必红（`ci/run.sh` 自身语义）；**本件只探测不升权，永不调用 sudo** |

供给清单落 `<队列>/supply/host-supply-<task_id>.json`（含各件 sha256 前 12 位与版本号；
**证据里零凭据正文**，只出指纹）。

### 3.2 SKIP 口径（承 `docs/CI-BOUNDARY.md` §一，禁改其语义）

* step6（cosign）/ step7（deny + advisory-db）的缺件跳过档是**对账口径**，
  不是预检改写；其宿主等价验证就是"F2 独立 runner 补跑"。
* **SKIP 不得计绿**——本节把它落成机器行为，共三道：
  1. **runner 层**：宿主供给缺件 ⇒ **不跑门**、**不写 integrator 记录件**、
     `verdict=skip-host-supply-missing` ⇒ 复核面根本拿不到可核的绿记录件；
  2. **摘要层**：SUMMARY 里出现白名单外的状态（含 `SKIP-*`）⇒ 非绿 ⇒ 不写记录件；
  3. **复核器层**：`ci/integrator_bind_check.py` 的 `emit` 对判不出绿的摘要**拒写记录件**，
     `verify` 对任何非 `pass` 的 verdict **拒绝**。
* ★ 本节**不新增** `ci/run.sh` 的 step 编号，也**不改** `ci/run.sh` 任何断言强度。

---

## 四、结论入账与 integrator 复核衔接（T7②）

### 4.1 四元组从哪来（禁手抄）

四元组 `base_sha` / `source_sha` / `tested_tree` / `suite_digest` **全部由**
`ci/integrator_bind_check.py emit` **机器自算**（`emit` 子命令内部：
`base_sha = merge-base(base_head, source_sha)`；`tested_tree = source_sha^{tree}`；
`suite_digest` = `ci/` + `.gitea/workflows/` 在该树内的逐件摘要）。
runner **只把记录件里那四个值读出来**放进结果件（抄的是工具产物，不是人抄）。
`suite_digest` 会随**新增门脚本**而变 ⇒ 新增 `ci/f2_*.py` 后，正在合流的候选树
必须**重跑门**（`docs/INTEGRATOR-SOP.md` §十.5：这是设计意图，不是缺陷）。

### 4.2 记录件与门摘要的同面规则

* 记录件落 `<候选树>/ci/out/integrator-record-f2-<task_id>.json`，
  门摘要落 `<候选树>/ci/out/SUMMARY.txt` —— **同目录**（SOP §四 4.3：
  复核要回读门摘要文件 ⇒ 该文件必须留存，不许删）。
* 记录件**不入候选树**（入了就改树 ⇒ `tested_tree` 失配 ⇒ 复核必红）。
* 复核器**强制回读**门摘要（磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文 三方比对）⇒
  门摘要被改一个字即 `GATE-SUMMARY-DIGEST` 拒绝。

### 4.3 `tested_tree` 与「合并树」不等价时的处置（可执行动作）

`ci/f2_runner.py handover` **只读**复算实测侧（合并树机算 + 门面摘要复算），产出
**动作清单**（**不执行** rebase/push —— 改 ref 与推送由 integrator 依 SOP 处置）：

| # | 动作 | 命令 | 依据 |
|---|---|---|---|
| 1 | 重落基点 | `git fetch origin <base>` && `git rebase origin/<base>`（真合并形态改为把主线合入候选分支并解冲突、重做候选树定稿） | SOP §5.2 步 2 |
| 2 | **重跑 F1 快速门** | 推送候选分支 ⇒ Actions 自动重跑 F1（`.gitea/workflows/f1.yml`，仅 base=`product/main` 触发） | SOP §三 步 2 + §5.2 步 3；**F1 绿是必要非充分条件，F1 绿不替代 F2 记录件** |
| 3 | **重跑 F2 全量门** | `bash ci/run.sh`（在重落基点之后的候选树工作树上） | SOP §5.2 步 3（不许拿"四项里前三项对上了"搪塞） |
| 4 | 重写记录件 | `emit --record <旧名>-r2.json …`（**旧件不许删**） | SOP §四 4.3 + §七 7.3 |
| 5 | 再复核 | `verify --record <旧名>-r2.json …` | SOP §五 步 5 |

**内容等价但基点前移**（落地树 == 门跑过的树且门面逐字节同）⇒ 门结论仍描述这棵树，
可继续，但**"基点变了"必须写进合流报告/台账**（SOP §5.2 步 6）。
**内容不等价** ⇒ 走上面 5 步清单。

---

## 五、边界（如实登记）

1. **F2 全量门不进 Actions**：本节交付物里**没有**任何"在 `.gitea/workflows/*.yml`
   里跑全量门"的设计；`.gitea/` 下**零改动**（门四核验：`git diff --name-only
   origin/product/main -- .gitea/` 须为空）。
2. **本节不改既有件**：不动 `ci/run.sh`（不新增 step 编号）、不动
   `ci/integrator_bind_check.py`、不动 `crates/`、不动 `Cargo.toml`/`Cargo.lock`、
   不动在役收件箱/门户/观测面（`tools/trigger-inbox.py`、`tools/af-jobs-portal.py`、
   `tools/event-router.py` 等**只读**）。
3. **不常驻**：派单侧与 runner 侧都是一次性 CLI；无常驻守护、不起监听、不用 sudo、
   不改 `product/main` 指针。
4. **门跑不跑由宿主供给决定**：F2 全量门必须在一台**供给齐备**的独立 runner 宿主上跑
   （`bin/cosign` + `cargo-deny` + advisory-db + netns）；缺件时门跑不齐
   （`ci/run.sh` step0 预检 fail-closed），此时 SKIP 不得计绿 ⇒ 结论只能是红的。
5. **本节不引入新协议**：派单输入仍是 `af.trigger.v1`；结论面仍是 T7② 的
   `af.integrator_bind_record.v1`；四元组仍由 T7② 的工具自算。

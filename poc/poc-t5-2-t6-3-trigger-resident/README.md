# poc-t5-2-t6-3-trigger-resident — v0.3-T5②/T6③ trigger-inbox 常驻运营面上在役（收卷证据）

> 施工批：v0.3-T5②/T6③（三项补派批内一条链，`dwfrun-d2bb3591`）。日期 2026-11-04。
> 基线 = `origin/product/main`（56ff52a）。分支 = `feat/t5-2-t6-3-trigger-resident`
> （工作树 `/home/pan-ding/workspace/af-wt-t5t6`，`git worktree add -b … origin/product/main`）。
> **未 commit、未 push**（diff 与读数留给主代理）。
> 原始读数在本目录 `raw/`（逐条可核）。

---

## ① 生产落点裁定（两处 `tools/` 的歧义读数 + 裁定 + 理由 + 对安灯探针的影响）

### 1.1 歧义读数（逐条带出处）

| # | 读数 | 出处 | 指向 |
|---|---|---|---|
| a | 「三个落点：`tools/trigger-inbox.py`（收件/验签/落盘）、`tools/event-router.py` poll_triggers 消费腿（读侧）、**本目录**（契约与注册表正本）」 | `af/poc/poc-super-agent/contracts/af.trigger.v1.md:5-6`（契约在 af 仓内，其「本目录」= `poc/poc-super-agent/contracts/`，注册表正本同仓） | **af 仓内** `af/tools/`（该目录已存在，内有 `gate-item-emit.py` 等 4 件） |
| b | 「本体 `tools/trigger-inbox.py` 的 bind 可配与拉起由主代理串行执行」＋ `WS = "/home/pan-ding/workspace/agent黑灯工厂"`（:232） | `tools/factory-keepalive.py:101-112`（工作区级文件） | **工作区级** `tools/` |
| c | `WS = "/home/pan-ding/workspace/agent黑灯工厂"`；`INBOX_TOOL = os.path.join(WS, "tools", "trigger-inbox.py")` | `af/poc/poc-super-agent/tests/selfcheck.py:49,54` | **工作区级** `tools/`（af 仓的 PoC 自检面**跨仓**指向工作区那份） |
| d | `trigger-inbox 起（100.64.0.8:8765，home=/home/pan-ding/workspace/agent黑灯工厂/af-state/triggers，sources=['gitea-webhook']…）` | `状态观测/trigger-inbox.log:1`（最后在役实例的自述行） | 最后在役跑的是**工作区级**那份，且**绑的是 tailnet 私网 IP 100.64.0.8（非回环）** |
| e | 门户转发腿的转发目标写死 `http://100.64.0.8:8765/inbox/gitea-webhook`（env `AF_TRIGGER_INBOX_URL` 可覆盖） | `tools/af-jobs-portal.py:148-149` | gov → 本机 **8765 走 tailnet 私网 IP**（非回环） |
| f | 现役件内容差异：工作区级那份比 PoC 原件多 3 处 `AF_TRIGGER_BIND` 改动（bind 行可配 + 两处日志/文案） | `diff poc/poc-super-agent/tools-mirror/trigger-inbox.py tools/trigger-inbox.py`（改前） | 工作区级那份是**生产接线批（2026-11-03）加过 bind 豁免**的版本 |

**★ 任务书前提与盘面不符的两处（如实登记，不按前提施工）**：

1. 任务书称「工作区级 `tools/` **目前没有** `trigger-inbox.py`」——**与盘面不符**：该文件
   2026-11-03 22:48 已在盘（14545 B，sha256 `58f0afee…`，本批已留 `.bak`）。
2. 任务书称「event-router **尚不在役**」——**与盘面不符**：`tools/event-router.py` 在役
   （pid 2759717，etime 1-03:27:57），其 `_trigger_liveness_andon()`（`tools/event-router.py:2662`）
   每轮重写 `状态观测/trigger-inbox-liveness.json`（读数：ts 2026-11-04 23:18:40，
   `alarm_code=trigger-inbox-silent`，`newest_delivery_age_s=88097`）。
   ⇒ 「源断供安灯未接线」这一前提**不成立**：该安灯在路由侧**早已在跑**（§4 细说差在哪）。

### 1.2 裁定

| 角色 | 落点 | 状态 |
|---|---|---|
| **版本受控正本** | **af 仓 `tools/trigger-inbox.py`**（相对工作区根 = `af/tools/trigger-inbox.py`） | **新增**（本批落地；sha256 `8a1bdb69fb581eabdac80942ca1ad7c5a9cc428c696a7912be72f9a50bf1b912`） |
| **在役运行位** | **工作区级 `tools/trigger-inbox.py`** | 本批**同步**为逐字同源（同一 sha256 `8a1bdb69…`）；改前件留 `.bak`（`tools/trigger-inbox.py.20261104-pre-触发常驻批-暴露面收窄.bak`，sha256 `58f0afee…`） |

**理由**：

1. 契约的「三个落点」是**仓内相对路径读法**（契约本身在 af 仓内、注册表正本同仓）⇒ af 仓
   `tools/` 是契约指定的收件端落点；工作区级 `tools/` 从未出现在契约里。
2. **唯一可评审/可回滚/可追溯的落点必须在 git 内**：工作区根**不是** git 仓
   （`git rev-parse --show-toplevel` → `fatal: 不是 Git 仓库`），落那儿就没有分支/diff/回滚面。
3. **在役运行位不能挪**：PoC 自检面 `INBOX_TOOL` 指向工作区那份（c），keepalive 文件头与
   生产日志也指向它（b/d）⇒ 挪路径会静默改掉既有自检指向与现役入口。
4. **同步纪律（消歧义）**：两处**必须逐字同源**（本批已做 sha256 对账，见 `raw/`），
   af 仓为上游：改 af 仓 ⇒ 同步到工作区；改工作区 ⇒ 回收进 af 仓。生产副本文件头已写明
   这条纪律与源件 sha256。

### 1.3 对安灯探针的影响：**无**（路径不参与判活）

keepalive 的 trigger-inbox 三件判据全部与文件路径**无关**：

- ① 进程在：`trigger_inbox_pids()` = `scan_pids(lambda argv: any(os.path.basename(a) ==
  "trigger-inbox.py" for a in argv[1:]))`（`tools/factory-keepalive.py:1617-1619`）⇒ **按 argv
  basename 匹配**；
- ② `AF_TRIGGER_HOME`：`_proc_environ_get(pids[0], "AF_TRIGGER_HOME")`
  （`:1622-1631`，读 `/proc/<pid>/environ`）⇒ 读**在役进程 environ**，与文件路径无关；
- ③ 段B secret：`_proc_environ_get(pids[0], "AF_TRIGGER_SECRET_GITEA_WEBHOOK")`（`:1794`）同上。

**实证**：形态① 自证时，隔离实例跑的是 **af 仓 worktree 路径**
`/home/pan-ding/workspace/af-wt-t5t6/tools/trigger-inbox.py`（与工作区那份完全不同路径），
keepalive 的真实判定函数照样命中：`alive=True detail=进程=[1481843] home=/tmp/t5t6-verify/lamp/A/home`
（读数见 `raw/lamp-readings.json`）。⇒ **生产副本落在 af 仓 `tools/` 不破安灯**。

---

## ② 常驻契约（启动序列 bind→secret / 停机 / 回滚 / 只增不删 / 暴露面读数）

### 2.1 拉起闸（既有裁决，本批只承接）

- **拉起闸 = 主代理**；守护（keepalive）**只报警不拉起**（既有 `fix_trigger_inbox` 逐字未动）；
- **恢复 = 主代理按裁决 `bind` / `secret` 串行拉起**；
- 本批**未把生产收件箱留在运行态**：全部实例均为隔离实例（临时端口 + 一次性测试 secret +
  隔离 `AF_TRIGGER_HOME`），测完即停（停机读数见 §2.5）。

### 2.2 启动命令序列（两条；★ bind 取值**已由主代理裁决＝序列 B**，裁决与依据见 §2.2-A）

**序列 A —— 只绑回环（本批纪律的字面合规形态）**

```bash
cd /home/pan-ding/workspace/agent黑灯工厂/af            # af 仓版本受控正本
# ① bind：先落定暴露面（缺省即回环；显式写出便于审计与回滚）
export AF_TRIGGER_BIND=127.0.0.1
export AF_TRIGGER_INBOX_PORT=8765
export AF_TRIGGER_HOME=/home/pan-ding/workspace/agent黑灯工厂/af-state/triggers
# ② secret：段B 只入 env（不回显、不进 argv、不落明文）
set -a; . ~/.config/af-trigger/gitea-webhook.env; set +a   # AF_TRIGGER_SECRET_GITEA_WEBHOOK
# ③ 拉起（nohup + 独立日志；不留运行态由本批承担——拉起是主代理的动作）
nohup python3 tools/trigger-inbox.py \
  >> /home/pan-ding/workspace/agent黑灯工厂/状态观测/trigger-inbox.log 2>&1 < /dev/null &
# ④ 验活（回环直连；缺 secret ⇒ /healthz 503 not_ready 不假绿）
curl -s http://127.0.0.1:8765/healthz
```

**序列 B —— tailnet 私网绑定（现役 Fast Path 所需；★ 已由主代理显式裁决＝采用）**

```bash
# 与序列 A 相同，只把 ① 换成（**多一个显式确认**，缺确认直接拒起 exit 3）：
export AF_TRIGGER_BIND=100.64.0.8
export AF_TRIGGER_BIND_ACK=100.64.0.8      # 确认值必须与绑定地址同字面
```

**两条序列的取舍（★ **已由主代理拍板＝序列 B**——因「拉起闸 = 主代理」本就是主代理的闸；
四条依据与「序列 A 不采用」的理由见本节末尾 §2.2-A）**：

| | 序列 A（回环） | 序列 B（tailnet 私网） |
|---|---|---|
| 本批纪律 | ✅ 满足「只许绑 127.0.0.1 / 禁非回环监听」字面 | ⚠️ 非回环（但**显式确认 + 可审计 + 拒起即 fail-loud**，杜绝误配即宽暴露） |
| 现役 Fast Path | ❌ **断链**：gov 门户转发目标写死 `100.64.0.8:8765`（`tools/af-jobs-portal.py:148-149`），回环绑定 ⇒ 连接被拒 ⇒ 门户回结构化 502；本批禁反代/隧道，无本地中继可补 | ✅ 保链（2026-11-03 端到端 202 已实证；ACL 只放行 gov→pandev {2222,8765}） |
| 暴露面 | 回环，唯一面最小 | tailnet 私网单口 + headscale ACL 双闸（gov→pandev 仅 8765 一条） |

⇒ **（施工批当时口径）本批不替主代理选**：本批只把「非回环必须显式确认」做成**代码级 fail-loud 闸**
（通配 `0.0.0.0/::` **一律拒起、无豁免**），使序列 B 成为**可审计的显式动作**
（确认值写进在役进程 environ，keepalive 可读），而不是「随手一个 env 就宽暴露」。
**★ 返修批（2026-11-05）注**：本句是**施工批收卷当刻**的口径（原文保留不删）；
**现已由主代理裁决取代**——裁决结果＝**序列 B**（见 §2.2-A），bind 闸的**代码语义不变**
（`resolve_bind()` 与其 fail-loud 行为逐字未动）。

#### 2.2-A ★ bind 取值裁决（返修批落定，2026-11-05；主代理已裁决）

**裁决＝序列 B**：`AF_TRIGGER_BIND=100.64.0.8` ＋ `AF_TRIGGER_BIND_ACK=100.64.0.8`
（端口 `AF_TRIGGER_INBOX_PORT=8765`）。**拉起闸＝主代理**（本就是主代理的闸）⇒ 施工批
只把「非回环须显式确认」做成 fail-loud 代码闸，**不替主代理选**；现由主代理拍板如下。

**依据（四条逐条可核，均为盘面原件）**：

| # | 依据 | 出处（逐字） | 指向 |
|---|---|---|---|
| ① | 生产接线批的**同款终态建议**：「收件箱拉起（闸在主代理）：bind 终态建议=100.64.0.8:8765（gitea 链需要；127.0.0.1 腿已被 ACL 变更+端到端证明不必要）」 | `30-工件/janitor-20261103/prod-wiring-report.md:74` | bind 终态＝`100.64.0.8:8765` |
| ② | 门户转发目标**写死**：`TRIGGER_INBOX_URL = os.environ.get("AF_TRIGGER_INBOX_URL", "http://100.64.0.8:8765/inbox/gitea-webhook")` | `tools/af-jobs-portal.py:148-149` | 回环绑定 ⇒ 门户转发**断链** |
| ③ | headscale ACL：「gov(srv-1)→pandev 现仅放行 {2222, 8765}（原只有 2222；此即 …：**8765 为 af.trigger.v1 收件箱 gitea 触发链专用**）」 | `状态/网络与出口.md:96` | 8765 **单条专用**、非公网、**非** portal 当年 `0.0.0.0` 经 caddy 三公网入口那种暴露 |
| ④ | 上一在役实例本来就是 `100.64.0.8:8765`：`trigger-inbox 起（100.64.0.8:8765，home=…，sources=['gitea-webhook']…）` | `状态观测/trigger-inbox.log:1` | 上一实例的 bind 实测值 |

**★ 序列 A（只绑回环）不采用——理由**：

1. **断链**：门户转发目标**写死私网 IP**（依据②）⇒ 只绑 `127.0.0.1` ⇒ 门户连本机被拒 ⇒
   门户回**结构化 502**（上游 `upstream_status` 502），gitea→收件箱链**断**；
2. **无本地中继可补**：本批**禁反代/隧道**（且禁新增守护）⇒ 回环形态下**没有**任何
   在本机把 `100.64.0.8:8765` 转接到 `127.0.0.1:8765` 的合法路径 ⇒ 「最小暴露面」换来的是
   「链断」，**不划算**；
3. **序列 B 的暴露面并不宽**：非回环**不是**通配——`0.0.0.0`/`::`/空仍**一律拒起 exit 3**；
   tailnet 私网单口另有**上游 ACL 双闸**（依据③：gov→pandev **仅** {2222, 8765}、非公网）
   ⇒ 暴露面＝「私网单口 + ACL 单条」；
4. **可审计**：序列 B 的显式确认值写进**在役进程 environ**（`/proc/<pid>/environ` 可读），
   且**缺确认即 exit 3**（fail-loud，不静默起非回环监听）⇒ 「非回环」是**显式、可追溯的动作**，
   不是随手一个 env。

**★ 本批不改 bind 闸的代码语义**：`resolve_bind()` 与其 fail-loud 行为**逐字不动**
（`tools/trigger-inbox.py` 的 bind 相关字节与前批一致；`resolve_bind()` 仍在
`main()` 起 socket **之前**调用，拒起即 exit 3）。**本返修批对 `tools/trigger-inbox.py`
零改动**（sha256 仍是 `8a1bdb69fb581eabdac80942ca1ad7c5a9cc428c696a7912be72f9a50bf1b912`）。

**★ 裁决的执行态实测（返修批亲读，非自述）**：主代理已于 **2026-11-04 23:57:55** 按序列 B
起生产收件箱 —— 在役进程 `pid=1523087`、`cmd=python3 tools/trigger-inbox.py`、
cwd=`/home/pan-ding/workspace/agent黑灯工厂`（⇒ 跑的是**工作区级在役运行位**）、
`/proc/1523087/environ` 实读 `AF_TRIGGER_BIND=100.64.0.8` ＋ `AF_TRIGGER_BIND_ACK=100.64.0.8`
＋ `AF_TRIGGER_INBOX_PORT=8765`；`ss -lntp` 命中 `LISTEN 100.64.0.8:8765`；
`状态观测/trigger-inbox.log` 首行自述「绑定面确认：非回环 100.64.0.8（AF_TRIGGER_BIND_ACK
已显式确认…）」⇒ **裁决与在役态一致**。

### 2.3 为什么是 bind → secret（顺序有因）

1. **暴露面先定形**：bind 是本批唯一不可逆（进程在役期间）的暴露面决定；先把它钉死，
   凭据才有「挂在一个已知窄面上的」前提。
2. **被拒时凭据根本不入 env**：bind 闸 fail-loud（exit 3）发生在**起 socket 之前**
   （`tools/trigger-inbox.py` `main()`：先 `resolve_bind()` 再 `ThreadingHTTPServer`）。
   序列把 secret 放在 bind 之后 ⇒ 绑定被拒时**段B 密钥连 shell 环境都没进过**，
   不会有「密钥已入 env、进程随后却绑了更宽的面」的窗口。
3. **反序的具体危害**：secret 先 source ⇒ 一旦 bind 被误配/放宽（`0.0.0.0`），
   段B 密钥就挂在一个对外可达的收件端上，而收件端是**唯一**用该密钥验签的进程。
4. 附注：段B 与段A（`AF_GITEA_HOOK_SECRET`）相互独立，门户被攻破也拿不到收件箱密钥
   （`30-工件/janitor-20261103/prod-wiring-secrets.md`），故本条的密钥暴露面=收件端单点。

### 2.4 停机 / 回滚 / 只增不删

- **停机**（主代理动作）：
  `pkill -f 'python3 .*tools/trigger-inbox.py'` 或按 pid；停后 `ss -lntp` 8765 应无监听行、
  `/proc` 扫描应零 `argv[1] basename=trigger-inbox.py` 进程。**本批停机读数见 §2.5**。
- **回滚**：
  - 收件端：**停进程即回滚**（契约 `af.trigger.v1.md` §回滚）；inbox 文件**留证不清理**。
  - 代码面（工作区在役运行位）：`cp tools/trigger-inbox.py.20261104-pre-触发常驻批-暴露面收窄.bak
    tools/trigger-inbox.py`（回滚到 sha `58f0afee…`；注意该版**无 ack 闸**，回滚后暴露面收窄
    闸同时消失 ⇒ 回滚必须立刻回到序列 A 或补确认）。
  - 代码面（af 仓正本）：`git checkout -- tools/trigger-inbox.py`（本批未 commit，工作树里直接删
    该新增文件亦可）。
  - 安灯面（判据行④）：`tools/factory-keepalive.py.20261104-pre-触发源断供安灯判据行.bak`
    （批次前原版，sha `24bf1dac…`）覆盖回原版即可 ⇒ keepalive 自刷新（os.execv，pid 不变）
    30s 内采纳；回退后**源断供安灯失效，既有 `trigger-inbox-down` 安灯不受影响**（既有行未动）。
- **只增不删**（审计面）：
  - 收件侧：`inbox/<source>/<delivery_id>.json` 只新增；重复 delivery 走 `200 duplicate`
    **不重写**（sha256/mtime 不变读数见 §3）；生产副本内**零删除调用**
    （`grep -n "os.remove|os.unlink|rmtree|truncate|shutil"` → ZERO）。
  - 读侧：`consumed.jsonl` 只追加（`tools/event-router.py:2656` `_trigger_append_consumed`）。
  - 判据面：`状态观测/trigger-inbox-source-silent.json` 每轮重写（报活心跳，同 event-router
    那份 `trigger-inbox-liveness.json` 的机制）；`alarms.log` 只追加。

### 2.5 暴露面读数（逐字）

隔离实例（临时端口 18765，缺省 bind）起服时的 `ss -lntp` 命中行：

```
LISTEN 0      5      127.0.0.1:18765      0.0.0.0:*    users:(("python3",pid=1475664,fd=3))
```

- **本地地址列 = `127.0.0.1:18765`** ⇒ 逐字**没有** `0.0.0.0:<port>` 绑定
  （注：LISTEN 行第 5 列的 `0.0.0.0:*` 是**对端列**，不是绑定面；判定按本地地址列）。
- 非回环绑定检查：`ONLY-LOOPBACK`；`0.0.0.0:<port>` 绑定计数 = **ZERO**。
- bind 闸实测：`AF_TRIGGER_BIND=0.0.0.0` ⇒ **exit 3**（`绑定地址被拒（'0.0.0.0'）——通配/空绑定
  一律禁起`）；非回环且无 `AF_TRIGGER_BIND_ACK` ⇒ **exit 3**（`绑定地址被拒（100.64.0.8）——
  非回环绑定须显式确认`）。两次都**未真正监听**（进程起即退）。
- **停机读数**（`raw/stop-reading.txt`）：`trigger-inbox` 在役进程 **EMPTY（零进程）**；
  `8765/18765/18766` 监听行 **EMPTY**；停机返回码 `-15`（SIGTERM）。
  同表附全机非回环 LISTEN 体检（2222 sshd / 7890 mihomo / 4317-4318 otelcol 等）——**均为本批
  之前既有**，本批**零新增非回环监听**。

---

## ③ 五态矩阵 + fail-closed + 幂等 + 原子落盘（用生产副本自证）

被测件 = **af 仓生产副本** `/home/pan-ding/workspace/af-wt-t5t6/tools/trigger-inbox.py`
（sha256 `8a1bdb69fb581eabdac80942ca1ad7c5a9cc428c696a7912be72f9a50bf1b912`）。
隔离：临时端口 18765/18766、隔离 `AF_TRIGGER_HOME=/tmp/t5t6-verify/sandbox/home`
（注册表＝正本副本）、**一次性测试 secret（进程内存生成、测完即弃、证据零正文）**。
原始读数：`raw/five-state-readings.json`。

| 态 | 判据 | 实测响应 | 结论 |
|---|---|---|---|
| ⑥ 合法 | `POST /inbox/gitea-webhook` + 合法签名 | `202 {'status':'accepted','delivery_id':'t5t6-valid-0001','path':'inbox/gitea-webhook/t5t6-valid-0001.json','thread_key':'git:opchadmin/af@0123456789ab'…}` | ✅ |
| ⑤ 重复 delivery | 同 body 同 delivery 重投 | `200 {'status':'duplicate','delivery_id':'t5t6-valid-0001'…}`；**盘面 sha256 `2b618d0b…` → `2b618d0b…` SAME；mtime `1793806627859150008` → 同值 SAME** | ✅ 不重写 |
| ① 体积 > 1MiB | body = 1MiB+10 B | `413 {'error':'payload_too_large','limit_bytes':1048576}` | ✅ |
| ② 未知 source | `/inbox/no-such-source` | `422 {'error':'unknown_source','detail':'source 不在注册表: no-such-source'}` | ✅ |
| ③ 坏签名 | 签名头错值 | `401 {'error':'bad_signature','key_id':'pocd-webhook-key-1'}` | ✅ |
| ③ 缺签名头 | 不带 `X-AF-Signature` | `401 {'error':'bad_signature',…}` | ✅ |
| ③ 错密钥 | 用别的密钥签 | `401 {'error':'bad_signature',…}` | ✅ |
| ④ 坏消息（非 JSON） | `not-json-at-all` | `422 {'error':'bad_message','detail':'body 不是合法 UTF-8 JSON'}` | ✅ |
| ④ 坏消息（顶层非对象） | `[1,2,3]` | `422 {'error':'bad_message','detail':'body 顶层必须是 JSON 对象'}` | ✅ |
| ④ 坏消息（缺 event） | 无 `event` 字段 | `422 {'error':'bad_message','detail':'event_type 不在白名单: None','allowed':['push','pull_request']}` | ✅ |
| ④ 坏消息（事件类型不在白名单） | `event=release` | `422 {'error':'bad_message','detail':"event_type 不在白名单: 'release'"…}` | ✅ |
| ④ 坏消息（delivery_id 非法） | `bad id/../x` | `422 {'error':'bad_message','detail':'delivery_id 缺失或非法（^[A-Za-z0-9._-]{1,128}$）'}` | ✅ |
| ④ 坏消息（delivery_id 缺失） | 无 delivery_id | `422`（同上 detail） | ✅ |

### 3.1 fail-closed 两项硬证（把 secret 拿掉重跑）

另起实例（端口 18766，同一 home，**不给任何 secret env**）：

- `/healthz` ⇒ **`503 {'status':'not_ready','secrets_missing':['gitea-webhook'],'bind':'127.0.0.1','port':18766}`** ✅ 不假绿；
- 空密钥 HMAC 签名 ⇒ **`401 {'error':'secret_not_configured','detail':'…fail-closed 拒绝验签，不降级空密钥'}`** ✅
  （**绝不用空密钥降级验签**：`or b""` 前科已修死）；
- 真签名（正确 secret 值）⇒ **同样 401** ✅（secret 未配置 ⇒ 一律拒收）；
- 期间 inbox 落盘数 **2 → 2（零落盘）** ✅。

### 3.2 幂等（文件不重写）

- 顺序重投：sha256 **不变**、mtime **不变**（§3 表）；
- **并发重投（同 delivery 8 并发）**：`202×1 200×7`，inbox 内该 delivery **只落 1 个 .json**
  ⇒ 生产副本新增的进程内幂等锁（`_DUP_LOCK`）把原 PoC 的「查存在→写」TOCTOU 竞态收掉
  （原实现并发下会双双落盘、后写覆盖前写＝违反「不重写」）。

### 3.3 原子落盘 + 只增不删

- 落盘 = `tmp` → `fsync(file)` → `os.replace` → **`fsync(目录)`**（生产副本新增③）；
  测后 `.tmp` 残留数 = **0**；
- 落盘信封七字段齐备：`contract/event_id/thread_key/payload_ref/payload_sha256/dedup_key/
  source/received_at` 全部在位（读数见 `raw/five-state-readings.json`）；
- 收件侧文件清单只增：`inbox/gitea-webhook/{t5t6-concurrent-1.json, t5t6-valid-0001.json}`；
- 生产副本**零删除调用**（grep ZERO）。

### 3.4 独立佐证：PoC 自检面在生产副本上跑

`python3 af/poc/poc-super-agent/tests/selfcheck.py`（自检 `INBOX_TOOL` 指向工作区在役运行位＝
本批同步后的同一份字节）：**27 PASS / 2 FAIL**（原始输出 `raw/selfcheck-output.txt`）。
收件端与消费腿相关项**全 PASS**：S2（127.0.0.1 最小暴露面）、P1/P2/P2b/P3（202 + 原子落盘 +
七字段 + sha256 复算）、N1 401、N2 413、N3（重复 200 且 mtime 不变）、N4/N5 422、N14（空密钥
HMAC 401）、N12（healthz 503 not_ready）、N13（未配 secret ⇒ 401 + 零落盘）、P4/P5/P6/P7/P8/A1
（消费腿 F1/F2 派单、order_id 确定性、`consumed.jsonl` 一行一事件、防重放、安灯判据文件 green）。
**两个 FAIL 与本批无关**：① `S0 桥已构建`（Rust bridge 二进制不存在：`af-wt-pocd` 工作树不在盘），
② `S1 Slow Path 六步`（同上，缺 bridge ⇒ `decision-artifact.json` 缺失 ⇒ 脚本在 :434 抛
FileNotFoundError 收尾）。**二者都需 `cargo build` 构建 Rust 桥，本批禁改 `crates/`、禁本地
重编译 ⇒ 未补**（如实登记为环境缺件，非本批改动所致）。

---

## ④ 源断供安灯设计 + 两形态自证读数（两码不同）

### 4.1 先厘清盘面实况（任务书前提已过期）

「源断供安灯**未接线**」这一前提**不成立**：路由侧的那盏灯**早已在跑**——
`tools/event-router.py:2662 _trigger_liveness_andon()` 每轮读注册表阈值判静默，写
`状态观测/trigger-inbox-liveness.json` + `alarms.log`（码 `trigger-inbox-silent`，跃迁必写 +
6h 刷新）。**它缺的恰恰是本批要求的那个区分**：

| | 路由侧 `trigger-inbox-silent`（既有，在跑） | 本批新增 `trigger-inbox-source-silent` |
|---|---|---|
| 判据 | 收件箱目录里**最新一投递**的 mtime 龄（阈值＝注册表 `liveness.inbox_silent_after_min`=240） | 同阈值，但**叠加在进程态之上** |
| 进程态 | **不知**（只看文件）⇒ 收件箱**进程不在**时它照样报「静默」（盲区） | **知**（keepalive 判活①②③全过才评估）⇒ 进程不在时**不报**新码 |
| 落面 | 判据文件 `trigger-inbox-liveness.json` + `alarms.log` | 判据文件 `trigger-inbox-source-silent.json` + `alarms.log` + MES 事件 |

⇒ 本批**不是**新建第四个守护，也**不是**重造一盏灯：**在既有安灯面（keepalive＝安灯三件套之
一）上补一条判据行④**，把「进程不在」与「进程在但源头断供」**物理分家**。

### 4.2 设计（落点纪律：只加判定行，不改启动/调度逻辑）

改件：**工作区级 `tools/factory-keepalive.py`**（安灯三件套之一；在役 pid 2079410，自刷新机制
`os.execv` 换新码、pid 不变）。**只加不改**（逐行说明见 §4.4）：

- **新增常量**（`:1598-1615`）：`TRIGGER_INBOX_SILENT_CODE="trigger-inbox-source-silent"`、
  去重窗 1800s、缺省阈 240（注册表缺 `liveness` 时的回落）、判据文件路径
  （`AF_TRIGGER_SILENT_F` 可覆盖，默认 `状态观测/trigger-inbox-source-silent.json`）、
  报警面（`AF_TRIGGER_ALARMS_LOG` 可覆盖，默认既有 `ALARMS_LOG`）；
- **新增判定函数**（`:1634-1762`）：`_trigger_inbox_silent_threshold`（**配方驱动**：读在役
  注册表运行位 `<home>/registry.json` 的 `liveness.inbox_silent_after_min`；显式 `None` 判
  ——0 是合法阈值，falsy 不吞；注册表读不到 ⇒ 不可证 ⇒ 不报绿也不报琥珀）、
  `_trigger_inbox_newest_delivery`（只读扫 `inbox/*/*.json` 的 mtime）、
  `_trigger_inbox_criterion_write`（tmp+`os.replace` 原子写）、
  `_trigger_inbox_silent_state_raw`（形态判定 + 报警）、`_trigger_inbox_silent_state`
  （**永不抛异常**：本判据行不得带崩看护轮）；
- **既有 `check_trigger_inbox` 加 5 处调用**（`:1772,1782,1787,1793,1797`）：①③ 判活不通过 →
  只镜像状态到判据文件（**不发新码**）；①②③ 全过 ⇒ 评估形态B；
- **既有 `fix_trigger_inbox` 逐字未动**（`diff` 读数：IDENTICAL）⇒ 形态A 的既有
  `trigger-inbox-down` 报警行**原样保留**。

**形态与码（分家）**：

| 形态 | 条件 | 码 | 谁发 |
|---|---|---|---|
| A 进程不在 | argv basename 扫描不到 `trigger-inbox.py` | **`trigger-inbox-down`（既有）** | 既有 `fix_trigger_inbox`（未动） |
| B 进程在但源头断供 | ①②③ 全过 **且** 最新投递龄 > 阈值(240min) | **`trigger-inbox-source-silent`（新）** | 新增判据行④ |
| 第三形态 | 进程在但②③不过（目录不可写/缺 secret） | 无码（`degraded`） | —— |
| 不可证 | 阈值/锚点读不到 | 无码（`unknown`） | ——（fail-closed） |

**起算锚点**（一处**刻意**与路由侧不同）：有投递 ⇒ 锚点＝**最新投递 mtime**；无投递 ⇒ 锚点＝
**在役进程启动时刻**（`proc_start_epoch` + `boot_time`）。
理由：投递**只增不删**，进程重启**不应把断供时钟清零**（若用 `max(投递, 进程启动)`，每次重启
都会把断供时钟归零 ⇒ 源断供永报不出——这正是路由侧那份的盲点）。

### 4.3 两形态自证读数（隔离沙箱，调用**在役** keepalive 的真实判定函数）

原始读数 `raw/lamp-readings.json`。隔离：`AF_TRIGGER_SILENT_F` / `AF_TRIGGER_ALARMS_LOG` 指到
沙箱、`MES_CH_URL` 指死地址（MES 写必失败、不污染生产 `af_mes`）、隔离 `AF_TRIGGER_HOME`
+ 临时端口 18775 + 一次性测试 secret；测完实例已停（rc=-15）。

**形态①（收件箱在 + 源头断供）**：

```
判活结论   : alive=True  detail=进程=[1481843] home=/tmp/t5t6-verify/lamp/A/home inbox 可写 secret 在位
判据文件   : status=amber  alarm_code=trigger-inbox-source-silent  silent_s=18000（300min）  threshold_min=240  process_alive=True
message    : trigger-inbox 源断供：收件箱进程在（pid=[1481843] …）但最新投递距今 300min（>阈 240min）——形态B（源头无事件可路由）；…与『进程不在』（trigger-inbox-down）**分码**
alarms.log : ['trigger-inbox-source-silent@2026-11-04 23:39:39']
断言       : has_source_silent=True   has_down=False   ← 新码已发、既有 down 码未出现
对照组     : 把该投递 mtime 改成现在后，同一判定函数 ⇒ status=green（silent_s=0）
             ⇒ 证明琥珀由「龄」驱动，**不是常量报警**
```

**形态②（收件箱进程不在）**：

```
判活结论   : alive=False  detail=进程不在（trigger-inbox.py；拉起闸=主代理，本守护只报警不拉起）
判据文件   : status=down  alarm_code=trigger-inbox-down  process_alive=False
既有 fix 路径实发（沙箱 alarms.log）: ['trigger-inbox-down@2026-11-04 23:39:39']
断言       : has_down=True   has_source_silent=False   ← 既有码在场、**新码未覆盖**
两码并置   : 形态①码=['trigger-inbox-source-silent'] | 形态②码=['trigger-inbox-down']
```

**★ 在役已生效（不是沙箱自说自话）**：keepalive 自刷新在 **23:38:50** 采纳新码
（`/tmp/factory-keepalive.log`：`自身源码变更（sha 24bf1dac4d18→ed4804bad6c0）→ 自我重载
（os.execv 载入新码，pid 不变=2079410）`），随后在役每轮写
`状态观测/trigger-inbox-source-silent.json`（读数 ts 2026-11-04 23:39:23，
`status=down / alarm_code=trigger-inbox-down / process_alive=false`）⇒
**形态A 在生产现态下就是「既有 down 码 + 新码不出现」**，与沙箱形态②一致。

### 4.4 逐行说明（既有守护件只改了判定行）

| 位置 | 改动 | 性质 |
|---|---|---|
| `factory-keepalive.py:1598-1615` | 新增 5 个常量（码/去重/阈/判据文件路径/报警路径） | **新增**，不动既有常量 |
| `:1634-1762` | 新增 5 个函数（阈值/最新投递/判据文件写/形态判定/入口包装） | **新增**，既有函数一行未改 |
| `check_trigger_inbox`（`:1764-1800`） | **加 5 处调用**（`:1772,1782,1787,1793,1797`）+ 注释 | 判定行**只增**；既有①②③判定行与返回值语义逐字未动 |
| `fix_trigger_inbox`（`:1802-1826`） | **零改动**（`diff` = IDENTICAL） | 形态A 报警行原样 |
| `TARGETS` / `CHECKS` / `FIXES` / `one_round` / `main` / 轮询周期(30s) / 自刷新段 | **零改动** | **启动/调度逻辑未动** |
| 新增 `AF_TRIGGER_SILENT_F` / `AF_TRIGGER_ALARMS_LOG` 两个 env 覆盖口 | 默认值＝生产路径，env 只作**沙箱自证隔离**用 | 只增、默认不变 |

---

## ⑤ 存储形态对账结论（JSONL vs per-delivery JSON + 消费腿后果 + 迁移路径）

### 5.1 两侧原文

- **方案**（`计划/产品化工作方案-v0.3-20261103.md:63`）：「inbox 契约（合并两版）：af.trigger.v1
  字段（event_id/thread_key/payload_ref/payload_sha256/dedup_key）+ 存储用 glm 版**一行一事件
  JSONL（读侧游标）**；thread_key=域:业务对象 id」——**注意括号里的限定语「读侧游标」**。
- **契约**（`af/poc/poc-super-agent/contracts/af.trigger.v1.md:47-56`，§4「存储形态」）：
  **收件侧**＝`af-state/triggers/inbox/<source>/<delivery-id>.json` **一投递一文件**（tmp+
  `os.replace` 原子落）；**读侧**＝`af-state/triggers/consumed.jsonl` **一行一事件**（游标真源），
  状态文件 `trigger_consumed` 为加速缓存，两者不一致**以 JSONL 为准**。
- **PoC 实现**（`af/poc/poc-super-agent/tools-mirror/trigger-inbox.py:19,260-269`）：收件侧落
  `inbox/<source>/<delivery-id>.json`（per-delivery 文件）；读侧游标＝`consumed.jsonl`。

### 5.2 裁定：**生产用「收件侧一投递一文件 + 读侧一行一事件 JSONL 游标」＝契约 §4 形态＝PoC 实现
形态；本批不做迁移**

理由（逐条可核）：

1. **方案那句的落点就是读侧**：方案原文自带限定语「**读侧游标**」，而读侧 JSONL 游标
   （`consumed.jsonl`）**现役已实现且被消费腿当真源用**（`tools/event-router.py:2467,2628-2660`
   `_trigger_consumed_ledger` / `_trigger_append_consumed`，`:2729 poll_triggers` 以它去重）。
   ⇒ 「PoC 与方案在**读侧完全一致**」，不存在任务书担心的「PoC 违背方案」。
2. **方案没有规定收件侧的落盘形态**（它只写了读侧）；收件侧形态由**契约 §4** 钉死为
   per-delivery 文件 ⇒ 按契约走即合规，**不构成「悄悄按现状定案」**（有明文依据、有理由、有
   消费腿后果、有迁移路径，见下）。
3. **收件侧若改单文件 JSONL，四个实质代价**：
   - **幂等主键退化**：现形态「文件存在即重复」是 O(1) 幂等（文件名即主键
     `<source>/<delivery_id>`）；JSONL 单文件下重复检测要全量扫描 + 处理尾部半行（进程在写
     途中被杀 ⇒ 半行），O(1) → O(n) 且引入解析脆弱面。
   - **原子性**：per-delivery 文件的 `tmp+os.replace` 是**单条**原子替换；JSONL 追加只能
     靠行级锁 + fsync，崩在写中间就留半行。
   - **只增不删的审计面**：JSONL 追加也是只增，但**无法隔离一条坏记录**（坏行只影响扫描，
     要么跳过要么整条链不可信）；per-delivery 文件天然按文件隔离（消费腿已是「信封不可读→
     跳过」，见 `tools/event-router.py` `触发信封不可读（跳过）`）。
   - **体量**：JSONL 单文件无限增长 ⇒ 要分片/轮转 ⇒ 破坏「只增不删」审计面的简单性；
     per-delivery 天然分片。
4. **消费腿后果（读侧按文件扫 JSON vs 按游标扫 JSONL）**：
   - 消费腿**现在就是**「文件枚举 + 游标去重」：`sorted(glob(inbox/*/*.json))` 逐个验信封
     （`payload_sha256` 对 `body_b64` 复算、冗余字段逐项核对），未在 `consumed.jsonl` 的才消费，
     消费后**追加一行** JSONL。
   - **幂等主键与去重语义不因存储形态改变**：三道防重放＝① `<source>/<delivery_id>`（收件侧
     文件存在 + 读侧游标两处）② `dedup_key`（业务内容）③ `order_id`＝`sha256(repo+suite+subject+
     recipe_digest)` 前 16 hex（确定性单号）。改 JSONL 只动①的**实现**（存在性查 → 扫描查），
     ②③语义**完全不受影响**。
   - **一个既有弱点（如实登记，与存储形态无关）**：消费腿按**路径字典序**枚举（`sorted(glob)`），
     **不是**投递时序 ⇒ 消费顺序 ≠ 发生顺序（乱序消费）。换 JSONL 也解决不了（除非按
     `received_at` 排序重写）；**本批不动消费腿**（禁改既有件 + 非本批职责）。
5. **迁移路径（若将来口径仍要改单文件 JSONL，三阶段、可回退）**：
   - 阶段1：收件侧**双写**（per-delivery 文件为真源 + 追加一份 `inbox.jsonl`），消费腿仍读文件；
   - 阶段2：消费腿改读 JSONL（按 `delivery_id`+`payload_sha256` 去重，坏行跳过并记拒读），
     两套游标并行对账一周；
   - 阶段3：确认读侧稳定后停收件侧文件写（**审计面仍保留只增文件，不需要删**）⇒ 净效果是
     「JSONL 变成派生索引」，而非替换真源。
   - **本批裁定：不做迁移**（理由如 5.2.3）；路径留档以备口径变更。

### 5.3 注册表纪律核（秘密不落注册表）

- 生产副本：秘密只从 env `AF_TRIGGER_SECRET_<SOURCE>` 取（`secret_for()`，`None` ⇒ 401
  fail-closed），**注册表只登记 `key_id`**（`pocd-webhook-key-1`）——生产副本与注册表**都守住**
  这条；注册表运行位 `af-state/triggers/registry.json` 与正本
  `af/poc/poc-super-agent/registry/trigger-registry.v1.json` 的差异只有 `$comment` 与
  `recipe_digest`（读数：语义 diff 仅这两项）。
- 新增的判据④ 只从注册表读**阈值**（`liveness.inbox_silent_after_min`），**不读秘密**。

---

## ⑥ 门读数与边界

### 6.1 门读数（原始：`raw/gate-readings.txt`）

| 门 | 命令 | 结果 |
|---|---|---|
| 格式门 | `cargo fmt --check --all`（worktree，CARGO_TARGET_DIR=/data/bench/targets/t5t6-trigger） | **exit 0** ✅ |
| cargo build/test/clippy | —— | **未跑**（本批禁改 `crates/` ⇒ 产物由主线全量门复验批承担；本机 14G 内存禁高并发重编译）——**如实登记未跑** |
| af 仓零改动（主树 `/…/af`） | `git status --porcelain` | 仅 `?? redteam/scans/2026-11-04-e4484537/`（**本批之前既有**的未跟踪目录，非本批产生）⇒ **本批对主树零改动** ✅ |
| refs/指针 | `git rev-parse HEAD origin/product/main product/main` | 三者同 = `56ff52a57c022983d74d994fd4e1c63e98f45235` ⇒ **HEAD 与 product/main 指针未动** ✅ |
| PoC 原件 blob | `git hash-object` vs `git rev-parse origin/product/main:poc/…` | 三件**逐字相同**（`3ad5fef7…`/`9573b21a…`/`a5c19dd1…`）⇒ `af/poc/poc-super-agent/**` **零改动** ✅ |
| 判据面零弱化 | `git diff --stat origin/product/main -- crates/ ci/ Cargo.toml Cargo.lock` | **空** ⇒ crates/ci/Cargo 零 diff ✅ |
| 自跑面门 | 五态 6 条全在 / fail-closed 两项 / 幂等 / 暴露面 / 源断供两形态 | 全绿（§3、§4）✅ |
| 自检面 | `af/poc/poc-super-agent/tests/selfcheck.py` | **27 PASS / 2 FAIL**（2 FAIL ＝ Rust 桥未构建，环境缺件，§3.4） |
| 构建盘 | `/data/bench/targets/` | 本批**未跑重编译**，未在 `/data/bench` 动 af-target 与 targets 以外的东西 ✅ |

### 6.2 如实登记的边界

> **★ 本节是「施工批收卷当刻」的快照，原文整段保留不删。**
> 返修批（2026-11-05）现态覆盖见 **§7**：其中 §6.2-1（bind 取值待拍板）与 §6.2-2
> （生产收件箱仍未在役）**已被 §7 取代**（bind 已裁决＝序列 B；收件箱已于
> 2026-11-04 23:57:55 按序列 B 起在役）。

1. **★ bind 取值是主代理裁决项，本批不替主代理选**：本批纪律字面要求「只许绑 127.0.0.1」，而
   现役 Fast Path（gov 门户 → `100.64.0.8:8765`，ACL 只放行 gov→pandev 8765）需要 tailnet 私网
   绑定。两条序列与后果见 §2.2；本批**不启动任何非回环监听**，只把非回环做成**显式确认+拒起
   fail-loud**（不可误配成宽暴露）。**主代理按序列 A 拉起 ⇒ gitea→收件箱链断（门户 502）；按
   序列 B 拉起 ⇒ 保链但监听面为 tailnet 私网。**
2. **生产收件箱仍未在役**：本批**没有**拉起生产实例（拉起闸=主代理）。现役状态实测：零
   `trigger-inbox` 进程、8765 无监听、`alarms.log` 里 `trigger-inbox-down` amber 仍在 1800s
   去重节奏上持续出现（收卷时读数：23:38:52、23:39:12…）。⇒ **欠账「上在役」这一格仍待主代理
   执行启动序列**；本批交付的是**可执行交接件**。
3. **任务书两处前提与盘面不符**（已如实登记并按盘面施工）：① 工作区级 `tools/` 早就有
   `trigger-inbox.py`（不是「目前没有」）；② `tools/event-router.py` **在役**（不是「尚不在役」）
   ⇒ 「源断供安灯未接线」的前提不成立，本批实际补的是**「两形态不分」的洞**（路由侧那盏灯不
   区分进程态）。
4. **路由侧 `trigger-inbox-silent` 仍在报，且与新码语义相邻**：收件箱进程不在时它照样报
   「静默」（路由侧盲区，非本批引入）。本批**不改 event-router**（既有件、职责在读侧）⇒
   在役 `alarms.log` 里两条码可能同现：**形态A 的权威码是 `trigger-inbox-down`**
   （keepalive，既有），`trigger-inbox-silent` 是路由侧观察（不知进程态）。若主代理要消歧，
   需另立一批改路由侧那盏灯（本批不做）。
5. **新码未进 factory-watch 的机器可读聚合面**（`状态观测/latest.json` 的 `trigger_liveness`
   段仍只含路由侧那份）：接线需给 factory-watch **加一条只增臂**读
   `状态观测/trigger-inbox-source-silent.json`，但该臂会新增快照顶层键 ⇒ 触发发布面键集冻结
   闸（`ci/` 冻结键集须人工审定）⇒ **本批不做**，留待主线按发布面纪律接线。
6. **自证副作用（如实登记）**：① 沙箱自证形态②时，既有 `fix_trigger_inbox` 的报警行写的是模块
   常量 `ALARMS_LOG`（生产 `状态观测/alarms.log`）⇒ **生产 alarms.log 多出一行**
   `trigger-inbox-down`（ts 2026-11-04 23:39:12，**内容为真实态复述**，非假报警；已用模块常量
   重定向修正，后续形态②自证零污染）；② 沙箱进程与在役 keepalive 共用 `/tmp/factory-keepalive.log`
   ⇒ 日志里混有沙箱行（含 2 条 `MES 写入失败（不致命）`，因沙箱 `MES_CH_URL` 指死地址）。
   **生产 MES 表零写入**（沙箱指向 127.0.0.1:1，连接被拒）；**生产 inbox 数据面零污染**
   （隔离 home，测完实例已停，停机读数 EMPTY）。
7. **PoC 自检面的 2 个 FAIL 是环境缺件**（Rust 桥未构建），非本批改动所致；本批**未跑**
   `cargo build/test/clippy`（禁改 crates/、禁本地重编译）。
8. **ADR 索引行未补**：`docs/adr/README.md` 的表格是既有节，本批按「docs/ 下仅新增一节、禁改
   既有节」**未改**它 ⇒ 新增的 ADR-015 未进索引，**待主线补登**。
9. **未 commit / 未 push**：分支 `feat/t5-2-t6-3-trigger-resident` 仍指向 `origin/product/main`
   （56ff52a），本批改动全部在工作树里（`git status --porcelain -uall` 可见）。

### 6.3 `raw/` 原始读数清单

| 文件 | 内容 |
|---|---|
| `raw/five-state-readings.json` | 五态矩阵 + fail-closed + 幂等 + 并发幂等 + 原子落盘 + 暴露面 + bind 闸 + 停机（§2.5/§3 全量逐条） |
| `raw/lamp-readings.json` | 源断供安灯两形态自证 + 对照组（§4.3） |
| `raw/stop-reading.txt` / `raw/stop-reading-final.txt` | 停机读数（零进程 / 零监听 / 全机非回环 LISTEN 体检） |
| `raw/selfcheck-output.txt` | PoC 自检面在生产副本上的输出（27 PASS / 2 FAIL） |
| `raw/in-service-readings.txt` | 在役自刷新采纳行 + 生产判据文件现态 + 生产 alarms.log 末 3 行 + 未改动既有件 sha256 对照 |
| `raw/patch-生产副本-vs-PoC原件.diff` | af 仓正本 vs PoC 原件（**PoC 原件未动**） |
| `raw/patch-keepalive-源断供判定行.diff` | keepalive 批次前原版 → 在役新码（**只增不改**） |
| `raw/gate-readings.txt` | 门读数（fmt exit 0 / 主树零改动 / refs 未动 / PoC blob 同值 / crates·ci 零 diff / 工作树改动清单） |

### 6.4 指纹

| 件 | sha256 |
|---|---|
| PoC 原件 `poc/poc-super-agent/tools-mirror/trigger-inbox.py` | `25f0d00e392a0e3fc4d0456f7d87b014f0560585d51d476a806c055130da0bdf`（未动） |
| 生产副本 `tools/trigger-inbox.py`（af 仓正本 ＝ 工作区在役运行位，逐字同源） | `8a1bdb69fb581eabdac80942ca1ad7c5a9cc428c696a7912be72f9a50bf1b912` |
| 工作区在役件改前 `.bak`（2026-11-03 生产接线批版，含无闸 AF_TRIGGER_BIND） | `58f0afee218f38d5eb2c4a488bc47bb4a72698e7fc7921c111e1ce59431ea3a5` |
| keepalive 批次前原版（`.bak`） | `24bf1dac4d1810426e45532562536604f13df123b6aea8bd23e417a9946023df` |
| keepalive 判据行④落地后（在役） | `19dbf455e2e442ca18a7c7a4a60c7ea42e2e39d08731c87226eb2845320113e3` |

**零凭据正文**：本证据与全部原始读数中**零 secret 正文**（测试 secret 由进程内 `secrets.token_hex`
一次性生成、测完即弃，从未回显/落盘）；生产段B 只登记**落位路径与行键名**
（`~/.config/af-trigger/gitea-webhook.env`，mode 600，`AF_TRIGGER_SECRET_GITEA_WEBHOOK=<REDACTED>`）。

---

## ⑦ 返修段（2026-11-05 · v0.3-T5②/T6③ 触发件遗留清偿）

> 返修批＝「三项返修批-G1-T7②-T5②T6③」§四 R3（主代理外派任务书）。
> **本段是追加**：原 §①–§⑥ 的原文与原 `raw/` 读数文件**逐字不改**（只增不删）；
> 新读数落 **`raw/repair-readings.txt`**（全量门/零改动/裁决在役态）与
> **`raw/probe-selfcheck.txt`**（探针判别力自证）。
> **工作树/分支不变**：仍在 `/home/pan-ding/workspace/af-wt-t5t6`、分支
> `feat/t5-2-t6-3-trigger-resident`，**未 commit、未 push**、既有未提交改动**零丢弃**。

### 7.1 本批「四件必做 + 两条不许做」的处置一览

| 项 | 要求 | 处置 | 落点 |
|---|---|---|---|
| ① | `docs/adr/README.md` 索引表**追加恰好一行**（不改任何既有行） | ✅ 已做，`git diff --numstat` ＝ `1 0` | §7.2 |
| ② | 用**当前** `git status --porcelain -uall` **替换** `raw/gate-readings.txt` §6 旧快照 | ✅ 已重出（**替换**非追加） | §7.3 |
| ③ | 落**只读**同源探针（两处 sha256 不一致即判红并报两处各自 sha256）＋ 挂载方案说明（不改 `PIN_MD5` 钉桩）＋ **探针自身要证会咬人** | ✅ 探针本体已落 af 仓 `tools/`；自证 4/4；挂载方案见 **§8**（挂载**另立一批**） | §7.4 / §8 |
| ④ | bind 取值裁决落地（ADR-015 决策 2 ＋ 证据 §2.2；**写明「序列 A 不采用」**）；**不改 bind 闸代码语义** | ✅ 序列 B 裁决与四条依据已落 ADR 决策 2 ＋ §2.2-A；`resolve_bind()` **逐字未动**（`tools/trigger-inbox.py` 本批零改动） | §2.2-A / §7.5 |
| ★不许做① | 新码进 factory-watch 聚合面 + 路由侧进程态感知 ⇒ **本批不做**（接线会新增快照顶层键 ⇒ 触发发布面键集冻结闸，须人工审定）；**不得为「做完」去改 `ci/` 或既有发布面** | ✅ **未做**：本批对 `ci/`、既有发布面（`tools/all-gates.sh`、`tools/deploy-consistency-probe.sh`）**零改动**；形态A 的权威码维持 ＝ keepalive 的 `trigger-inbox-down`；**只把它登记为待办**（ADR-015 后续项 3） | §7.6 |
| ★不许做② | 生产 `状态观测/alarms.log` 那行 `2026-11-04 23:39:12 trigger-inbox-down` **★ 不许删**；在证据里加一段说明它是**沙箱自证副作用** | ✅ **行仍在盘**（未删未改）；说明见 §7.7（含**逐条归因读数**）；并**落到台账**（§7.7.3） | §7.7 |

### 7.2 必做①：ADR-015 索引行（纯插入、0 删除）

```
$ git diff --numstat docs/adr/README.md
1	0	docs/adr/README.md
```

`git diff --stat origin/product/main` ＝ `docs/adr/README.md | 1 +` ／ `1 file changed, 1 insertion(+)`。
⇒ **纯插入、0 删除**、既有行**逐字未动**（`git diff` 的 hunk 头 `@@ -19,3 +19,4 @@` ＝ 只在文件尾
追加 1 行、无 `-` 行）。新增行内容（ADR-015 标题 + 决策摘要 + 「bind 取值＝序列 B」状态）。**逐字 diff 见
`raw/repair-readings.txt` §2。**

### 7.3 必做②：重出改动清单（**替换**旧快照，非追加）

旧快照（§6）早于 ADR 015 与末版 `raw/` 文件落盘 ⇒ 会被读成改动面比实际小。
**做法**：`raw/gate-readings.txt` 的 §6 段被**整段替换**为**当前** `git status --porcelain -uall` 的
真实输出（旧段落的行一个不留、不是追加）。**重出后的清单逐字见
`raw/gate-readings.txt` §6 与 `raw/repair-readings.txt` §3**（两者同一份输出）。
重出后清单 **16 行**（1 行已跟踪修改 ` M docs/adr/README.md` ＋ 15 行新增未跟踪 `??`），
新增件含本批三件新产物：`tools/trigger-inbox-src-parity-probe.py`（探针）、
`poc/poc-t5-2-t6-3-trigger-resident/raw/repair-readings.txt`、
`poc/poc-t5-2-t6-3-trigger-resident/raw/probe-selfcheck.txt`。

### 7.4 必做③：两处 `tools/trigger-inbox.py` 同源纪律的**机械保障**（探针本体）

**落点**：af 仓 `tools/trigger-inbox-src-parity-probe.py`（与 `trigger-inbox.py` 正本**同域**；
探针的 A 侧缺省路径＝**探针自身所在目录**下的 `trigger-inbox.py` ⇒ 探针与被探件不会分家）。
**只读性**：只 `open(path, "rb")` 算 sha256；**零写、零网络、零监听、零换码**
（源码级核：`grep -nE "socket|listen|HTTPServer|requests|urllib|http"` → **0 命中**；
唯一的 `open(...,"w"/"wb")` 只在 `--selftest` 里、只写**探针自己 `mkdtemp` 建的临时目录**、测完自清）。

**判据**（任一不过 ⇒ 非零退出＝红；不静默 SKIP、不猜绿）：

| 情形 | 判定 | 退出码 |
|---|---|---|
| 两处 sha256 相等 | 绿（逐字同源成立） | 0 |
| 两处 sha256 不等 | **红**（漂移）＋ **报出两处各自 sha256** | 1 |
| 任一件缺件/不可读 | **红**（对账不可证 ⇒ 按红处置） | 1 |

**★ 探针自身「会咬人」自证（原始读数 `raw/probe-selfcheck.txt`）**：

| # | 用例 | 期望 | 实测 |
|---|---|---|---|
| 1 | 真盘默认两处（af 仓正本 vs 工作区在役运行位） | 绿 | **rc=0**，`VERDICT=ALIGNED`，两处同为 `8a1bdb69…` |
| 2 | 夹具：两处内容**不同** | **红** | **rc=1**，`VERDICT=DRIFT`，报两处 sha256（`866047d1…` vs `c96d24d1…`） |
| 3 | 夹具：两处内容**一致**（不同文件名） | 绿 | **rc=0**，`VERDICT=ALIGNED` |
| 4 | 夹具：**差一个字节** | **红** | **rc=1**，`VERDICT=DRIFT` |
| 5 | 夹具：**缺件** | **红**（不可证不猜绿） | **rc=1**，`VERDICT=UNPROVABLE` |
| 6 | 真盘：af 仓正本 vs 工作区 `.bak`（2026-11-03 旧版） | **红** | **rc=1**，`8a1bdb69…` vs `58f0afee…` |
| 7 | `--selftest` 内建四例（①不同②一致③差一字节④缺件），**全部以子进程跑本体**（证的是**可执行面**不是函数） | 4/4 符合预期 | **`SELFTEST_VERDICT=ALIGNED_PROBE_WORKS`，rc=0** |

> 用例 6 用的是**真盘历史漂移**：af 仓正本（`8a1bdb69…`）vs 工作区 2026-11-03 改前 `.bak`
> （`58f0afee…`，即本批之前的在役运行位）⇒ 探针对**真实发生过的**漂移形态确实会红。

**挂载**：本批**只交探针本体 + 挂载方案说明**（§8）；**在役发布面的门项登记另立一批**
⇒ 本批**未改** `tools/deploy-consistency-probe.sh` 的 `PIN_MD5` 钉桩（逐字未动，见 §8）。

### 7.5 必做④：bind 取值裁决落地（主代理已裁＝**序列 B**）

- **裁决**：`AF_TRIGGER_BIND=100.64.0.8` ＋ `AF_TRIGGER_BIND_ACK=100.64.0.8`（端口 8765）。
- **落点**：① **ADR-015 决策 2**（含裁决、四条依据、「序列 A 不采用」理由、且声明不改 bind 闸语义）；
  ② **本证据 §2.2-A**（同内容＋**裁决的执行态实测**）。
  原「待主代理拍板」处已改为**已裁定**（§2.2 标题与「两条序列的取舍」引语均已改；原「本批不替主代理选」
  一句**保留不删**并**显式标注为施工批当刻口径、已被本裁决取代**）。
- **★ 不许改 bind 闸语义 ⇒ 本批遵守**：`resolve_bind()` 与其 fail-loud 行为**逐字未动**
  （本批对 `tools/trigger-inbox.py` **零改动**，sha256 仍是 `8a1bdb69fb581eabdac80942ca1ad7c5a9cc428c696a7912be72f9a50bf1b912`；
  `resolve_bind()` 仍在 `main()` 起 socket **之前**调用 ⇒ 拒起即 exit 3）。
- **★ 裁决与在役态一致（返修批亲读，非自述）**：主代理已于 **2026-11-04 23:57:55** 按序列 B 起生产收件箱：
  `pid=1523087`、`cmd=python3 tools/trigger-inbox.py`、`cwd=/home/pan-ding/workspace/agent黑灯工厂`
  （⇒ 跑的是**工作区级在役运行位**）、`/proc/1523087/environ` 实读
  `AF_TRIGGER_BIND=100.64.0.8` ＋ `AF_TRIGGER_BIND_ACK=100.64.0.8` ＋ `AF_TRIGGER_INBOX_PORT=8765`
  （**段B secret 键名在位、值不回显**）；`ss -lntp` 命中 `LISTEN 100.64.0.8:8765`；
  `状态观测/trigger-inbox.log` 首行「绑定面确认：非回环 100.64.0.8（AF_TRIGGER_BIND_ACK 已显式确认…）」。

### 7.6 ★不许做①：新码进 factory-watch 聚合面 + 路由侧进程态感知 ⇒ **本批不做**（只登记待办）

- **本批零改动的面**（逐条可核）：`ci/`（**零 diff**）、`tools/all-gates.sh`（既有发布面，未动）、
  `tools/deploy-consistency-probe.sh`（`PIN_MD5` 钉桩**逐字未动**）、`tools/factory-watch.py`（未动）、
  `tools/event-router.py`（未动）。
- **不做的理由（任务书原文口径）**：接线会给 `状态观测/latest.json` **新增快照顶层键**
  ⇒ **触发发布面键集冻结闸（须人工审定）** ⇒ **不得为「做完」去改 `ci/` 或既有发布面**。
- **登记为待办（不是遗漏，是裁决）**：ADR-015「后续项」第 3 条已登记（`docs/adr/015-…md`）；
  形态A 的权威码**维持** ＝ keepalive 的既有 `trigger-inbox-down`（既有报警行**零改动**）。

### 7.7 ★不许做②：`状态观测/alarms.log` 那行 `2026-11-04 23:39:12 trigger-inbox-down` ⇒ **不删** ＋ 说明

#### 7.7.1 行仍在盘（**未删、未改、未重排**）

```
$ grep -n '"code": "trigger-inbox-down"' 状态观测/alarms.log | sed -n '18p'
2135:{"ts": "2026-11-04 23:39:12", "level": "amber", "code": "trigger-inbox-down", "message": "trigger-inbox 失联：进程不在（trigger-inbox.py；拉起闸=主代理，本守护只报警不拉起）；不自动拉起（拉起闸=主代理），恢复=按裁决 bind/secret 串行拉起", "since": "2026-11-04 23:39:12"}
```

（`状态观测/alarms.log` ＝ **审计面历史记录，禁删**；返修批**只读不写**该文件。）

#### 7.7.2 ★ 说明：它是**沙箱自证副作用**（非在役来源；内容为真实态复述、非假报警；生产 inbox 数据面与判据面零污染）

| 维度 | 结论 | 依据（逐条） |
|---|---|---|
| **来源** | **沙箱自证进程**，**不是**在役 keepalive | 同一秒相邻行 `[2026-11-04 23:39:12] [trigger-inbox] MES 写入失败（不致命）: <urlopen error [Errno 111] Connection refused>`（`/tmp/factory-keepalive.log:4786`）是**沙箱指纹**（沙箱 `MES_CH_URL` 指死地址 `127.0.0.1:1`）；在役 keepalive 的 MES 腿走真实 MES 地址、不会 Connection refused |
| **成因** | 沙箱自证形态②时，既有 `fix_trigger_inbox` 的报警行写的是**模块常量 `ALARMS_LOG`**（＝生产 `状态观测/alarms.log`），不是沙箱重定向后的路径 | `tools/factory-keepalive.py:1815` `with open(ALARMS_LOG, "a", …)`；施工批已用模块常量重定向修正（**后续形态②自证零污染**：沙箱 23:39:39 的同码报警**未**出现在生产 alarms.log） |
| **内容真伪** | **非假报警**：内容是**真实态复述**（当时生产态确实是「收件箱进程不在」） | 相邻的在役两行 `23:38:52`（在役 keepalive 23:38:50 自我重载后首轮）与 `23:39:55`（在役 keepalive 23:39:54 自我重载后首轮，`/tmp/factory-keepalive.log:4790-4794`）**说的是同一件事、同一文案** ⇒ 沙箱这行与在役两行**互相印证**，只是**写入者**不同 |
| **生产 inbox 数据面** | **零污染** | 沙箱用隔离 `AF_TRIGGER_HOME=/tmp/t5t6-verify/sandbox/home` ＋ 临时端口 18765/18766；生产 `af-state/triggers/inbox/gitea-webhook/` 仍是**两个**投递（`82fa25d2-…json`、`verify-main-20261103-1.json`，均为 2026-11-03 旧件）；沙箱实例**测完即停**（`raw/stop-reading.txt`：零进程、零监听） |
| **生产判据面** | **零污染** | 沙箱把 `AF_TRIGGER_SILENT_F` / `AF_TRIGGER_ALARMS_LOG` 重定向到沙箱；生产判据文件 `状态观测/trigger-inbox-source-silent.json` 由**在役** keepalive 写（`tool=factory-keepalive/trigger-inbox`），内容与沙箱无关 |
| **处置** | **保留该行**（审计面历史记录禁删）；在本证据与台账**登记其来源** | 本节 §7.7 ＋ 台账（§7.7.3） |

#### 7.7.3 该说明**落到台账**

- 台账件：`00-项目部/建设台账-20261103.md`（「批次跟踪单一事实源」）的**决策登记**段 —— 本返修批
  **追加一条** dated 条目（**只增不改既有行**；改前件留 `.bak`），载明：① 上述 §7.7.2 的来源判定；
  ② **P4 待办登记**（新码进 factory-watch 聚合面 ⇒ 发布面键集冻结闸须人工审定 ⇒ 本批不做）；
  ③ 本批四件必做的落点与门读数指针。
- 债表 `30-工件/剩余欠账总表-20261104/剩余欠账总表.md` 的 `v0.3-T5②/T6③` 行（:19）**本批未改**
  （该表是盘点快照、行态回写归主代理；**只增不改纪律**下不在返修批里动它）。

### 7.8 门读数（返修批亲跑；逐条）

| 门 | 命令 | 结果 |
|---|---|---|
| **格式门** | `cargo fmt --check --all`（worktree，`CARGO_TARGET_DIR=/data/bench/targets/t5t6-trigger`） | **exit 0** ✅ |
| **cargo build / test / clippy** | —— | **未跑**（理由：禁改 `crates/` ⇒ 产物由主线全量门复验批承担；本机 14G 内存禁本地重编译）——**如实登记未跑**，不写成已跑 |
| **ADR 索引行** | `git diff --numstat docs/adr/README.md` | `1  0  docs/adr/README.md` ⇒ **纯插入、0 删除** ✅ |
| **af 仓零改动核** | `git diff --stat origin/product/main` | `docs/adr/README.md | 1 +` ／ `1 file changed, 1 insertion(+)` ⇒ **只有本批授权的那一行** ✅ |
| **crates/ ci/ Cargo 零 diff** | `git diff --stat origin/product/main -- crates/ ci/ Cargo.toml Cargo.lock` | **空** ⇒ 零 diff ✅ |
| **crate 数** | `ls crates/ | wc -l` ／ `git ls-tree origin/product/main crates/ | wc -l` | **22 → 22** ✅ |
| **ci/clippy-baseline.txt 未变** | `sha256sum` vs `git show origin/product/main:…` | 两边同 `a48639e75b5203fef7e1ffc7026b8f0d9de20f3ce862fa3988a1ec6b49326ba4` ✅ |
| **refs/指针未动** | `git rev-parse HEAD origin/product/main product/main` | 三者同 `56ff52a57c022983d74d994fd4e1c63e98f45235` ✅ |
| **PoC 原件 blob 逐字未动** | `git hash-object` vs `git rev-parse origin/product/main:poc/…` | 三件同值（`3ad5fef7…`/`9573b21a…`/`a5c19dd1…`）⇒ `af/poc/poc-super-agent/**` **零改动** ✅ |
| **af 主树零改动** | `git status --porcelain`（主树 `/…/af`） | 仅 `?? redteam/scans/2026-11-04-e4484537/`（本批之前既有）⇒ 本批对主树**零改动** ✅ |
| **重出改动清单** | `git status --porcelain -uall` | **16 行**（1 `M` + 15 `??`），逐字见 `raw/gate-readings.txt` §6 ✅ |
| **探针自跑面（自证会咬人）** | `python3 -I tools/trigger-inbox-src-parity-probe.py [--selftest]` | 真盘 rc=0（绿）／夹具不同 rc=1（红）／一致 rc=0（绿）／差一字节 rc=1／缺件 rc=1／selftest **4/4** `ALIGNED_PROBE_WORKS` ✅ |
| **判据面零弱化** | 本批对 `crates/`/`ci/` 零 diff；探针**只增检出力**（不一致/缺件 ⇒ 红，无任何放宽分支） | ✅ |
| **非回环监听面** | `ss -lntp` 返修批前后对拍 | 本批**零新增监听**；生产收件箱的 `100.64.0.8:8765` 是**主代理 23:57:55 起的在役实例**，非本批所起（见 §7.5） |
| **cargo 三门** | build / test / clippy | **未跑 ＋ 理由**（见上）——如实登记 |

**★ 本批未跑的门**：cargo build / test / clippy 三门（禁改 `crates/` ⇒ 禁本地重编译，产物由主线全量门
复验批承担）；`af/poc/poc-super-agent/tests/selfcheck.py` **未重跑**（施工批已跑过并留读数
`raw/selfcheck-output.txt`，其 2 个 FAIL ＝ Rust 桥未构建＝环境缺件，与本批改动无关；返修批**零改
`af/poc/poc-super-agent/**`** ⇒ 无需重跑）。

### 7.9 返修批指纹

| 件 | sha256 |
|---|---|
| `tools/trigger-inbox.py`（af 仓正本 ＝ 工作区在役运行位，**本批零改动**） | `8a1bdb69fb581eabdac80942ca1ad7c5a9cc428c696a7912be72f9a50bf1b912` |
| `tools/trigger-inbox-src-parity-probe.py`（**本批新增**只读探针） | `ec43302ab4ed99ff9366e38337db73ff9d7b90d36f76444f06b7e35969da7f64` |
| `docs/adr/015-trigger-inbox-resident-contract.md`（**本批改**：决策 2 落裁决＋后续项更新） | 见 `raw/repair-readings.txt` §8（当刻值） |
| `docs/adr/README.md`（**本批改**：索引追加 1 行） | 见 `raw/repair-readings.txt` §8（当刻值） |

---

## ⑧ 同源探针的**挂载方案**（本批只交方案，**挂载另立一批**）

> **★ 本批不动在役发布面**：`tools/all-gates.sh` **未改**、`tools/deploy-consistency-probe.sh` 的
> `PIN_MD5` 钉桩**逐字未动**（那是 af-jobs-portal 的锚，改它＝触发钉桩重锚＋发布面变更）⇒
> **零新增守护**（无 systemd / supervisor / cron / 常驻进程）。

### 8.1 探针本体路径

| 角色 | 路径 | sha256 |
|---|---|---|
| 探针本体（**af 仓**，与 `trigger-inbox.py` 正本同域） | `af/tools/trigger-inbox-src-parity-probe.py` | `ec43302ab4ed99ff9366e38337db73ff9d7b90d36f76444f06b7e35969da7f64` |
| A 侧（af 仓版本受控正本） | `af/tools/trigger-inbox.py`（**缺省＝探针同目录**，不写死路径） | `8a1bdb69…` |
| B 侧（工作区级在役运行位） | `${AF_WORKSPACE:-/home/pan-ding/workspace/agent黑灯工厂}/tools/trigger-inbox.py` | `8a1bdb69…` |

### 8.2 调用点（挂载批次执行；**本批未执行**）

- **门项位置**：在役发布面 `tools/all-gates.sh` 的 **L3 常驻门**族（与
  `sil-verdict-probe` / `live-daemon-version-probe` / `obs-patrol-probe --check` 同族），
  以 `add_item L3 "<门名>" <timeout_s> "<命令>"` **新增一个门项**。
- **建议门项**（两条，与既有先例同构）：

  ```
  add_item L3 "trigger-inbox-src-parity-probe（两处 trigger-inbox.py 同源对账门）" 60 \
    "python3 -I '$AF_REPO/tools/trigger-inbox-src-parity-probe.py'"
  add_item L3 "trigger-inbox-src-parity-probe --selftest（同源对账门判别力负向自测）" 60 \
    "python3 -I '$AF_REPO/tools/trigger-inbox-src-parity-probe.py' --selftest"
  ```

  （`$AF_REPO` ＝ af 仓根；挂载批次须把它解析成**仓根**而不是工作树路径，否则会拿未合并的工作树当受控正本。）
- **退出码语义对齐**：`all-gates.sh` 的 `run_item`（`:360-365`）是 **rc=0 ⇒ PASS、rc=2 ⇒ FAIL、
  其余非零 ⇒ FAIL**；本探针 **rc=0 绿 / rc=1 红**，且**红文案含字面量 `FAIL`** ⇒ `run_item` 的
  `grep -m1 -E 'FAIL|Error|error|Traceback|assert'` 能摘到可读说明（**不静默 SKIP**）。

### 8.3 钉桩如何登记（**不动既有钉桩**）

- **既有钉桩面**：`tools/deploy-consistency-probe.sh` 的 `PIN_MD5="8a7ea5a2132ff1c02b6bccf6b0a059f5"`
  （`tools/deploy-consistency-probe.sh:125`）钉的是 **`tools/af-jobs-portal.py`** 的本地基准
  ⇒ 与本探针**无任何关系**；**本批逐字未动**（挂载批次**也不应动它**：改它会触发 af-jobs-portal
  的钉桩重锚 ＋ 发布面变更，属另一批的授权面）。
- **本探针的「锚」＝它自己的 A 侧**：af 仓 `tools/trigger-inbox.py` **就是**受控正本
  （ADR-015 决策 1）⇒ **不需要**另造钉桩；探针判的是「B 侧是否仍逐字等于 A 侧」，
  **A 侧被换 = git 变更可见**（af 仓是 git 仓、有 diff 可追溯），**B 侧被换 = 探针立刻红**。
- **若挂载批次仍想给 A 侧也钉一个 sha256**：**新增独立钉桩行**（例如在本探针内部或另一份
  独立的钉桩表里登记 `PIN_SHA256_TRIGGER_INBOX=8a1bdb69…`），**不碰** `deploy-consistency-probe.sh`
  的既有 `PIN_MD5` 行；重锚须走既有维护契约（同步钉值 + 基线 mtime + 标记），
  **且 A 侧每一次换码都必须重锚**（否则会把「上游换版」误报成「下游漂移」）。

### 8.4 零新增守护（自证）

- 探针是**一次性只读进程**（`python3 -I …`），由既有 all-gates L3 轮次调用 ⇒ **零新增常驻进程**；
- **零 systemd / supervisor / cron / timer**（挂载批次**不得**为此另起守护）；
- **零监听、零网络**：探针只 `open(path,"rb")` 算 sha256（源码级 grep：`socket|listen|HTTPServer|requests|urllib|http` → **0 命中**）；
- **零新增发布面顶层键**：本门**不写任何状态面/快照文件** ⇒ **不触发发布面键集冻结闸**
  （与 §7.6 的 P4 待办不同面：本门只读、对账，不新增 `latest.json` 顶层键）。

### 8.5 挂载批次需要做的三件事（**本批未做**）

1. 在 `tools/all-gates.sh` L3 族**新增 1–2 个门项**（如 §8.2），**不新增 `ci/run.sh` step 编号**、
   不改既有门项；
2. 解析 `$AF_REPO`（af 仓根）并把 A 侧钉值（如 §8.3）登记在**独立位置**，
   **不碰** `tools/deploy-consistency-probe.sh` 的 `PIN_MD5`；
3. 跑一次 `python3 -I $AF_REPO/tools/trigger-inbox-src-parity-probe.py --selftest` 作为
   **挂载前判别力自证**（不绿不许挂）。


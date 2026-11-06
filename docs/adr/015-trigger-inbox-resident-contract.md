# ADR-015 trigger-inbox 常驻运营面契约（落点 / 暴露面闸 / 源断供安灯分码 / 存储形态）

- 状态：**已裁决（施工侧落地；bind 取值已由主代理裁为序列 B，并于 2026-11-04 23:57:55 按序列 B 起在役）**
- 日期：2026-11-04
- 批次：v0.3-T5②/T6③（收件箱 trigger-inbox 常驻化 + 暴露面收窄 + 源断供安灯）
- 证据：`poc/poc-t5-2-t6-3-trigger-resident/README.md`（六段 + 原始读数；另有 **§⑦ 返修段**
  ＋ **§⑧ 同源探针挂载方案**，返修批 2026-11-05 追加）
- 关联：方案 v0.3 `:53`（收件端独立、最小暴露面）、`:63`（inbox 契约 + 读侧 JSONL 游标）、
  `:64`（Watchdog 复用三件套、源断供安灯必须设计）；契约
  `poc/poc-super-agent/contracts/af.trigger.v1.md`；安灯面 `tools/factory-keepalive.py`

## 决策 1：生产落点 = af 仓 `tools/trigger-inbox.py`（版本受控正本）＋ 工作区 `tools/trigger-inbox.py`（在役运行位），两处逐字同源

契约（af 仓内）写的「三个落点：`tools/trigger-inbox.py`…」是**仓内相对路径**；keepalive 文件头与
PoC 自检面（`INBOX_TOOL`）指向的却是**工作区级**同名文件 ⇒ 两处 `tools/` 不是同一目录。

- **版本受控正本**落在 af 仓 `tools/trigger-inbox.py`：唯一能被评审/回滚/追溯的面
  （工作区根不是 git 仓）。
- **在役运行位**留在工作区级 `tools/trigger-inbox.py`：PoC 自检面、keepalive 文件头、生产日志
  都指向它，挪路径会静默改掉既有指向。
- **同步纪律**：两处必须逐字同源（sha256 对账），af 仓为上游。
- **对安灯探针无影响**：keepalive 三件判据全部与路径无关（①argv basename 匹配；②③读在役进程
  `/proc/<pid>/environ`）。

## 决策 2：暴露面收窄做成**代码级 fail-loud 闸**，非回环须显式确认，通配一律拒起

- `AF_TRIGGER_BIND` 缺省 `127.0.0.1`；`0.0.0.0`/`::`/空 ⇒ **exit 3，无豁免**；
- 非回环绑定须同时给 `AF_TRIGGER_BIND_ACK=<同一地址>`，否则 **exit 3**（确认值写进在役进程
  environ ⇒ 可审计）。
- 决策理由：现役 Fast Path（gov 门户 → 本机 8765）走 tailnet 私网 IP 且 ACL 收窄到
  gov→pandev:8765，但「非回环」必须是**显式动作**而非随手一个 env；收窄闸与现役拓扑不冲突。
- **★ bind 取值已裁决＝序列 B**（**主代理裁决**，2026-11-04）：`AF_TRIGGER_BIND=<内网地址>`
  ＋ `AF_TRIGGER_BIND_ACK=<内网地址>`（端口 8765）。依据（四条逐条可核）：
  ① `30-工件/janitor-20261103/prod-wiring-report.md:74`——生产接线批的**同款终态建议**
     （bind 终态＝<内网地址>:8765；且「拉起闸＝主代理」本就是主代理的闸）；
  ② `tools/af-jobs-portal.py:148-149`——门户转发目标**写死**
     `http://<内网地址>:8765/inbox/gitea-webhook`；
  ③ `状态/网络与出口.md:96`——headscale ACL：**gov→pandev 仅放行 {2222, 8765}**
     （8765 单条专用、非公网、非 portal 当年 0.0.0.0 经 caddy 三公网入口那种暴露）；
  ④ `状态观测/trigger-inbox.log:1`——上一实例本来就是 `<内网地址>:8765`。
- **★ 序列 A（只绑回环）不采用**：门户转发目标**写死私网 IP** ⇒ 只绑回环 ⇒ 连接被拒 ⇒
  门户回**结构化 502**（gitea→收件箱链断）；本批**禁反代/隧道** ⇒ 无本地中继可补 ⇒
  「最小暴露面」换来的是「链断」，不划算。序列 B 的暴露面由**上游 ACL 双闸**兜住
  （gov→pandev 单条 8765、非公网、非通配），且非回环仍必须**显式 ACK**、确认值写进在役
  进程 environ（可审计、fail-loud），**不是随手一个 env 就宽暴露**。
- **★ 本 ADR 不改 bind 闸的代码语义**：`resolve_bind()` 与其 fail-loud 行为**逐字不动**
  （通配拒起 exit 3、非回环无 ACK 拒起 exit 3；`tools/trigger-inbox.py` 的 bind 相关
  字节与前批一致，见证据指纹）。

## 决策 3：源断供安灯接到**既有安灯面**（keepalive 判据行④），两形态**物理分家**

**不新建第四个守护**；既有守护件**只加判定行**，启动/调度逻辑零改动（`fix_trigger_inbox` 逐字未动）。

| 形态 | 判据 | 码 |
|---|---|---|
| A 收件箱进程不在 | argv basename 扫描不到 | `trigger-inbox-down`（**既有**，仍由既有报警行发） |
| B 进程在但源头断供 | ①②③ 判活全过 **且** 最新投递龄 > 注册表 `liveness.inbox_silent_after_min` | `trigger-inbox-source-silent`（**新**） |

- 新码只在形态B 出现 ⇒ **永不覆盖既有 down 码**；形态A 时判据文件自述
  `status=down/alarm_code=trigger-inbox-down`（镜像既有码，非新码）。
- 起算锚点：有投递 ⇒ 最新投递 mtime（投递只增不删 ⇒ 进程重启不把断供时钟清零）；无投递 ⇒
  在役进程启动时刻（刚部署不冤枉）。
- 与路由侧 `event-router._trigger_liveness_andon()` 的 `trigger-inbox-silent` **不同码不同义**：
  那盏灯只看文件、**不知进程态**；本条是**进程态已知**的形态判别。路由侧那盏灯本批不动
  （职责在读侧）⇒ 在役 alarms.log 两条码可能同现，形态A 的权威码是 `trigger-inbox-down`。
- 落面：判据文件 `状态观测/trigger-inbox-source-silent.json`（原子写，机器可读）＋
  `alarms.log`（1800s 去重）＋ MES 事件（同既有面）。

## 决策 4：存储形态＝**收件侧一投递一文件 ＋ 读侧一行一事件 JSONL 游标**（契约 §4 形态，不迁移）

- 方案 `:63` 的「一行一事件 JSONL」自带限定语「**读侧游标**」，而读侧 JSONL 游标
  （`consumed.jsonl`）现役已实现且被消费腿当真源 ⇒ **PoC 与方案在读侧完全一致**；
  方案未规定收件侧，收件侧由契约 §4 钉为 per-delivery 文件 ⇒ 依契约即合规。
- 收件侧若改单文件 JSONL：幂等主键 O(1)「文件存在即重复」→ O(n) 扫描＋半行脆弱面；追加写
  原子性弱于单条 `os.replace`；无法按文件隔离坏记录；单文件无限增长要分片 ⇒ 收益为负。
- **消费腿后果**：消费腿本就是「文件枚举＋游标去重」（`sorted(glob(inbox/*/*.json))` → 验信封 →
  未在 `consumed.jsonl` 的才消费 → 追加一行）。**幂等主键（`<source>/<delivery_id>`）与三道去重
  语义不因存储形态改变**；改 JSONL 只把①的存在性查换成扫描查。
- 已知弱点（如实登记，与存储形态无关）：消费腿按**路径字典序**枚举 ⇒ 消费顺序 ≠ 投递时序。
- 若将来仍要改单文件 JSONL：三阶段迁移（双写 → 消费腿改读 → 停收件侧文件写；审计面只增不删，
  净效果是 JSONL 变派生索引），**本批不做**。

## 决策 5：注册表纪律（秘密不落注册表）维持

生产副本秘密只走 env `AF_TRIGGER_SECRET_<SOURCE>`（未配置 ⇒ 401 fail-closed，绝不用空密钥降级
验签；`/healthz` 报 503 not_ready）；注册表只登记 `key_id`。新增判据④ 只从注册表读**阈值**。

## 后续项（留给主线，非本批）

1. **拉起已执行**：主代理按**序列 B** 于 **2026-11-04 23:57:55** 起生产收件箱
   （`AF_TRIGGER_BIND=<内网地址>` ＋ `AF_TRIGGER_BIND_ACK=<内网地址>`，端口 8765；
   在役进程 environ 实读一致）。残余＝段B secret 是否在位（`/healthz` 报
   `secrets_missing` 与否）——**拉起闸与恢复闸仍属主代理**，守护只报警不拉起。
2. **索引行已补登**：`docs/adr/README.md` 索引表**追加恰好一行**（返修批做；
   `git diff --numstat docs/adr/README.md` ＝ `1 0` ⇒ 纯插入、0 删除、不改任何既有行）。
3. **新码进 `状态观测/latest.json` 聚合面** ⇒ 需给 factory-watch 加**只增臂**并按**发布面键集
   冻结闸**走人工审定（新增快照顶层键 ⇒ 须人工审定）⇒ **本批不做**，只登记为待办
   （形态A 的权威码维持 ＝ keepalive 的 `trigger-inbox-down`）。
4. 路由侧 `trigger-inbox-silent` 的进程态盲区（另一盏灯）如需消歧，另立一批。
5. **两处 `tools/trigger-inbox.py` 同源探针的挂载**：本批只交**探针本体**
   （af 仓 `tools/trigger-inbox-src-parity-probe.py`，只读 sha256 对账，自证会咬人）
   ＋**挂载方案说明**（见证据 §8）；**在役发布面 `tools/all-gates.sh` 的门项登记另立一批**
   ⇒ 本批**未改**既有发布面，**未改** `tools/deploy-consistency-probe.sh` 的 `PIN_MD5` 钉桩
   （那是 af-jobs-portal 的锚，改它＝触发钉桩重锚＋发布面变更），**零新增守护**。

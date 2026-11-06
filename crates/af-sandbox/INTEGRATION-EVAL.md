# INTEGRATION-EVAL — CubeSandbox 集成评估（波次 4 遗留件："CubeSandbox 集成评估进 Tool broker IDL"）

日期：2026-10-19 ｜ 对象：`af/crates/af-sandbox`（沙箱会话语义层）× CubeSandbox v0.7.3（PoC 实测通过）

输入材料（本文所有引用均带 file:line，零编造）：

- `af/poc/poc-sandbox/REPORT.md` — CubeSandbox v0.7.3 全链实测（KVM 裸机路径、闭环、冷启动、内存、SDK 面）
- `af/crates/af-sandbox/RESULT-UPDATE.md` + `af/crates/af-sandbox/src/` — W5A 刚合流的沙箱客户端语义层
- `设计/产品设计_拍板版.md` §四 — 十三抽象中的 Tool / Credential / Trace / Evaluation 条目

---

## ① Tool broker IDL 草案

### 1.0 设计立场

拍板版把 Tool 定为"系统调用"级抽象："agent 的一切外部能力经进程外 broker 验权后在沙箱内执行"（`设计/产品设计_拍板版.md:106`），产线运转同样写明"工具调用经 broker 进沙箱"（`设计/产品设计_拍板版.md:149`）。所以 IDL 的切分原则只有一条：

> **broker 是决策点（验权、扣预算、定幂等键、发 fencing 纪元）；执行面只接受"已决策完毕"的调用。**

af-sandbox 已经按这个原则实现：`Session::exec` 的顺序注释钉死为"围栏、后端可用性、会话超时、凭证白名单、预算决策、后端、耗时上限"，且"凭证与预算失败都不会调用后端"（`af/crates/af-sandbox/src/session.rs:237-239`）；`Session::open` "不检查凭证，也不扣预算"（`af/crates/af-sandbox/src/session.rs:150`）。IDL 只是把这个既成事实补成显式接口。

### 1.1 调用面（broker → 执行面）

```text
ToolInvocation {
  tool_name      : String          // 权限词表名，沙箱执行即 "sandbox:exec"
                                   //   （af-sandbox 已定名：src/scope.rs:20）
  params         : JsonValue       // 工具自身参数，按工具声明版本化的 JSON Schema 校验；
                                   //   沙箱 exec 时编译为 Command argv
                                   //   （argv 全非空校验：src/types.rs:80-86）
  credential_ref : CredentialRef   // 凭证引用：broker 侧凭 id 取出 Biscuit 令牌后以
                                   //   CredentialScope<'_>（token, namespace, spend_cents）
                                   //   借用传入（src/scope.rs:23-45）。裸令牌不进 params。
  budget_ref     : BudgetRef       // 预算扣点引用：broker 在调用前已完成的类型化扣费回执。
                                   //   af-sandbox 的对应类型是 BudgetDecision::Charged
                                   //   { dimension, slots, decided_at, release_at }
                                   //   （src/budget.rs:26-34），映射 af_meter 的
                                   //   Meter::Concurrency::acquire（af/crates/af-meter/src/lib.rs:377-380，
                                   //   计量树叶子 Meter::Concurrency 定义 lib.rs:146-150）。
  fence          : FenceCtx        // fencing 上下文 = (boot_id, epoch, session_id)，
                                   //   即 SessionHandle { id, epoch, boot_id }
                                   //   （src/session.rs:22-27）。
}
```

四元组的每一项都不是新发明，各自有已合流的载体：

| IDL 字段 | 已有实现 | 证据 |
|---|---|---|
| `tool_name` | `SANDBOX_EXEC_TOOL = "sandbox:exec"` | `af/crates/af-sandbox/src/scope.rs:20` |
| `credential_ref` | `CredentialScope`（Biscuit 借用 + 命名空间 + 单次美分上限） | `af/crates/af-sandbox/src/scope.rs:23-45`；判定走 `af_credential::denial(token, tool, namespace, spend_cents)`（`af/crates/af-credential/src/credential.rs:251-256`） |
| `budget_ref` | `BudgetDecision`（Charged / Refused） | `af/crates/af-sandbox/src/budget.rs:47-50` |
| `fence` | `SessionHandle` 纪元围栏 | `af/crates/af-sandbox/src/session.rs:22-27` |

**凭证引用的判定契约**（已钉死，IDL 原样继承）：

- 一次判定只注入一个 `request_tool`——biscuit check 是存在性查询，多事实注入有一个命中即放行（`af/crates/af-credential/src/credential.rs:182-184`；`af/crates/af-sandbox/src/scope.rs:3-6`）。
- 放行不看 `inspect` 的事实存在性单独定论：衰减删不掉 `tool(...)` 事实，"事实还在不代表工具仍可用"；`fact_present` 只是拒绝结果上的观察字段（`af/crates/af-sandbox/src/scope.rs:7-8`；实现 `scope.rs:52-69`；回归 `tests/semantics.rs:193` `attenuated_token_keeps_fact_but_exec_is_denied`）。
- `denial` / `inspect` 的真错误（限额、令牌损坏）是 `CredentialFault`，fail-closed 不放行（`af/crates/af-sandbox/src/scope.rs:51`）。
- 凭证里的 `spend_cents` 是单次美分上限，不是并发槽；两者不能混（`af/crates/af-sandbox/RESULT-UPDATE.md:53`）。

**预算扣点引用的契约**：

- 扣费发生在 broker（通常 `af_meter::Concurrency::acquire`），不在 `exec` 路径里——`Session::exec` 不再调用扣费钩子，避免同一次 exec 扣两次（`af/crates/af-sandbox/src/budget.rs:61-64`）。
- 只有"沙箱并发维且至少一槽"的 `Charged` 能进入 exec；拒绝、零槽、其它维原样放回错误，不改写（`af/crates/af-sandbox/src/budget.rs:109-123`）。
- `decided_at` / `release_at` 单位由调用方与 meter 约定，af-sandbox 不换算、到点不替调用方 `release`（`af/crates/af-sandbox/src/budget.rs:9-10`）。

### 1.2 返回面（执行面 → broker）

```text
ToolOutcome =
  | Ok(ArtifactBundle)      // 执行完成。注意：非零退出码也是"完成"——命令跑完了
                            //   （src/session.rs:29；回归 tests/semantics.rs:153
                            //    nonzero_exit_is_a_finished_exec）
      ArtifactBundle {
        exit_code  : i32
        stdout/stderr : bytes
        elapsed_ms : u64        // 后端报告的耗时，broker 用它复核超时
        files      : Vec<Artifact { path, bytes }>   // collect 收走的产物原文
                            //   （src/types.rs:114-118；collect 契约
                            //    session.rs:276-291：缺一个路径整次失败，
                            //    会话超时后仍可收文件）
      }
  | Err(ToolError)          // 类型化错误，绝不合成为一个字符串
                            //   （RESULT-UPDATE.md:54 反馈 4）
```

broker 拿到 `ArtifactBundle` 后把 `files` 字节写入内容寻址存储，**工件引用（内容寻址 id）由 broker 层签发**——拍板版 Artifact 抽象："一切产出都是不可变的内容寻址块"（`设计/产品设计_拍板版.md:101`）。执行面只产出字节，不产生工件身份；这是 IDL 返回"工件引用"与 af-sandbox 返回"文件字节"之间的明确分工。

### 1.3 类型化错误与优先级链

`SandboxError` 的变体即 IDL 的错误码表（`af/crates/af-sandbox/src/error.rs:11-83`），语义互斥、fail-closed：

| IDL 错误码 | af-sandbox 变体 | 语义 |
|---|---|---|
| `FENCE_STALE` | `StaleEpoch`（含 request/current 的 epoch+boot 四元组） | 旧纪元句柄，不探活、不重开（`error.rs:16-25`） |
| `FENCE_GONE` | `SessionGone` / `SessionClosed` | 本宿主无此会话；同纪元重复关闭是成功（`error.rs:27-33`） |
| `SANDBOX_DOWN` | `SandboxDown` | 后端不可用，区别于"这次请求被拒"（`af/crates/af-sandbox/src/backend.rs:26-29`） |
| `DEADLINE` | `TimedOut` | 注入时钟到会话上限，或后端报告耗时到达上限（后一种丢输出，`tests/semantics.rs:354`） |
| `CREDENTIAL_DENIED` | `ToolDenied { tool, fact_present, denial }` | 原始 `Denial` 不改写透传（`error.rs:35-44`） |
| `CREDENTIAL_FAULT` | `CredentialFault` | 判定器自身错误，fail-closed（`error.rs:46-48`） |
| `BUDGET_REJECTED` | `BudgetRejected { decision }` | 决策原样带回（`error.rs:50-52`） |
| `BACKEND_REJECTED` / `ARTIFACT_MISSING` | 同名 | 后端拒绝 / 产物缺路径（`error.rs:70-75`） |

判定优先级已由测试钉死：围栏 → 可用性 → 会话截止 → 凭证 → 预算 → 后端 → 后端报告耗时（`af/crates/af-sandbox/RESULT-UPDATE.md:54`；回归 `tests/semantics.rs:538-576` 三条优先级测试）。超时判定是闭区间且加法溢出按超时拒绝（`af/crates/af-sandbox/src/session.rs:416-422`），全链不睡眠（`RESULT-UPDATE.md:47`）。

### 1.4 af-sandbox 作为执行面的挂接方式

四层结构，接缝只有一个 trait：

```text
Tool broker（决策层：凭证 denial 判定 / meter 扣点 / 幂等键 / 工件入库 / trace 事件）
      │  ToolInvocation（已决策：credential_ref + budget_ref + fence）
      ▼
af-sandbox 语义层（复核层：Session / SandboxHost / 纪元围栏 / 超时 / 预算决策复核）
      │  SandboxBackend trait —— 唯一执行面接缝
      ▼                      （af/crates/af-sandbox/src/backend.rs:35-60：
      │                        available / generation / open / exec / collect / close / retire_all）
CubeAdapter（待建：CubeSandbox 线协议客户端，实现 SandboxBackend）
      │  REST / SDK（控制面 127.0.0.1:13000，E2B 兼容 POST /sandboxes）
      ▼
CubeSandbox v0.7.3（每沙箱 = 1 个 shim 内嵌 VMM 进程，持 /dev/kvm）
```

挂接要点：

1. **`generation` 是后端的第一公民契约**：后端进程重启必须改变 `generation`，客户端据此把纪元加一并作废旧句柄；服务不可达时 `available` 为 false（`af/crates/af-sandbox/RESULT-UPDATE.md:56` 反馈 6；机制实现 `af/crates/af-sandbox/src/session.rs:338-348` `sync_generation`）。CubeAdapter 的每次 API 会话建立都必须探测/绑定一个单调代际（如控制面实例 id + 启动时刻），这是适配器最容易被做错的一条。
2. **`retire_all` 对应 Cube 的销毁语义**：宿主 `resume` 纪元加一后要求后端丢掉全部会话；后端丢会话失败时纪元仍然已前进、返回 `SandboxDown`，旧句柄不再能执行（`af/crates/af-sandbox/src/session.rs:113-130`）。映射到 Cube 即"对本节点全部已知 sandbox id 幂等 Kill + 后台对账"。
3. **限额注入是 open 时一次性声明**：`Resources { cpu_millicores, memory_bytes, timeout_ms }` 三项构造时拒绝 0（`af/crates/af-sandbox/src/types.rs:29-58`），原样交给后端（回归 `tests/semantics.rs:87`）。这恰与 Cube 的实测接口面一致——资源配额（cpu/mem/disk）在**模板创建时声明**而非运行时改（`af/poc/poc-sandbox/REPORT.md:170`）。
4. **execute-only 的后端**：`SandboxBackend` 的方法签名里没有凭证、预算、身份参数（`backend.rs:35-60`）——执行面在类型层面就无法做决策。这是"只执行不决策"的结构保证，不是约定俗成。

### 1.5 Cube 特有原语在 IDL 中的位置

PoC 验证的快照/回滚/克隆是"纯 E2B 云服务上没有的能力"，REPORT 明确说"是 broker 应该利用的增量能力"（`af/poc/poc-sandbox/REPORT.md:18`），实测数字：snapshot 570ms、rollback 319ms（FS+内存精确恢复）、clone 522ms 且状态延续（`REPORT.md:12,163`）。IDL 归位建议：

- 它们是**会话生命周期操作，不是工具调用**——不带 `credential_ref`/`budget_ref` 的工具语义，而是 Run 的"打检查点后暂停恢复"（Run 抽象原文即有此语义，`设计/产品设计_拍板版.md:100`）。用途映射：任务级 checkpoint/重试 = snapshot→rollback；并行发散 = clone(n)；长闲置省内存 = pause/resume（`REPORT.md:165`）。
- 因此它们**不应塞进 `SandboxBackend::exec`**，而应作为 trait 的后续扩展方法（`snapshot` / `rollback` / `clone_from`）或独立 capability trait，fencing 语义与 `close` 同规：旧纪元拒绝、幂等。当前 trait 未含它们，进 §③ 缺口清单。

---

## ② CubeSandbox 在本设计中的归位：L2 适配器（只执行不决策）

**结论：CubeSandbox 是 L2 执行面适配器。决策（谁能用、花多少、何时作废、如何记账）全部不属于它。** 论据五条：

**论据 1 —— 决策所需的状态 Cube 一概没有。** 有效权限 = 策略引擎允许 ∩ 凭证持有，凭证编码工具白名单、命名空间、多维预算（`设计/产品设计_拍板版.md:103`）。CubeSandbox 默认无鉴权：`CUBE_API_KEY` 未设即关闭，`E2B_API_KEY` 任意占位串可过 SDK（`af/poc/poc-sandbox/REPORT.md:177`）。它连"请求者是谁"都无法表达，更谈不上白名单交集与预算硬闸。凭证判定必须在 broker（af-credential `denial`，`af/crates/af-credential/src/credential.rs:251`），Cube 侧收到的只是"被放行后的执行指令"。

**论据 2 —— 预算硬闸是宪法条款，Cube 的接口面不提供拦截点。** 宪法 9："凡声明为预算的计量，要么可拦截、要么必须明说'只记账不拦截'"（`设计/产品设计_拍板版.md:50`）。沙箱并发维的拦截点在 meter 的 `Meter::Concurrency::acquire`（`af/crates/af-meter/src/lib.rs:377-380`，到期经 `refresh` 自动释放），扣点发生在调用 exec 之前（`af/crates/af-sandbox/src/budget.rs:3-4`）。Cube 的资源上限是模板期静态规格（2vCPU/2GiB/writable-layer，`REPORT.md:168`），是"给多少"，不是"允许多少个同时跑"——前者是执行配置，后者才是预算。

**论据 3 —— af-sandbox 的 trait 形状已在结构上排除执行面决策。** `SandboxBackend` 无权限/预算参数（`af/crates/af-sandbox/src/backend.rs:35-60`）；`exec` 的调用顺序保证凭证与预算失败不触后端（`af/crates/af-sandbox/src/session.rs:237-260`）。CubeAdapter 实现 `SandboxBackend` 即自动落在这个位置，RESULT-UPDATE 反馈 6 已写明适配器的义务："服务不可达时 `available` 为 false；进程重启必须改变 `generation` 并丢掉旧会话"（`af/crates/af-sandbox/RESULT-UPDATE.md:56`）。

**论据 4 —— Cube 的价值恰在执行面，且实测成立。** 硬件级 microVM 隔离实证（guest 内核 `6.6.1199-0009` ≠ 宿主 `7.0.0-38`，`REPORT.md:12,95`）；进程模型干净（每沙箱 1 个 shim 内嵌 VMM、随沙箱生灭无残留，`REPORT.md:14,115`）；冷启动 p50=156.5ms 支撑"每任务一沙箱"而不必池化（`REPORT.md:13,149`）；快照/回滚/克隆原语独有（`REPORT.md:18,163`）。这些全是"怎么跑得好"，没有一条是"该不该跑"。

**论据 5 —— 选型基线本就把它放在协议适配位。** 拍板版技术底座："沙箱（E2B，扩容走同协议的 CubeSandbox）"（`设计/产品设计_拍板版.md:163`）；PoC 实测补充了修正：原生 SDK 为主、E2B 兼容为辅（E2B SDK 数据面本机未端到端跑通，结论基于控制面 REST + 原生 SDK，`REPORT.md:142`）。无论主辅，适配器都把线协议差异封死在 `SandboxBackend` 实现内，broker 与语义层零感知。

**边界（不是什么）**：L2 不代表"薄到没有逻辑"。纪元围栏、超时比较、预算复核这些**客户端语义**在 af-sandbox（L1.5），不在 CubeAdapter；CubeAdapter 只做线协议翻译、代际探测、错误归类（Cube 的 HTTP 错误 → `BackendFailure::Down/Rejected`，`af/crates/af-sandbox/src/backend.rs:26-32`）。决策仍一丝不下沉。

---

## ③ 集成缺口清单（上产线前还缺什么）

按"已有载体 / 缺口 / 产线需要"列出。无工期含义。

| # | 缺口 | 现状证据 | 上产线需要 |
|---|---|---|---|
| G1 | **镜像签名来源未接** | `Image` 只是字符串引用，空白即拒（`af/crates/af-sandbox/src/types.rs:5-23`），无验签钩子；拍板版要求 Skill"装载时验签不可跳过"（`设计/产品设计_拍板版.md:106`），选型含 zot+cosign（`设计/产品设计_拍板版.md:163`） | broker 在 `open` 前验签镜像引用（cosign 签名→内容寻址存储的策略绑定）；`Image::reference` 只接受已验签条目，或增加 `Image::verified(ref, sig_id)` 构造路径。当前任何能打到 API 的字符串都能被 open |
| G2 | **网络策略未建模** | PoC 未执行 allowlist/断网测试（`REPORT.md:192`）；数据面依赖泛解析 DNS `<port>-<sandboxId>.cube.app`，无泛解析环境需 `CUBE_PROXY_NODE_IP` 直连旁路（`REPORT.md:32,173`）；沙箱网段整段 CIDR 预留、需与宿主路由不冲突（`REPORT.md:29,169`） | (a) IDL 增加 `NetworkPolicy`（allowlist/默认断网）进 open 参数并在模板期声明；(b) 适配器内置"直连地址 + Host 重写"注入，不把 DNS 假设泄漏进接口（`REPORT.md:174` 的 IDL 需求原话）；(c) CIDR/端口余量纳入节点级资源管理 |
| G3 | **资源配额维度未登记进 meter** | `Resources` 只有 cpu 毫核/内存字节/超时毫秒三项（`af/crates/af-sandbox/src/types.rs:30-34`）；磁盘（writable-layer-size）、沙箱并发槽、网络带宽均不在其中。并发维 meter 已有（`Meter::Concurrency`，`af/crates/af-meter/src/lib.rs:146-150`）且 af-sandbox 已消费；宪法 9 要求声明为预算的计量可拦截或明示只记账（`设计/产品设计_拍板版.md:50`） | 磁盘/网络维按配方声明登记进 meter 树（`Stock`/`Window` 叶子可承载）；在凭证多维预算里扩列；每沙箱实际占用（shim PSS，实测 176.6MB/个，`REPORT.md:14`）作为容量测量（机制 12，`设计/产品设计_拍板版.md:136`）的采集源 |
| G4 | **认证与多租户** | Cube 默认无鉴权，broker 需自带 API-key 签发与每请求头注入（`REPORT.md:177`） | broker 持有对 Cube 控制面的唯一凭据，租户流量永不直连控制面；适配器为唯一持有方 |
| G5 | **Cube 特有原语未进 trait** | `SandboxBackend` 只有 open/exec/collect/close/retire_all（`backend.rs:35-60`）；快照/回滚/克隆/pause/resume 只有 PoC 实测语义（`REPORT.md:128-134,163-165`） | 扩展 capability trait（snapshot/rollback/clone_from/pause/resume），fencing 与 close 同规；先在 IDL 定稿错误码（部分失败：快照成功但登记失败等） |
| G6 | **线协议适配器不存在** | "没有 CubeSandbox 线协议实现，也没有在本任务里对 v0.7.3 做真实调用"（`af/crates/af-sandbox/RESULT-UPDATE.md:60`）；真实 Cube 不进 `cargo test`（`RESULT-UPDATE.md:9,56`） | CubeAdapter crate：REST 客户端 + `SandboxBackend` 实现 + 错误归类；对照测试（非 CI）对着真环境跑 |
| G7 | **后台对账与孤儿沙箱** | broker 需要 idempotent kill + 对孤儿沙箱的后台对账（`REPORT.md:150`）；af-sandbox 侧 `Drop` 不关闭会话，未 close 的会话要等 `resume` 的 `retire_all` 或后端重启（`RESULT-UPDATE.md:63`） | 对账器：list 语义扫描 → 与宿主会话表比对 → 超时/孤儿幂等 Kill；成本口径（shim 进程数）作为泄漏指标 |
| G8 | **纪元与 boot_id 跨进程持久化** | 纪元/会话表只在内存，不落 SQLite；跨进程延续要调用方自己保存计数器并在启动时 `resume` + 换新 `boot_id`（`RESULT-UPDATE.md:61,55`） | broker 宿主进程需接 durable 执行层（选型 Temporal/DBOS，`设计/产品设计_拍板版.md:163`）落地纪元计数与句柄簿 |
| G9 | **部署配置热路径** | systemd 服务唯一配置入口是 `/usr/local/services/cubetoolbox/.one-click.env`，进程环境不传导（`REPORT.md:181,30` 实证踩坑） | 部署器写该文件而非进程环境；端口/网段避让参数纳入放置决策 |
| G10 | **未测项直通产线风险** | 并发压测（clone n>1、多沙箱并行冷启动）、大文件交换、跨节点 pause/resume（需 S3）、网络策略全部未测（`REPORT.md:192`）；冷启动数字含 SDK RTT 与 Python 开销（`REPORT.md:196`） | 上述各项在准产线环境补测；容量规划用本机数字时要打折（WiFi 主机 + loop-XFS + 控制面同机，`REPORT.md:13`） |
| G11 | **E2B 兼容通道** | `e2b-code-interpreter` 数据面未端到端跑通（需官方 dev-sidecar 或泛解析 DNS），结论仅基于控制面 REST + 原生 SDK（`REPORT.md:193,142`） | 若保留 E2B 兼容为辅路径，需解决数据面寻址；否则 IDL 明确只承诺原生协议 |
| G12 | **RunCode 通道未建模** | `commands.run`（p50≈53ms/条）与 `run_code` code-interpreter 通道（292ms）时延差 5 倍+，REPORT 明确建议 IDL 分开建模（`REPORT.md:155-156`）；af-sandbox `Command` 只有 argv（`types.rs:73-91`） | 增加独立 `run_code` 后端方法或第二工具名（如 `sandbox:run_code`），不复用 exec 接口硬套 |
| G13 | **工具错误止损的数据源** | 机制 6："连续相同的工具错误=模型已无影响力，立即停止"（`设计/产品设计_拍板版.md:130`）；依赖 Trace 记录每次工具调用（`设计/产品设计_拍板版.md:104`） | broker 每次调用把 `ToolInvocation`/`ToolOutcome`（含 §1.3 错误码）写 trace；错误码表的粒度（`ToolDenied` vs `BudgetRejected` vs `TimedOut`）就是止损指纹的粒度——错误合并会直接毁掉这条机制 |

---

## ④ 风险与边界

**R1 —— 寻址旁路是 workaround，不是方案。** 闭环靠 `CUBE_PROXY_NODE_IP` 直连旁路通过（`REPORT.md:32`），本机 `/etc/resolv.conf` 是既有 dnsmasq、任何依赖 systemd-resolved 泛解析的方案都要绕行（`REPORT.md:195`）。产线必须正面解决泛解析 DNS 或内建 sidecar，否则每台宿主都要带专属网络假设。

**R2 —— 实测数字的环境敏感性。** 冷启动 p50=156.5ms 与 README"亚 60ms"的差距来自 WiFi/loop-XFS/控制面同机（`REPORT.md:13`）；容量规划若直接引用本机数字会系统性偏差。量测口径见 `REPORT.md:196`。

**R3 —— 单机起步。** 拍板版已登记："沙箱单机起步（扩容路径已备）"（`设计/产品设计_拍板版.md:195`）。跨节点 clone/pause-resume 需 S3 后端且未测（`REPORT.md:192`），IDL 现阶段不应承诺跨节点语义。

**R4 —— Stub 与真实后端的语义缝。** af-sandbox 的 27 个测试全部跑在 `StubHandle` 上（`RESULT-UPDATE.md:7,45`）。已知缝：Stub 对"未知会话"返回 `BackendFailure::Rejected`（`backend.rs:257-261`），而真实 Cube 在网关重启后同 id 可能指向**别人的沙箱**——这正是纪元 + `generation` 双围栏存在的理由，适配器若偷懒用可预测 id 或不换代际，围栏会被架空。Stub 的"缺产物整次失败、不返回部分字节"语义（`backend.rs:293-306`）也需在真实 `files.read` 上复核。

**R5 —— 并发模型。** 宿主用一把 `Mutex` 串行访问后端，无多线程压测（`RESULT-UPDATE.md:64`）。broker 高并发下这会成为吞吐上限；但语义层串行换来的是围栏判定的简单正确，放并发前必须重证优先级链（§1.3）。

**R6 —— 副作用边界在 IDL 之外。** 沙箱内的网络出流量、写宿主卷不属于本 IDL：宪法 8"一次性外部操作须走副作用网关"（`设计/产品设计_拍板版.md:49`）、机制 7 的幂等键 + fencing + 每日对账（`设计/产品设计_拍板版.md:131`）由 af-effect-gateway 承载。Cube 的 `allow_internet_access` 创建参数（`REPORT.md:132-133`）在 broker 必须被当作副作用网关的输入而非执行面开关。IDL 只保证：进出沙箱的字节都有 broker 画押。

**R7 —— 供应链单点。** 镜像全部来自腾讯 registry、bundle 来自 download.cubesandbox.com/cnb.cool（`REPORT.md:27-28`）；github.com 直连被断（`REPORT.md:55`）。G1 的签名链落地前，这条供应链是不可校验的信任。

**R8 —— 本文档的边界。** 本文是评估与 IDL 草案，未改任何代码；`SandboxBackend` 现有签名不含 G5/G12 的扩展方法，草案若被采纳需另行走 crate 变更流程。本文不包含工期与排期。

---

## 附：一句话归位

CubeSandbox 实测可行且增量明确（microVM 隔离、百毫秒冷启动、快照/回滚/克隆），它的正确位置是 `SandboxBackend` 的一个 L2 实现（`af/crates/af-sandbox/src/backend.rs:35-60`）；Tool broker IDL 的四元组（工具名/参数 JSON/凭证引用/预算扣点引用）与返回面（工件字节→broker 签发工件引用 / 类型化错误）在 af-sandbox 已有逐字段对应物，缺的是 G1–G13 清单上的产线件——其中没有一条能靠 CubeSandbox 自己补齐，这正是"只执行不决策"的证明。

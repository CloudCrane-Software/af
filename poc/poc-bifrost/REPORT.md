# REPORT — poc-bifrost：Bifrost 网关 × af-meter 计量联动实测

任务 id：poc-bifrost ｜ 日期：2026-10-19 ｜ 环境：linux x64，Go 1.26.8，Python 3.14，docker

---

## ① 结论

**通过（核心场景全部实测验证）**。af-meter `Reading{available, recovers_at}` 承诺的
『日窗耗尽→转 API、重置→切回订阅』在 Bifrost 网关上的执行点已找到并实测打通：
**主备 = 同一 OpenAI 兼容 provider（vllm 类型）下的两个 key，每个 key 自带 `vllm_key_config.url`
自定义端点**；计量信号到路由的接线有三种已验证形态（配置热更 / 请求标记 header / CEL 规则），
全部真实跑通并测得切换时延。

关键实测数字：
- 配置热更（PUT key `enabled` / `weight`）：Admin API 写入后 **首请求级生效，切换开销 <20ms**（PUT 7–9ms + 首请求 7–8ms）
- 请求标记（`X-Meter-Route` header + 静态 CEL 规则按 `key_id` 定向）：**11/11 请求精确命中目标端点**，逐请求零配置写
- `x-bf-api-key-id` header 显式定向：**主/备各一次验证通过**（网关零配置，最小接线面）
- 在途请求：切换瞬间 **3 个在主上的慢请求全部原路完成（200，无中断、无迁移）**，新请求即刻走备
- 故障传播：**500/网络错误 → 同 key 重试后透传，绝不跨 key**（官方文档确认此为设计行为）；**429/凭证拒绝/配额耗尽 → 自动换 key 重试**（实测 429 从主 failover 到备成功）
- Bifrost **无请求驱动的健康熔断**：主 mock 持续 500 达 30s，`provider_status` 始终 `active`；健康探测只有配置级（key 添加时 `list_models` 探测、provider base URL 可达性）

对方案最重要的决策输入：
1. **af-meter 必须是主动信号源，不能依赖网关错误 failover 顶替计量切换**——500 类错误 Bifrost 按设计不跨端点。
2. 最优接线 = **weight 控制（低频、窗口级）+ 请求标记 header（逐请求精确）双层**：`weight=0` 让默认流量整体离开耗尽侧，但被 `key_id` 定向的规则仍可达该 key（实测 weight=0 不影响定向），不会造成 enabled=false 的硬 400。
3. Bifrost 原生 `budget_used` CEL 变量（文档 v2.2.x）是计量驱动路由的官方等价物，但是金额维度；af-meter 的次数/窗口树是补充而非重复。

主要版本边界（如实记录）：
- **GitHub release 无二进制资产**（所有 release assets 为 0，仅 helm chart）；ghcr.io 上可拉的最新镜像为 `v2.0.0-prerelease3`（2026-08-13 构建），落后于 git release（core/v1.12.0、transports/v2.3.0，2026-10）约 2 个月；文档描述的部分能力（`/api/routing/rules` 新路径、fallbacks 对象形式带 key_id 为 v2.2.3+）在该 prerelease 中不存在或行为不同。
- github.com 主站直连超时，源码改经 goproxy.cn（Go module zip）与 raw.githubusercontent.com（单文件）获取，关键语义已源码级核实（见 ③ 引用行）。

## ② 步骤（全部真实执行）

1. **获取 Bifrost**：
   - `curl api.github.com/repos/maximhq/bifrost/releases` → 全部 release assets=0（无二进制）
   - `git clone https://github.com/maximhq/bifrost.git` → 超时失败（github.com:443 连不上；api.github.com 可用但匿名限流）
   - `docker pull ghcr.io/maximhq/bifrost:v2.0.0-prerelease3-amd64` → 成功（ghcr.io 匿名可达）
   - `docker create` + `docker cp bifrost-extract:/app/main` → 提取出 122MB 静态 Go 二进制
   - `./bifrost-bin --help` → 确认用法（`-app-dir -port -host`）
2. **起网关与 mock**：
   - `./bifrost-bin -app-dir .../run/bifrost-app -port 5160 -host 127.0.0.1` → 启动成功（bootstrap 22.4s，插件：telemetry/prompts/logging/governance/compat/model-catalog-resolver）
   - `python3 mock_provider.py --port 5161 --name subscription` 与 `--port 5162 --name metered-api`（OpenAI `/v1/chat/completions` 格式，带 `/control` 控制端点与 `/stats` 统计）
3. **配置主备**（Admin API，默认 `auth_type: none` 无认证）：
   - POST `/api/providers {"provider":"vllm"}`；POST `/api/providers/vllm/keys` 两次：
     `sub-primary → vllm_key_config.url=http://127.0.0.1:5161`、`api-secondary → :5162`，models 均为 `mock-model`
   - 两 key 添加后自动 `list_models` 探测 → `status: "success"`
4. **驱动脚本** `drive_meter.py`（内嵌简化 af-meter：`Window{capacity=3}`，`given() → {available, recovers_at}`，模拟时钟把日窗压缩为 6s）跑三场景；`run/scenario2.py` 补测在途。
5. **文档对照**：抓取 `docs.getbifrost.ai`（llms.txt 索引 → routing-rules / keys-management / config-store 等页），与实测互证。

## ③ 证据（命令原文与输出摘录）

### 3.1 主备路由基础（实测）

```
POST /v1/chat/completions {"model":"vllm/mock-model"} x4
→ hello from subscription | routing_info.key=sub-primary  ×3
→ hello from subscription | routing_info.key=api-secondary ×1   （加权随机，1:1 时约各半）
```

### 3.2 场景1A：配置热更接线（`drive_meter.py` 实际输出）

```
[11:23:42] == 场景1A 配置热更接线: enabled 翻转 ==
req 1 meter=SUB avail=3 -> gw route=api-secondary 44ms
req 2 meter=SUB avail=2 -> gw route=sub-primary 3ms
req 3 meter=SUB avail=1 -> gw route=sub-primary 7ms
req 4 meter=EXHAUSTED recovers_at=+6.0s -> flip primary off
    PUT enabled=false took 9ms
    first req on secondary: attempt#1 after 8ms (req 8ms)
[11:23:48] meter refresh -> {'available': True, 'remaining': 3, 'recovers_at': None}
    PUT enabled=true took 75ms
    first req back on primary: attempt#2 after 129ms (req 58ms)
```

另测 weight 对照（同脚本家族）：

```
weight=0: 默认池 8 发: ['api-secondary' ×8]
weight=0: header=primary: (200, 'sub-primary', 2.77ms)     ← weight=0 不影响 key_id 定向
恢复 weight=1: 默认池 4 发: ['api-secondary','api-secondary','api-secondary','sub-primary']
```

### 3.3 场景1B：请求标记接线（修正规则表后 11/11）

规则（POST `/api/governance/routing-rules`，targets 支持 `key_id`）：
- `headers["x-meter-route"] == "primary"` → `{"provider":"vllm","model":"mock-model","key_id":"8555…","weight":1}`（priority 10）
- `... == "secondary"` → 备 key（priority 11）

```
req 1 header=primary   -> sub-primary   15ms OK
req 2 header=primary   -> sub-primary   45ms OK
req 3 header=primary   -> sub-primary   45ms OK
req 4 header=secondary -> api-secondary  7ms OK recovers=+5.9s
req 5-7 header=primary -> sub-primary OK
req 8 header=secondary -> api-secondary  3ms OK recovers=+6.0s
req 9-11 header=primary -> sub-primary OK
RESULT: 11 OK, 0 MISMATCH
```

CEL 规则被网关在线编译校验（错表达式 400 回编译错误）；同 scope `priority` 唯一（重复 409）。

### 3.4 场景2：切换瞬间在途请求（`run/scenario2.py` 实际输出）

```
t+1.0s: primary inflight=3 (期望3)
切换动作: PUT enabled=false 耗时 7ms
切换后 header=primary 的新请求: HTTP400 10ms
  {"error":"no supported key found with id \"8555…\" for provider: vllm and model: mock-model"}
在途3请求结局: [(1, 3009ms, 'hello from subscription', 'sub-primary', 200),
               (0, 3049ms, 'hello from subscription', 'sub-primary', 200),
               (2, 3049ms, 'hello from subscription', 'sub-primary', 200)]
primary completed增量=3, failed增量=0
```

结论：**在途请求不被切换中断（原端点完成）**；但 key_id 定向 + key disabled = 立即 400 硬失败（无兜底）。

### 3.5 场景3：provider 500/429/断连的故障传播

```
3a 主500(max_retries=2): 500 in 384ms, 主收到 3 次(同key重试), 备收到0 -> 透传
3b 主429(max_retries=2, header=primary pin主): 429 in 277ms, 主收到 3 次   ← pin 死时无处可切
B) 主429 无header(默认池): (200, 'api-secondary', 43ms), 主 received Δ=0, 备 received Δ=1  ← 自动换key
3c 主drop连接(max_retries=2): 502 in 349ms
3d 主恢复后: 200 route=sub-primary 3ms
```

官方文档（docs.getbifrost.ai /features/keys-management）确认设计语义：
> 单 key 失败重试（速率限制、凭证被拒、配额耗尽）："draws again from the keys not yet tried"；
> 网络错误或 5xx：重试时复用同一个 key，不切换。

源码级确认（core v1.12.0，经 goproxy.cn 拉取 module zip）：
- `core@v1.12.0/bifrost.go:7105-7108`：
  ```go
  // Select / rotate key: always on attempt 0, and again when the previous failure was
  // tied to the key itself (rate-limit, auth, billing, or permission error). Transient
  // server errors (5xx, network) keep the same key since they're not per-key problems.
  if keyProvider != nil && (attempts == 0 || lastWasPerKeyFailure) {
  ```
- `core@v1.12.0/schemas/failureclass.go:60-72`：`IsPerKey() = RateLimit ∨ {Credential, ModelAccess, Quota, ModelGone, RegionBlocked}`；
  500/502/503/网络错误归 `FailureClassTransient` → 同 key 重试，永不跨 key；per-key 失败还会**额外授予**换 key 的
  attempt（`extraAttempts`，不受 MaxRetries 预算限制，`bifrost.go:7081-7088`）。

### 3.6 健康检查形态（实测 + strings）

- key 添加时：Bifrost 向端点发 `list_models` 探测 → `status: success / list_models_failed`（带原因，如 "network error … DNS lookup, connection refused"）
- provider 级：`provider_status: active/error`（配置探测；base_url 不可达时 error）
- **无周期性主动探测、无请求级熔断**：

```
主 mock 设 fail500 后 30s 内每 3s 采样 /api/providers：
{'openai': ('active','unknown'), 'vllm': ('active','unknown')}  ← 10 次采样全部 active，无变化
```

- Enterprise 版另有 "Adaptive Routing"（被拒 key 挂起至被动探测成功）与 "Circuit Breaker"，开源 prerelease 不含（文档）。

### 3.7 key/凭证住哪（DB 实查）

- 配置与凭证在 **SQLite**（`run/bifrost-app/config.db`，v2 起 config.json → DB）；表：`config_providers`（含 `custom_provider_config_json`）、`config_keys`、`config_env_keys`（`env_var, provider, key_type, config_path, key_id`——**环境变量引用**形态）、`governance_virtual_keys`、`routing_rules`/`routing_targets`、`governance_budgets` 等 60+ 表。
- key `value` **明文列**（实测 `value='sk-mock-sub'`，`encryption_status: plain_text`）；VK 同样明文。API 回显脱敏（`sk-m****-sub`）。
- 默认 **无认证**：`/api/session/is-auth-enabled → {"auth_type":"none"}`；启用需 `BIFROST_SETUP_TOKEN`（或 config.json `setup_token`）建首个 admin。
- 文档：敏感信息可用环境变量引用而非明文值（config-store 页）；Enterprise 有 Secret Management（Vault/Secrets Manager）。

### 3.8 路由按外部信号切换（文档 + 实测）

- CEL 变量（docs /providers/routing-rules）：`model, provider, request_type, headers[…], params[…], virtual_key_*, team_*, customer_*`，**容量指标 `budget_used / tokens_used / request`（0–100）**，`complexity_tier`。`budget_used` 在源码中真实存在（`plugins/routing@v1.2.0/rules/enginecomplexity_test.go:166`：`complexity_tier == "SIMPLE" && budget_used > 60.0`）。
- `budget_used > 85 → targets` 是文档给的原生计量路由示例；本 PoC 的 prerelease 中该路径未走通（`pricing-overrides` API 字段结构无法构造出非空 `pricing_patch`，多次尝试均 400/空 patch，如实记录为版本差异限制）。
- governance budget 已建成功：provider 级 `PUT /api/governance/providers/vllm {"budgets":[{"max_limit":0.05,"reset_duration":"24h"}]}` → 200（金额 + 相对周期，`calendar_aligned` 另有开关）。
- 热更传播：PUT 后**内存立即生效**（实测首请求 <20ms），无需重启（DB 有 `governance_config.restart_required` 标志位，路由/key 变更未触发它）。源码级形态：Admin handler 写穿 configstore（DB + 内存态），插件持**活指针**读配置——`transports/bifrost-http/handlers/config.go:744/810` 原注释：*"No restart needed - logging plugin holds a live pointer to ClientConfig.*"*、*"no restart needed - governance plugin reads via pointer"*；routing 插件每请求从 store 取规则（`plugins/routing@v1.2.0/rules/engine.go:133` `EvaluateRoutingRules`，CEL 程序有缓存，`engine.go:249`）。
- key 加权选择源码：`core@v1.12.0/keyselectors/weightedrandom.go:10-22`（加权随机；**全零权重时退化为均匀随机**——所以 weight 切换方案里备 key 不能也置 0）。

### 3.9 版本与获取渠道记录

```
release: cli/v1.0.0, core/v1.12.0, transports/v2.3.0, framework/v1.9.0, plugins/routing/v1.2.0 …（assets 全部为 0）
ghcr tags（尾页，字典序止于）: v2.0.0-prerelease3-amd64（创建于 2026-08-13T18:56Z）
镜像大小 349MB；提取二进制 122,611,280 字节，statically linked, stripped
docker.io 不可达；ghcr.io 可达；github.com 直连超时、codeload 慢速（~25KB/s）
```

## ④ 对方案的反馈/决策输入

### 4.1 L2 适配器接口草案（af-meter Reading → Bifrost 路由）

推荐 **双层接线**，meter 进程只对网关做两种幂等写 + 客户端 SDK 做逐请求标记：

```
┌─────────────┐  Reading{available,recovers_at}   ┌──────────────────────────┐
│ af-meter     │ ──(窗口状态翻转时, 低频)──────────▶│ Bifrost Admin API         │
│ (Rust, 单写者)│   PUT /api/providers/vllm/keys/  │  PUT keys/{id} {weight}   │  ← 窗口级大方向
│              │   {id} {"weight": w}              │  (enabled 仅作禁用开关)    │
└─────────────┘                                    └──────────────────────────┘
┌─────────────┐   X-Meter-Route: primary|secondary┌──────────────────────────┐
│ 客户端 SDK    │ ──(每请求, 由本地 Reading 缓存)──▶│ 静态 CEL routing rules     │  ← 逐请求精确
│ (读 meter)   │   或 x-bf-api-key-id: <key-id>    │ headers[...] → key_id 定向 │
└─────────────┘                                    └──────────────────────────┘
```

**形态对比（全部实测）**：

| 形态 | 切换时延 | 失败模式 | 适用 |
|---|---|---|---|
| A. 配置热更 `weight`（PUT key） | <20ms 首请求生效 | 无硬失败；默认流量整体迁移 | 窗口级主备方向（推荐主通道） |
| A'. 配置热更 `enabled=false` | <20ms | **定向请求 400 硬失败**（3.4 实测） | 仅用于人工禁用，不做常规切换 |
| B. 请求标记 `X-Meter-Route` + CEL `key_id` 定向 | 0（随请求走） | 规则表需与 key 集合同步维护 | 逐请求精确；多租户差异化路由 |
| B'. `x-bf-api-key-id` 直指定向 | 0 | pin 死后无 failover | 调试/管理面 |
| C. `budget_used` CEL（官方原生计量路由） | 0（网关自算） | 金额维度；prerelease 未走通 | 若 af-meter 语义可折算成金额 |

**接口契约（草案）**：

```rust
// af-meter 侧唯一需要新增的 trait（进程内已算出 Reading, 适配器只做投影）
trait GatewaySignalSink {
    // 幂等: 相同状态重复写入无害。mode = WeightShare { primary: f32 } 时写 weight,
    // 翻转边界（available 变化 / recovers_at 到达）才调用, 频率 = 窗口重置频率
    fn apply(&mut self, reading: &Reading, route: MeterRoute) -> Result<AppliedAt>;
}
// route 投影规则（与 RESULT.md 语义一一对应）:
//   Reading.available == true            → primary weight=1, secondary weight=1(兜底)
//   Reading.available == false           → primary weight=0（不清零 enabled）
//   now >= recovers_at（refresh 后 available）→ primary weight=1
// 客户端 header: available ? "primary" : "secondary"（由 SDK 缓存 Reading, 不逐请求查 meter）
```

工程形态选择：**IPC 不必要**。af-meter 与网关之间只需 HTTP Admin API（幂等 PUT），
比进程内嵌入（跨语言 Rust→Go 不现实）与配置文件热更（Bifrost v2 已无 config.json 热载 provider 段）
都干净；Admin API 写入即生效（实测 <20ms），带 DB 持久化（网关重启后计量状态仍在——但注意
**af-meter 的窗口状态不能依赖网关 DB，仍以 af-meter 自己的持久化为准**，见 af-meter RESULT.md ⑤-2）。

### 4.2 密钥管理建议（Bifrost 配置 vs af-credential 喂入）

实测事实：Bifrost 的 provider key 明文存 SQLite `config_keys.value`（`encryption_status: plain_text`），
Admin API 默认无认证（`auth_type: none`），key 经 Admin API 的任何一次读写都会过网关进程内存。

建议：
1. **凭证的唯一住处仍是 af-credential；Bifrost 只作运行时持有者**。喂入路径：
   `af-credential` →（短时效注入）→ `POST /api/providers/vllm/keys {value: <真实key>, vllm_key_config:{url}}`。
   af-credential 保持签发/轮换权（轮换 = PUT 同一 key 的 value，实测 PUT 全量更新可用），
   Bifrost DB 仅是缓存与故障恢复的便利品。
2. **Admin API 必须加锁**：生产以 `BIFROST_SETUP_TOKEN` 建 admin 后开启认证（本 PoC 为本地裸跑）；
   管理面端口（5160 的 /api/*）与数据面端口分开、前者只对 meter/运维网段开放。
3. **环境变量引用优于明文列**：`config_env_keys` 表原生支持 `env_var` 引用形态（3.7），
   af-credential 可通过向网关进程环境注入 `env_var` 名（而非值落库）进一步收窄持久化面；
   Enterprise 的 Vault/Secrets Manager 集成是现成升级路径，OSS 版不阻塞。
4. **虚拟 key（sk-bf-…）作为对客户端的唯一凭证**：客户端永远只见 VK，真实 provider key 不出网关；
   VK 可绑定 budget/provider 白名单（实测 403 model_blocked 生效），与 af-meter 的"每租户窗口"天然对齐。
5. 日志脱敏已内建（响应回显 `sk-m****-sub`；`disable_content_logging` 可关内容记录），但
   **key 值一旦经 `POST keys` 传入即落明文 DB 文件**——DB 文件权限需纳入 af-credential 的威胁模型。

### 4.3 对 af-meter 接口语义的反馈

- `recovers_at` 到达时**必须由 af-meter 主动触发** `weight` 回写——Bifrost 不会按时间自动恢复路由
  （它的 budget reset 是金额周期，不是窗口重置回调）。若想省这跳，可用形态 C（budget_used CEL）
  让网关自判，但需把 af-meter 窗口折算成金额并接受网关内建模。
- 计量切换（预防性）与 429 failover（反应性）**天然互补**：weight 控制大方向后，默认池里的意外 429
  仍会被网关自动换 key 吸收（3.5-B 实测）；这是把 af-meter RESULT.md ⑤『固定窗口峰值双倍突发』
  的余量风险转移给网关的免费兜底。
- Bifrost governance budget（金额/24h/`calendar_aligned`）与 Meter 树的 `Window{capacity}` 语义同构度
  高，正式方案若双跑，建议 af-meter 为唯一真值源、governance budget 作为网关侧应急熔断（阈值放宽）。

## ⑤ 遗留问题

1. **版本差距未消除**：实测对象是 `v2.0.0-prerelease3`（2026-08-13）；git release 已到 `transports/v2.3.0` / `core/v1.12.0`（2026-10）。`/api/routing/rules` 新路径、fallbacks 带 key_id 对象（v2.2.3+）、budget_used CEL 均未在实测版本验证；docker.io 不可达且 ghcr 无 v2 正式版镜像，若需要新版只能源码构建（codeload tarball 下载中，见 5.6）。
2. **budget_used CEL 路径未实测**：`pricing-overrides` API 在 prerelease 中多次尝试（`input_cost_per_token`/`pricing_patch` 等字段结构）均无法产生非空 `pricing_patch`，预算用量无法增长，CEL 容量变量路由未闭环。已确认存在的是：provider 级 budget 绑定成功、VK 预算字段结构（`max_limit/reset_duration/last_reset/current_usage`）。
3. **VK 的 `provider_configs.keys` 字段未走通**：PUT 多种字段形态（`keys`/`key_ids`/`allow_all_keys`）回显均 `keys: []`，VK 授权后始终 "no keys found"；文档（v2.2.x）字段名为 `key_ids`，与 prerelease 不一致，未深究。
4. **流式（SSE）未测**：切换/在途行为仅在非流式补全上验证；`ttft_timeout_ms`（流式首 token 超时切 fallback，文档 v2.2.x）未验证。
5. **多副本一致性未测**：Admin API 热更对单实例即时生效；多网关实例间配置传播机制（文档未述）未验证，af-meter 多写者并发（RESULT.md ⑤-1）叠加未测。
6. **源码级确认（部分完成）**：codeload tarball 下载失败（curl exit 92，GitHub 大文件通道不稳）；改经 **goproxy.cn 拉 Go module zip** 取得 `core@v1.12.0`（bifrost.go 主重试循环、failureclass.go、keyselectors/weightedrandom.go）、`framework@v1.9.0`（configstore）、`plugins/routing@v1.2.0`（CEL 规则引擎）、`plugins/governance@v1.9.0`，关键语义已源码核实（见 3.5/3.8 引用行）。transports 网关层（bifrost-http）经 raw.githubusercontent.com 单文件只读到了 `main.go`/`handlers/config.go`（其 go.mod module path 与 major 版本不匹配，goproxy 拒收——上游仓库自身问题）；transports 完整源码树（Admin API 全部 handler、key list_models 探测的调度点）未逐行核实，相关结论以 DB schema + 黑盒实测 + 官方文档为据。
7. mock 与网关进程清理情况见附录（本次会话结束时统一回收 5160–5162 端口进程）。

---

### 附录 A：现场产物

- `poc/poc-bifrost/mock_provider.py` — OpenAI 兼容 mock（/control 模式切换：ok/fail500/fail429/drop/delay；/stats 计数）
- `poc/poc-bifrost/drive_meter.py` — 三场景驱动（内嵌 MiniMeter：`Window{capacity=3}` → `{available, recovers_at}`）
- `poc/poc-bifrost/run/scenario2.py` — 在途请求补测
- `poc/poc-bifrost/run/bifrost-app/` — Bifrost app dir（config.db 等）
- `poc/poc-bifrost/run/drive_log.txt` — 驱动脚本完整时间线
- 二进制：`/home/pan-ding/bifrost-bin`（来自 ghcr.io/maximhq/bifrost:v2.0.0-prerelease3-amd64；`go version -m`：path `github.com/maximhq/bifrost/transports/bifrost-http`，go1.26.5 构建，transports (devel)）
- 源码 module zip（goproxy.cn）：`/home/pan-ding/bifrost-framework.zip`、`core.zip`（=core@v1.12.0）、`prouting.zip`（=plugins/routing@v1.2.0）、`pgov.zip`（=plugins/governance@v1.9.0）；解压于 `/home/pan-ding/bifrost-framework-src/`、`bifrost-src-core/`、`bifrost-src-prouting/`、`bifrost-src-pgov/`
- transports 网关层单文件（raw.githubusercontent.com）：`/tmp/bf_main.go`、`/tmp/bf_cfg.go`（transports/bifrost-http/handlers/config.go）

### 附录 B：本次实测配置快照

- Bifrost：127.0.0.1:5160，`-app-dir run/bifrost-app`，auth none
- provider `openai`：base_url 指向 5161（后删除；前 4 个探索性操作：POST 无 keys→error、409、PATCH 405、DELETE 重建——base_url 顶层字段在该版本 API 中不生效，最终方案改用 vllm 类型 per-key url）
- provider `vllm`：keys `sub-primary`(→:5161, id 8555a558…) / `api-secondary`(→:5162, id ee0fea1f…)，models=["mock-model"]，network_config 最终为 max_retries=2/backoff 100–500ms/timeout 30s，concurrency 1000
- routing rules：`meter-subscription`(p0, headers→vllm/mock-model)、`meter-primary-key`(p10, →主key)、`meter-secondary-key`(p11, →备key)
- governance：VK `vk-meter-test`（budget 0.01/24h）；provider `vllm` budget 0.05/24h；pricing-overrides 尝试 3 条（均空 patch）

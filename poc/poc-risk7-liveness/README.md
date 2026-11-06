# poc-risk7-liveness —— 风险#7 收件箱活性安灯：af 产品链（Rust 面）接入

> 风险#7 登记原文（`计划/产品化工作方案-v0.3-20261103.md:128`）：
> 「事件流静默 268h 前科：**新链活性安灯必须建成（建成没事件也要报活）**」
>
> 本批把 PoC-D event-router 的活性安灯从 Python 消费腿**补进 af 产品链
> （Rust 面，`crates/af-order`）**，判定语义与 PoC-D 侧**逐字对齐**（两腿对
> 同一注册表/收件箱给出同一读数），读数可被外部进程以 PoC-D 同款 JSON
> 快照读到。分支 `feature/risk7-liveness`（基线 = 开工时 `origin/product/main`
> = 2542929）。

## 一、改动清单（全部在 `crates/af-order/` + 本证据目录）

| 文件 | 性质 | 说明 |
|---|---|---|
| `crates/af-order/src/router_liveness.rs` | **新增**（约 470 行） | 活性安灯读数腿：liveness 段最小视图（任务书选项②，独立视图类型）+ 静默判定 + 起算锚点生命周期 + PoC-D 同款快照序列化/原子落盘；含 5 个单元测试 |
| `crates/af-order/src/lib.rs` | 修改（+11 行） | 仅模块注册与 re-export（`pub mod router_liveness;` + `pub use router_liveness::{…}`）；零语义改动 |
| `crates/af-order/tests/router_liveness_scenarios.rs` | **新增**（约 300 行） | 四场景 + 锚点生命周期 + 注册表 fail-closed 门 + glob 语义镜像 + 只读性对拍，共 8 个集成测试 |
| `crates/af-order/examples/router_liveness_demo.rs` | **新增**（约 50 行） | 可运行示范（证据脚本入口 + 聚合接线批的调用形态参考）；非产品 CLI 面 |
| `poc/poc-risk7-liveness/*`（本目录） | **新增** | 对齐 harness、四场景读数、只读性对拍、门读数、本 README |

**零触碰**（判据面零弱化的正面清单）：

- `crates/af-order/src/router.rs` **零改动**——消费腿判据面
  （`verify_envelope` / `resolve_adjudication_target` / `load_consumed` /
  `append_consumed` / `TriggerRegistry` 宽松解析契约）原样；
- 既有 `af-order/tests/*.rs` **零改动**（只新增测试文件）；
- `crates/` 其它 crate、`ci/`、根 `Cargo.toml` members、`Cargo.lock`、
  `tools/`、`状态观测/` **零改动**；**零新增 crate、零新增外部依赖**
  （chrono/serde/serde_json/thiserror 均为 af-order 既有依赖）。

## 二、四场景读数（改前 → 改后）

**改前基线**（`before-baseline.txt`）：`origin/product/main` 的
`crates/af-order` 里 grep `liveness` 仅命中 router.rs 一条**自述不消费的
注释**（「正本还有 repo_suites/liveness/slow_path 等段，本模块不消费」）——
**af 产品链 Rust 面对四场景全部无读数可给**（读不到阈值、无静默判定、
无快照面）。安灯只在 PoC-D Python 腿里工作。

**改后**（Rust 腿 = `af-order::router_liveness`；钉死 t0=1700000000，
详细逐字段表见 `align-result.txt`）：

| 场景 | 安排 | Rust 读数 | PoC-D Python 读数 | 一致 |
|---|---|---|---|---|
| ① 收件箱有近投递 | 投递 mtime=t0+1h，评估于 m+120s | green，silent=120.0s，since=最新投递 mtime，阈 240 | 同左（message 逐字节同） | ✅ |
| ② 收件箱空但刚起跑 | 单次评估=首跑 | green，silent=0.0s，锚点来源=first_run_anchor（**不冤枉刚部署**） | 同左 | ✅ |
| ③ 收件箱空且锚点已久 | 首跑 t0，251min 后再评估 | **amber**，silent=15060.0s 可复算，阈 240，alarm_code=trigger-inbox-silent | 同左（message 逐字节同） | ✅ |
| ④a 注册表缺 liveness 段 | 239min 静默 | green，**threshold_min=240（显式 None 判回落）** | 同左 | ✅ |
| ④b 注册表阈值=0 | 静默 2s | **amber**，threshold_min=0（**0 是合法阈值，没被当缺省吞**） | 同左 | ✅ |

补充压线（`scenario4` 测试）：silent 恰等于 阈×60（240min 整）⇒ **green**
（严格 `>`，与 PoC-D `silent_s > thr_s` 同款）；241min ⇒ amber。

**Rust↔Python 对拍方法**（`align_check.py`，结果 `align-result.txt`）：
外层工作区 `tools/event-router.py` 本机不在位，对齐正本取**仓内版本控制
镜像** `poc/poc-super-agent/tests/event-router-poCD.diff`——harness 从 diff
**逐字提取** `_trigger_liveness_andon`（sha256=
`b821014c34a1df8a68af95bd2d4ceee3b43fc592b7db4eb2addcce64c835cc46`，防手抄
走样），配齐其模块级依赖（伪 `time` 钉死评估时刻）后原样执行；Rust 腿用
`examples/router_liveness_demo` 对**同一 fixtures**（同 registry.json、同
inbox 文件、同 `os.utime` 钉死的 mtime、同调用时刻序列）跑同样两轮。
对拍 55 项：**判据字段 50 项全部逐字一致**（status / alarm_code /
threshold_min / since_epoch / newest_delivery_age_s / silent_s 浮点 /
message 逐字节 / anchor_source / 快照 ts / 快照 ts_epoch），`tool` 字段
5 项为**登记的设计差**（两腿各自署名：`event-router/trigger-leg` vs
`af-order/router-liveness`——字段语义=写者标识，互覆时可归因）。

### 登记的口径差（判据字段之外，如实列出）

| 面 | PoC-D（Python） | Rust 腿 | 裁决 |
|---|---|---|---|
| 阈值非整数（`"240"`/`240.5`/`true`） | `float()` 强转出读数 | `LivenessError::Registry` fail-closed 拒 | 整数阈值（正本形态）两腿逐字一致；非整数=坏形，**拒比猜安全** |
| `liveness` 段非对象（如 `0`/`""`） | `reg.get("liveness") or {}` 按缺席回落 240 | serde 反序列化失败 → Registry Err | 同上，坏形 fail-closed（正本形态不受影响） |
| 锚点载体 | 进程内 `started_at`（无常驻即无锚点文件） | `<triggers-home>/liveness-anchor.json`（无进程态） | 语义同款（首跑写下/复用不刷新），载体不同是 Rust 腿无进程态的必然 |
| alarms.log 摘要行（跃迁必写+6h 刷新） | 有 | 本批不做 | PoC-D 侧升级面归 `状态观测/alarms.log`（外层）；af 腿快照已含 alarm_code/message，摘要行属聚合接线批裁决 |

## 三、起算锚点生命周期（`<triggers-home>/liveness-anchor.json`）

- **首跑**（文件缺席）：锚点=当次 now，tmp+rename 原子写；读数
  `anchor_created=true`。
- **后续跑**：复用锚点，**永不刷新**——静默时长单调累积（场景③证据：
  锚点内容始终 `{"first_run_epoch_s":1700000000.0}`，251min 后读数
  since_epoch 仍= t0）。
- **损坏**（非 JSON / 缺字段 / 负值 / 不可读）：**fail-closed**
  `LivenessError::Anchor`，不产读数、不改写坏文件。**选择理由**：静默重建
  锚点会把静默时钟清零，重造本安灯要治的 268h 盲窗（删锚点=再盲 240min
  才 amber）；与 `router::load_consumed` 坏行 fail-closed 同纪律（锚点是
  起算真源，真源损坏不容糊弄）。**处置**：人工核查后显式删锚点=知情重置
  （删后下一跑从当次 now 重新起算，测试 `anchor_lifecycle_corrupt_is_fail_closed`
  全覆盖）。
- 锚点 per-leg：Rust 腿锚点独立于 PoC-D 腿进程 `started_at`——两腿各自从
  各自首跑起算（同语义，非共享锚）。

## 四、只读性对拍结论

- **库层**（`liveness_leg_writes_only_the_anchor_file`）：评估+快照落盘
  前后全目录清单对拍——triggers home 内**唯一新增文件=锚点**；快照落
  home 外的调用方路径；**不写 consumed.jsonl、不派单、不造账本**。
- **demo 层**（`out/readonly-proof/readonly-diff.txt`）：两轮评估后新增
  仅锚点+快照两件；home 内只有 `inbox/ registry.json liveness-anchor.json`。
- amber 是读数不是失败：判定函数对 amber 正常返回（不 Err、不 panic），
  **不引入静默即失败**——安灯是观测不是门。

## 五、门读数（五门）

| 门 | 读数 |
|---|---|
| 1 `cargo fmt --check --all` | exit 0（改前基线亦 exit 0；rustfmt 只归一了本批新增文件） |
| 2 `cargo test --workspace --no-fail-fast` | 见 `gates.txt`（三元组） |
| 3 clippy + `ci/clippy_gate.py` | error 级 0 条；gate PASS（见 `gates.txt`；先 touch 全部源文件防缓存平凡绿） |
| 4 判据面零弱化 | `ci/clippy-baseline.txt` sha256 与改前一致 `a48639e7…`；crate 集合 22 个零新增；既有 `af-order/tests/*.rs` 零改动（git status 为证） |
| 5 本批判据定点复算 | 单测 5 + 集成 8 全绿；Rust↔Python 对拍 50/50 判据字段逐字一致（`align-result.txt`）；锚点生命周期/损坏行为有测试；只读性两层对拍在案 |

## 六、聚合面接线建议（本批不做，登记给外层接线批）

**现状**：`状态观测/latest.json`（factory-watch 写的全量快照）不含
trigger-inbox-liveness 面；obs-patrol / factory-keepalive 不读它——安灯
响了没有机器可读聚合面抬起来（268h 静默的结构性原因）。

**建议接线点**（`tools/` 属外层工作区，本批零改动）：

1. **判据文件位**：factory-watch 侧聚合读取两份判据文件——
   - PoC-D 腿：`状态观测/trigger-inbox-liveness.json`（既有，勿动）；
   - **af 腿（本批新增）**：建议落
     `状态观测/trigger-inbox-liveness.af-order.json`（**不同名**，见下）
     或接线批自定位；两份字段同构（10 字段同序同义，`tool` 可区分写腿）。
2. **`latest.json` 字段位**：建议加一段
   `"trigger_inbox_liveness": {"status": "green|amber", "alarm_code": …,
   "newest_delivery_age_s": …, "threshold_min": …, "since_epoch": …,
   "legs": {"event-router": …, "af-order": …}}`——聚合判据=**worst-of**
   （任一腿 amber 即整体 amber；两腿互为对账，读数分歧本身就该报警）。
3. **需要的判据**：任一腿判据文件**缺席/坏 JSON/超龄**（如 ts_epoch 距
   now 超过轮询周期×N）⇒ 聚合面按 **amber（stale）** 处理，不得按 green
   （fail-closed：安灯面自己失联也是断供形态——治的正是"没报活"）。
4. **两腿互覆裁决**（任务书点名核查）：若两腿写**同一文件名**，每轮
   重写=互相覆盖（后写者胜，`tool` 字段会来回翻转）——**不会坏数据**
   （字段同构、判据一致），但会抹掉"哪条腿在报活"的证据且轮次交错时
   ts 抖动。**裁决建议**：**各写各名**（上述 ② 差异化文件名），聚合面
   读两份取 worst；不改 PoC-D 侧（本批纪律），由接线批落。
5. **af 腿快照的生成方**：由聚合接线批以
   `af_order::router_liveness::evaluate_liveness` + `write_snapshot` 落盘
   （调用形态见 `crates/af-order/examples/router_liveness_demo.rs`）；
   若需常驻轮询，可在接线批给 af-order 加 CLI 子命令（本批未加，判据面
   已备，见 §七-3）。

## 七、如实登记的边界

1. **`tools/event-router.py` 与 `状态观测/` 本机不在位**（外层工作区未
   同步到本机）：对齐复算的正本取**仓内版本控制镜像**
   `poc/poc-super-agent/tests/event-router-poCD.diff`（含
   `_trigger_liveness_andon` 全文，harness 逐字提取并记 sha256）。任务书
   所引"当刻 amber 17.9h"现况读数无法在本机复核，未复核即未声称。
2. **`tools/` 与 `状态观测/` 零改动**（纪律）：聚合面接线（§六）留给
   外层接线批；本批 af 腿快照落**本工作树内**证据目录，未碰在役状态面。
3. **未加 CLI 子命令**：af-order CLI 是下单门面（四个子命令，契约钉死）；
   消费腿 router 本就是库面（由测试/接线方调用），安灯读数腿同构处理。
   任务书 §② 允许"读函数+调用方落盘"形态；demo example 即调用示范。
4. **阈值非整数坏形的口径差**（§二表）：两腿对正本整数形态逐字一致，
   坏形 Rust 拒/Python 强转——已登记，不追齐 Python 松面。
5. ** alarms.log 摘要行未在 af 腿实现**（§二表）：属外层 `状态观测/`
   升级面，留给接线批裁决。

## 八、复现

```bash
export PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=8 LANG=C.UTF-8
cargo test -p af-order --lib router_liveness                    # 单测 5
cargo test -p af-order --test router_liveness_scenarios         # 四场景集成 8
cargo build --example router_liveness_demo
python3 -I poc/poc-risk7-liveness/align_check.py "$PWD" \
  "$CARGO_TARGET_DIR/debug/examples/router_liveness_demo" \
  poc/poc-risk7-liveness/out                                     # Rust↔Python 对拍
```

# af-order RESULT — 任务包 P2-F02 下单门面 CLI 骨架

> 身份：第二阶段下单门面实现代理（TDD）｜工作树：`/home/pan-ding/workspace/af-wt-phase2-order`（分支 `phase2-order`，基于 P2-F01）
> 日期：2026-10-24｜依据：ADR-002（crate 划分：af-order=下单门面 CLI）/ ADR-003（订单持久化：纯 CLI+状态文件+引擎子进程）/ ADR-007（TDD）/ ADR-009（资源纪律）

## 1. 做了什么

新建 crate `crates/af-order`（纯离线，无 docker / LLM / 网络），三个子命令：

```text
af-order submit --spec <order.json> --state <dir>   # 一命令走通全生命周期
af-order list   --state <dir>                        # 全部订单及终态（JSONL）
af-order show   <order-id> --state <dir>             # 单订单 ledger 摘要 + MES 事件
```

`submit` 读极简 OrderSpec（`order_id` / `line{line_id,line_version}` / `budget` /
`note` 可选），编排：构造订单 → bind_line → **签发预算凭证**（真 ed25519，种子
`/dev/urandom` 取熵、落订单目录 0600、签发即自验 verify_strict）→ attach_voucher
→ release → start → 执行桩单元 → attach_artifact → complete → close。
生命周期驱动、ledger/MES 落盘、执行桩**全部复用 af-execline**
（`OrderLifecycleDriver` / `EngineClient` / `OrderLedger` / `MesFileWriter` /
`StubExecuteUnit`）；ed25519 原语复用 af-workorder。未改动任何既有 crate 源码。

状态目录布局（`order_id` 经 spec 校验为安全路径成分）：

```text
<state>/orders/<order_id>/ledger.jsonl        # af-execline 账本（序号连续+整单快照）
<state>/orders/<order_id>/mes-events.jsonl    # release/start/complete/close 四类跃迁
<state>/orders/<order_id>/artifacts/fa-<id>.json
<state>/orders/<order_id>/voucher.json        # 凭证材料（payload+签名，可独立验签）
<state>/orders/<order_id>/voucher-key.seed    # ed25519 种子（0600）
```

| 模块 | 职责 |
|---|---|
| `spec.rs` | 拒绝式解析：缺字段点名 / 未知字段全列（字典序）/ 类型预检 / order_id 路径安全 / budget>0 / line 子字段严格 |
| `voucher.rs` | 凭证签发与读回：种子 create_new+0600+fsync；payload 域分离+长度前缀；重复 submit 读回原凭证不重签；盘上凭证 order_id/amount 不符 = OR2003（预算不可经重放偷改） |
| `state.rs` | 状态目录布局 + list/show 数据源；读回只信盘上 ledger，损坏（EX2xxx）透传 OR2005 不静默跳过 |
| `cli.rs` | 参数解析（纯 std，不引 clap）+ 三子命令编排；`--workspace`/`--target-dir` 优先级：显式参数 > env（`AF_WORKSPACE_ROOT`/`CARGO_TARGET_DIR`）> 默认 |
| `error.rs` | OR1001（用法，退出码 2）/ OR2001 spec 读 / OR2002 spec 拒 / OR2003 凭证 / OR2004 订单不存在 / OR2005 状态读；af-execline EX 码原样透传 |

git 序列（先红后绿再重构，ADR-007）：

| 提交 | 性质 | 当时测试状态 |
|---|---|---|
| `9623567` test(af-order): RED 下单门面 CLI 验收测试 | 红 | lib 0 passed / 6 failed；integration 0 passed / 4 failed（`--no-fail-fast` 实测） |
| `f520013` feat(af-order): 最小实现转绿 | 绿 | lib 13 + integration 4 全过；clippy 0 警告 |
| 见 git log（refactor 笔） | 重构 + 文档 | lib 14 + integration 4 全过 |

## 2. 测试清单（验收标准可执行）

验收命令（任务书原文）：

```
cd /home/pan-ding/workspace/af-wt-phase2-order && CARGO_TARGET_DIR=~/afbuild-phase2 cargo test -p af-order
```

结果：**全绿**（lib 14 passed + integration 4 passed + doc 0）。
`cargo fmt --check --all` 零 diff；`cargo clippy -p af-order --all-targets` 0 警告。

集成测试（`tests/cli.rs`，4 例，std::process 调构建出的真二进制
`CARGO_BIN_EXE_af-order`，覆盖任务书四类验收）：

1. `submit_full_lifecycle_lands_ledger_mes_voucher_and_artifact`——一命令走通全
   生命周期：stdout JSON（state=Closed、replayed=false、stable_id 与
   `StableId::generate` 同规则、engine_bin 指向真 af-engine 子进程）；ledger 八
   op 全成功、close 唯一、序号连续；MES 恰四类跃迁、work_order_id=stable_id；
   工件文件落盘；**凭证 ed25519 在测试内独立验签**（公钥从 key_id 重构、
   verify 通过）；种子权限恰 0600。
2. `list_and_show_read_back_orders`——空状态 list 空清单不报错；两单提交后
   list 两行（按 order_id 排序，state/ledger_ops 正确）；show 输出 ops 首为
   create_order 末为 close、全 ok、MES 四类；查无此单：非零退出 + stderr 点名
   订单号与 OR2004。
3. `resubmit_same_order_id_is_idempotent`——同 spec 二次 submit：退出 0、
   replayed=true、终态 Closed 不变；close 成功记录仍唯一（**不产生第二份终态**）；
   replay_skip 审计行在账；MES 仍 4 条不重复；voucher.json 逐字节不变（不重签）。
4. `bad_spec_is_rejected_with_reason`——六类坏 spec（缺 budget / 未知字段 /
   `../escape` order_id / budget=0 / line 子字段空 / 坏 JSON）全部非零退出且
   stderr 含 OR2002 + 点名原因；坏 spec 不创建订单目录；spec 文件不存在 =
   OR2001+路径；无子命令 / 缺 --state = 退出码 2。

库单测（14 例）：spec 解析 7 例（含 line 子字段严格）、凭证 2 例（payload 域
分离/签发落盘+读回同份+改预算被拒）、state 2 例（状态名线格式/空目录）、
cli 参数 2 例（三子命令形状与 7 类用法错误 / 构建目录优先级四级）、错误码 1 例。

另做手动端到端演示（scratch state）：submit→list→show 输出与状态树见 §5。

## 3. 与任务书 / ADR 的差距（骨架边界）

- **执行单元是桩**：复用 af-execline `StubExecuteUnit`（echo 型工件文件）。真实
  执行宿主（容器 / LLM / ssh）不在骨架范围（落地契约 v0 §1 同 P2-F01 差距）。
- **只走快乐路径 + 拒绝路径经 af-execline 已测**：approve_decision、cancel 边
  未纳入 CLI 子命令（ADR-002 提到的 approve 留后续任务包）。
- **凭证"签名"与 BudgetVoucher 线格式的耦合是布尔的**：af-workorder
  `BudgetVoucher.signed: bool`，ed25519 材料存 `voucher.json`（旁证可验签），
  引擎门控只看 signed。签名入线格式需改底座语义 = 新 ADR，不在本包。
- **密钥管理是单机文件**：种子明文落 state 目录（0600），无 KMS/openBao 接入、
  无轮换。产线化需按 状态/openBao.md 的 bao-agent 路径另裁。
- **graph_version / station_id 是骨架常量**（`af-order/poc-1` / `af-order-cli`），
  spec 不接收（极简约束）；接计划编译产物（af-plan）时上提为 spec 字段。

## 4. 遗留 / 已知边界

- **`--workspace` 默认 cwd**：不传 `--workspace` 且无 `AF_WORKSPACE_ROOT` 时按
  当前目录解析；非 workspace 根下裸跑会在引擎定位时 EX1001/EX1002 失败
  （fail-loud，不静默）。
- **并发 submit 同一 order_id 未加锁**：两个进程同时首驱同一单会竞争
  ledger append（af-execline 同边界，P2-F01 遗留）。文件锁留后续。
- **list 对 `orders/` 下无 ledger 的目录静默跳过**（如手工放置的杂目录）；
  有 ledger 但损坏则 OR2005 fail-loud——跳过的只是"不是订单的目录"。
- **`voucher.json` 损坏 = OR2003 透传拒绝**（读回/解析失败映射 `OrderError::Voucher`，
  与上一行账本损坏的 OR2005 是两条稳定码：凭证面 OR2003、状态/账本面 OR2005；
  红队 W3-C-26 勘误——此处原误写为 OR2005），不重建（不猜）。
- **独立验证未做**（同 P2-F01）：测试由实现代理按任务书验收标准自写，
  尚待验证代理独立对拍。

## 5. 复现方式

```
cd /home/pan-ding/workspace/af-wt-phase2-order
CARGO_TARGET_DIR=~/afbuild-phase2 cargo test -p af-order    # 全绿

# 手动演示（scratch）：
cat > /tmp/order.json <<'EOF'
{ "order_id": "wo-demo-1", "line": {"line_id": "line-poc", "line_version": "lv-1"}, "budget": 42, "note": "骨架演示" }
EOF
CARGO_TARGET_DIR=~/afbuild-phase2 ~/afbuild-phase2/debug/af-order submit --spec /tmp/order.json --state /tmp/af-order-state --workspace $PWD
# → {"state": "Closed", "replayed": false, "ledger_ops_appended": 8, "mes_events_emitted": 4, ...}
~/afbuild-phase2/debug/af-order list   --state /tmp/af-order-state   # JSONL 一行一单
~/afbuild-phase2/debug/af-order show wo-demo-1 --state /tmp/af-order-state
```

实测样例（2026-10-24）：submit 输出 state=Closed / 8 op / 4 事件 /
engine_bin=`~/afbuild-phase2/debug/af-engine`；state 树 =
`orders/wo-demo-1/{ledger.jsonl, mes-events.jsonl, artifacts/fa-wo-demo-1.json,
voucher.json, voucher-key.seed}`，种子权限 600。

## 6. 资源纪律（ADR-009）

开工 `df -h / /tmp`：根分区余 47G（>15G 阈值）；构建只落 `~/afbuild-phase2`
（af-engine 二进制已存在，未触发重构建）；未占用主仓 af/target；未起/碰任何
容器与工厂守护进程；/tmp 只放测试 scratch 短命件（用毕即删）。

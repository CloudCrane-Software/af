# PoC：风险#5 跨库无原子事务——故障注入验收（外派验收面）

> 分支 `test/risk5-fault-injection`（**未 commit / 未 push**，diff 与读数留给主代理），
> 基线 = `origin/product/main` @ `56ff52a`。工作树
> `~/af-dev/af/wt/risk5-faultinject`（detach 自建，未动共享树分支指针）。
> 本目录自足留证：机制读准、逐场景注入读数、差距登记、四门读数。
>
> **性质**：本批是**验收面**——补偿机制（`feature/risk-cross-db-atomicity` @
> ec95411，已在 product/main）不重做、不改判据；只做独立故障注入验收 +
> 如实登记差距。生产代码**零改动**（`git status`：crates/ 下仅本批新增的
> 测试文件，ci/ 零 diff，Cargo.toml/lock 零 diff）。
>
> 新增测试：`crates/af-order/tests/adjudicate_fault_injection_acceptance.rs`
> （A1–A8 八项；既有测试文件与断言零触碰）。原始读数：`raw/`。

## 一、机制读准（从 `origin/product/main` 正本读出，非转述）

### 两相各落什么

`run_steward_loop`（`crates/af-order/src/adjudicate.rs:599`）一次闭环按序写
五张持久面，意图账三相夹在中间：

```text
① generate+persist 裁决单文书   文件  adjudications/<order>/<adj-id>.json
② station.enqueue / claim       sqlite(lease 库)  task 行（幂等键 {stable}/{adj-id}）
③ seal（内存，无落盘）
④ journal.append_prepared       文件  <adj-id>.journal.jsonl  ← prepared 相
   「我将要受理什么」：decision/decision_seq/decision_digest/
    receipt_idempotency_key/next_orders
⑤ admit → append_receipt        文件  orders/<order>/admissions.jsonl
   ★ 跨库提交点（OR2112，fsync；此前任一步失败 = 零外部效应）
⑥ settle_decision_branches      WIP 面 orders/<rw>/ledger.jsonl（create_order）
                                或 orders/<src>/ledger.jsonl（cancel）
⑦ journal.append_committed      ← committed 相：rework 清单/是否 abort
⑧ station.complete              sqlite(lease 库)  task → done
⑨ journal.append_completed      ← completed 相：全部面收敛
```

- **prepared 相落**：`PreparedIntent`（adjudicate.rs:269）——提交点**前**的
  意图，核心是 `receipt_idempotency_key = {stable}/admit/{seq}/{digest}`，
  崩溃后任何重放都按它对账「这笔提交到底落没落」；
- **committed 相落**：`CommittedFact`（adjudicate.rs:288）——提交点**后**、
  complete 前的分支结算事实（rework_order_ids / aborted）；
- 意图账是 append-only JSONL、fsync 追加（adjudicate.rs:357），**不是新库**
  （落在既有 adjudications 目录）。

### 两相之间「只有一边」的中间态在哪

| 窗 | 意图账 | 受理账 | WIP 面 | lease 库 | 后果 |
|---|---|---|---|---|---|
| W0 | prepared | **无** | 无 | claimed | 提交点前崩溃；无回执即无双写面 |
| W1 | prepared | **有** | 无 | claimed | 提交点后、结算前；含 **W1.5 部分结算**（WIP 落一半） |
| W2 | committed | 有 | 有 | **claimed** | 改前双写窗（poc-cross-db-atomicity/before 实证 seq1/seq2 两笔） |
| W3 | committed | 有 | 有 | **done** | 收口线缺；reconcile 补 |

消费两面：**重放幂等臂**（认领时 `committed_receipt` adjudicate.rs:763 判
「已过提交点」→ 不重签不重受理，按账收敛返回原回执）与 **reconcile 扫描**
（`reconcile_adjudications` adjudicate.rs:1064：PreparedResidue 留检 /
分支幂等结算 / committed 线补缺 / completed 线收口）。

### 本改动实际触及哪些库

| 面 | 库 | 本链动作 |
|---|---|---|
| 意图账（新增） | 文件 `*.journal.jsonl` | prepared/committed/completed 三相 |
| 订单受理账 | 文件 `admissions.jsonl` | admit 落 receipt（读+写） |
| WIP 面 | 文件 `orders/*/ledger.jsonl` | cancel 行 / rework create_order 行 |
| lease 库 | af-station sqlite | enqueue/claim/complete/get |
| 裁决文书 | 文件 `*.json` | 生成+落盘 |
| **Run 库** | af-harness 三 sqlite + MES | **不在该写链**（admit 的引擎子进程只做四层校验零状态写；Run 面自有 IntentKey 重放 + epoch fencing，施工面边界①已登记） |

## 二、逐场景注入读数（A1–A8，8/8 断言通过）

注入纪律与既有套件同款：**零生产注入钩子**——测试镜像 `run_steward_loop`
首走的真实 pub 调用序列在指定相位停机（≡ 进程级 SIGKILL 的持久面后果，
所有写入 fsync 后才推进）；时钟用 `ManualClock` 推租约（不睡眠）；引擎走真
子进程（`EngineClient::locate`），队列走真 af-station。断言全部从盘上/库上
重载复算。原始逐行读数：`raw/acceptance-run.log`。

| 场景 | 注入（前→后） | 补偿读数（补偿后） | 判定 |
|---|---|---|---|
| **A1** W1.5 Revise 部分结算 | journal=[prepared]；rw1 create_order=1、**rw2 缺**；receipts=1；task claimed | reconcile①：幂等补齐（rw1 仍 1、rw2 补 1、committed=[rw1,rw2]）；reconcile②再跑：**零新写入**（journal 行数不变）；重放：task done、journal=[p,c,c]、receipts 恒 1 | **收敛 ✓ 幂等 ✓** |
| **A2** W1.5' 半程 abort | journal=[prepared]；cancel 行=1；receipts=1 | reconcile①：已 Cancelled 即收敛（cancel 恒 1）+ committed(aborted=true)；②再跑零新写入；重放：done、journal=[p,c,c]、receipts 恒 1、cancel 恒 1 | **收敛 ✓ 幂等 ✓** |
| **A3** Closed+Reject（G-1） | 首走即 `Err(MissingEdge)`；**receipts=1（提交点已过）**、cancel=0、journal(adj-1)=[prepared] | reconcile → `Err(MissingEdge)`；抢先重试 → 认领 adj-2 再真受理 **receipts 1→2** 再撞墙；到期重试 → 重放臂仍 `Err`，journal 恒 [prepared]，任务永打转 | **悬空 ✗ → G-1** |
| **A4** 毒账阻断扫描（G-2） | 毒账（A3 形态）+ 健康账（W3：task done、缺收口线） | reconcile → 整体 `Err(MissingEdge)`；健康账 journal 停 [prepared, committed]，**completed 没补** | **阻断 ✗ → G-2** |
| **A5** 补偿调用副作用（G-3） | W2 崩溃（journal(adj-1)=[p,c]、receipts=1） | 补偿①：重放收敛 adj-1（receipts=1、done、[p,c,c]）但 **adj-2 残留 Queued**（无意图账）；补偿②：认领 adj-2 首走**真受理** receipts=2（seq=[1,2]） | 重放臂幂等 ✓ / **调用面 ✗ → G-3** |
| **A6** lease 库漂移（G-4） | W1 崩溃 + 换新 station（任务缺位） | reconcile：journal/WIP 面仍收敛到 [prepared, committed]（settle 在缺位检查**前**），任务面 `PreparedResidue(缺位)` 留检；重试只能开新轮次（receipts=2）；旧账每轮留检永不收口 | **悬空 ✗ → G-4** |
| **A7** 受理账漂移（G-5） | W2 崩溃后抹 `admissions.jsonl`：journal=[p,c]、receipts=0 | reconcile → `PreparedResidue`（措辞「admit 未提交或被拒」，与 committed 线在账矛盾）；重放臂 → `Err("…跨库账面漂移，拒绝重放，留检")`，任务永久 Claimed | **悬空 ✗（fail-loud 诚实）→ G-5** |
| **A8** 注入点返回错误 | Reviser 签 Approve → admit `OR2106` 拒；**受理账零写入**、journal=[prepared] | reconcile → PreparedResidue（无回执不可能双写）；推过 60s 租约后正权角色重试 → 认领**旧任务** adj-1 首走真受理收敛：receipts 恒 1、done、journal=[p,p,c,c]（重试第二条 prepared，历史不抹） | **收敛 ✓ 恒 1 笔 ✓** |

初跑红过两处（6/8），均为**夹具预测偏差而非机制改判**，已按实测修正并在
断言旁注明：A6——reconcile 在任务缺位时仍先把 journal 面收敛到 committed
（我原断言停在 prepared）；A8——首走拒后任务持 60s 租约，推 2s 不足以回炉
（改为推 61s）。修正后 8/8 绿，`raw/acceptance-run.log` 为终跑。

## 三、结论：主干验收通过 + 5 条差距如实登记

**通过面**（判据「中间态收敛到 prepared-未-committed 或 completed 之一」
「补偿幂等不二次损坏」在以下形态成立）：

1. 四个主崩溃窗 W0/W1/W2/W3（既有 6 项复核全绿，含改前双写窗 W2 重放后
   受理账恒 1 笔）；
2. **W1.5 部分结算**（Revise 多 rework 落一半 / Reject cancel 行已落）——
   reconcile 幂等补齐 + 双跑零新写入 + 重放收敛（A1/A2，既有套件未覆盖）；
3. **admit 被拒形态**（OR2106 越权）——prepared 残留可检、无回执零双写、
   正权重试收敛恒 1 笔（A8）。

**差距面**（`GAPS.md` 逐条：缺口形态/触发条件/当前行为/修法方向）：

| # | 形态 | 严重度 | 一句话 |
|---|---|---|---|
| G-1 | Closed 源单+Reject：receipt 已落（提交点已过）但 abort 无边，prepared 永悬；抢先重试每错一次**多一笔不可结算受理** | 高 | settle 前移预检 / 留档语义收口 / reconcile 独立变体 |
| G-2 | 单份毒账令 reconcile 整体 Err，**健康账收口被阻断** | 中 | 单账级报告，扫描不中断 |
| G-3 | 补偿调用自带新裁决单入队；二次补偿=真受理第二笔（「补偿跑两次=跑一次」在调用面不成立） | 中 | 入队前去重 / 恢复专用入口 |
| G-4 | lease 库漂移：journal 面可收敛、任务面/收口线永不收敛（每轮留检） | 中 | 运营者收敛工具 / 留检变体细分 |
| G-5 | 受理账漂移双判读不对称（reconcile 误导性 PreparedResidue vs 重放臂 fail-loud），任务永久悬空 | 低 | 两面同词 fail-loud |

G-1/G-2/G-5 **不在**施工面自己的边界登记（poc-cross-db-atomicity §六）内；
G-3 施工面明知并标 by-design（验收判据下仍记差距）；G-4 与其「不代认领」
边界相邻但形态不同（任务**缺位**而非在队）。

## 四、门读数与边界

| 门 | 命令 | exit | 读数 |
|---|---|---|---|
| 1 | `cargo fmt --check --all` | **0** | 干净（首跑红块全部在本次新增测试文件 :36/:207/:291，仅对该文件 rustfmt 归零，`raw/gate1-fmt.log` 留痕） |
| 2 | `cargo test --workspace --no-fail-fast` | **0** | **基线 1578/0/9 → 本批 1586/0/9**（+8 = 新增 A1–A8，只增不减；`raw/gate2-{baseline,final}-test.log`） |
| 3 | `cargo clippy --workspace --all-targets --message-format=json`（先 touch 全部 .rs 全量重 lint）→ `python3 -I ci/clippy_gate.py <jsonl>` | **0 / PASS** | error 级 0 条、warning 0 条（CLIPPY GATE: PASS，基线 0 签名零动）；首跑曾红 = 本批新文件内 `clippy::err_expect`×8 + `clippy::too_many_arguments`×2，改 `Result::expect_err` + `WalkCtx` 收参后复跑 PASS；`raw/gate3-clippy.log`（jsonl 同目录） |
| 4 | 判据面零弱化 | **0** | `git diff origin/product/main -- ci/` 0 行；`clippy-baseline.txt` sha256 `a48639e7…` 与钉一致；Cargo.toml/lock 0 diff；crates/ 零新增 crate（唯一改动 = 新增测试文件）；`raw/gate4-zero-weakening.log` |

**如实登记的边界**：

1. 注入形态 = 调用间停机 + 人为库面漂移（A6 换 station / A7 抹受理账），
   未做进程级 SIGKILL/断电注入——全部持久写 fsync 后才推进，与施工面
   边界③同款理由（停机 ≡ SIGKILL 的持久面后果）；
1b. 门 1/门 3 首跑各红过一次，红源**全部在本次新增的测试文件内**（fmt
   折行三块 :36/:207/:291；clippy 新签名 10 条）——均只对该新文件修复
   （rustfmt / `expect_err` / `WalkCtx` 收参），既有件与 ci/ 基线零触碰；
2. 多 steward 并发认领未注入（af-station 守卫 UPDATE 既有并发测试覆盖，
   施工面边界④同款）；
3. A3/A4 的「抢先重试」形态以 ManualClock 精确编排（租约未到期 vs 到期），
   真实时钟下等价于运维在租约窗内反复重试；
4. 本批读数均**串行**跑（施工面 PoC 已注记并发 cargo 会交叉重写
   af-engine 信任锚）；
5. G-1..G-5 只登记与给修法方向，**未动任何生产代码**——修复属施工面决策。

## 复现

```bash
export PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=8 LANG=C.UTF-8
export CARGO_TARGET_DIR=/data/bench/targets/risk5-faultinject
cargo test -p af-order --test adjudicate_fault_injection_acceptance -- --nocapture  # A1–A8
cargo test --workspace --no-fail-fast                                               # 门2
cargo fmt --check --all                                                             # 门1
find crates -name '*.rs' -exec touch {} + && \
  cargo clippy --workspace --all-targets --message-format=json 2>/dev/null \
  > /tmp/clippy.jsonl && python3 -I ci/clippy_gate.py /tmp/clippy.jsonl             # 门3
```

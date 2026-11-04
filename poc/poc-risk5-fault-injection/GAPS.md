# 风险#5 故障注入验收 —— 差距登记（验收面，不改生产代码）

> 外派任务书 `test/risk5-fault-injection`（基线 = `origin/product/main` @ 56ff52a）。
> 依据：`raw/acceptance-run.log`（A1–A8 注入读数，8/8 断言通过——红的两项初跑
> 已按实测修夹具预测后全绿；断言钉的是**机制当前真实行为**，不是期望行为）。
> 每条差距都给出：缺口形态 / 触发条件 / 当前行为（实测读数）/ 建议修法方向。
> **本批只登记，不动 `crates/` 生产件。**

机制施工面自己的边界登记见 `poc/poc-cross-db-atomicity/README.md` §六（五条：
覆盖面 / reconcile 不代认领 / 无进程级注入 / 多 steward 并发 / 跨 state 根）。
下面 G-1/G-2/G-5 **不在**其登记内；G-3/G-4 与其边界相邻但形态不同（逐条注明）。

---

## G-1（A3 实证，高）：Closed 源单 + Reject 决策 ⇒ 提交点后不可收敛悬空态

- **缺口形态**：受理账 `orders/<id>/admissions.jsonl` 已落回执（**跨库提交点
  已过**），意图账永停 `prepared`（`committed`/`completed` 线永缺），lease 库
  任务随租约回炉永远打转（claimed → expired → reclaimed → …）。既非「已
  prepared 未 committed」（无回执形态），也非「已完成」——**悬空态**。
- **触发条件**：源单已 **Closed**（终态）+ 签发 Decision=Reject。admit 的
  OR2102 只拒 Cancelled、明文放行 Closed（`admit.rs:447`，「Closed 单受理——
  闭环单的裁决留档正是本链的用途之一」），于是 admit 落账后
  `abort_source`（`adjudicate.rs:976`）撞状态机：Closed 无 abort 边 →
  `MissingEdge`。
- **当前行为（实测，raw/acceptance-run.log A3）**：
  - 首走（真路径）：`err = 状态机无此边: 源单 wo-acc-a3 已终态（Closed）…`；
    `receipts=1`、`cancel 行=0`、`task(adj-1)=Claimed`、`journal(adj-1)=[prepared]`；
  - 补偿① reconcile：整体 `Err(MissingEdge)`，journal 不动；
  - 补偿② **租约未到期即重试**：认领到本轮自产新单 adj-2 → 首走**再真受理
    seq=2** → 再撞 MissingEdge ⇒ `receipts 1→2`（每抢先重试一次多一笔
    **不可结算**的受理）；
  - 补偿③ 租约到期重试：重放臂认领 adj-1 → 重结算 → 仍 `MissingEdge`，
    `journal(adj-1)=[prepared]` 恒定，任务面永打转。
- **建议修法方向**（任选其一，均为施工面决策，本批不动）：
  1. **前移检查**：seal/admit 之前对（source 终态 × decision）组合预检 abort
     边存在性（Reject 需非终态源单），OR21xx 类型化拒绝在**提交点之前**——
     失败落回 W0 形态（prepared 无回执，可重试、可收敛）；
  2. **留档语义**：`settle_decision_branches` 对「终态源单 + Reject」按
     「结论仅以 receipt 留档、不改单」落 `CommittedFact{aborted:false}` 收口
     （模块文档对 MissingEdge 的措辞已接近此语义，只差把它定义为已结算）；
  3. **reconcile 兜底**：`reconcile_one` 对该形态产出独立 `ReconcileStatus`
     变体（如 `Unsettleable{detail}`）留检而非 Err，阻断面见 G-2。

## G-2（A4 实证，中）：单份毒化意图账令 reconcile 整体 Err，阻断同批健康账收敛

- **缺口形态**：`reconcile_adjudications`（`adjudicate.rs:1064`）在扫描循环里
  `reports.push(reconcile_one(...)?)`——**任意一份**意图账的收敛动作报错，
  整个扫描 Err 返回，排在后面的健康账既无报告也无收敛动作。
- **触发条件**：任一账处于 G-1 形态（或 settle 任何 Err），且同 state 根下
  还有其他在飞/待收口账。
- **当前行为（实测，A4）**：毒账（Closed+Reject）字典序在前，健康账为 W3
  形态（任务已 done、只缺 completed 收口线）。reconcile → `Err(MissingEdge)`；
  健康账 `journal=[prepared, committed]`，**completed 线没补**——若无人再跑
  reconcile（毒账不除），健康账的收口永远缺失。
- **建议修法方向**：`reconcile_one` 的 Err 降为单账级报告（error 变体入
  `ReconcileReport`，扫描继续），或至少先完成全部只读判读再对可收敛账执行
  写动作；毒账单独 fail-loud 留检。

## G-3（A5 实证，中）：补偿调用（run_steward_loop）自带「新裁决单入队」副作用，二次补偿=一笔新受理

- **缺口形态**：`run_steward_loop`（`adjudicate.rs:599`）**先**无条件
  `generate_adjudication_doc`（单号 = 已落盘份数+1）+ `enqueue`，**后**才
  `claim`。崩溃恢复场景里，每次运维重试都会在队列里留下一个无人认领的新
  裁决单（adj-N+1）；下一次补偿认领到它时走**首走真受理**（seq+1 新回执）。
- **触发条件**：同一源单上 run_steward_loop 被调用 ≥ 2 次且第一次未把队列
  清空（崩溃恢复的常态）。机制施工面在 W2 测试注释里明知此行为
  （「会生成新裁决单 adj-2 入队，但认领按 rowid 先拿到回炉的旧任务」）并在
  其 README §四标注「新轮次，by-design」——**by-design ≠ 满足验收判据**
  「补偿跑两次与跑一次结果相同」。
- **当前行为（实测，A5）**：W2 崩溃 → 补偿①重放收敛 adj-1（`receipts=1`，
  adj-2 残留 Queued、无意图账）→ 补偿②认领 adj-2 首走真受理
  （`receipts=2，seq 集合=[1,2]`）。重放臂**自身**对同一裁决单幂等 ✓；
  不幂等的是**补偿调用面**。另注：A3 补偿②显示租约未到期窗内的重试同样
  直接开新轮次（而非等回炉旧单），放大 G-1 的受理面污染。
- **建议修法方向**：入队前去重——生成新单前先扫描该源单「已入队未裁决」
  的残单并复用（或入队幂等键从 `{stable}/{adj-id}` 收紧为源单级 pending
  键）；或提供「仅恢复不新增」的恢复入口（跳过 generate/enqueue 直接认领）。

## G-4（A6 实证，中）：lease 库漂移 ⇒ journal/WIP 面可收敛、任务面与收口线永不收敛

- **缺口形态**：receipt 在账、意图账 `committed` 线已由 reconcile 补上，但
  prepared 所指 `task_id` 在 lease 库缺位 → `completed` 收口线永无落点，
  reconcile 每轮 `PreparedResidue("队列任务 … 缺位（lease 库与意图账漂移）")`。
- **触发条件**：lease 库（af-station sqlite）丢失/回滚/换实例而 state 根
  （文书/意图账/受理账/WIP 面）保留——基础设施级故障（库分库部署下真实
  存在）。与施工面边界登记第 2 条「不代认领」不同：那条是任务**在队**等认领；
  这条是任务**缺位**，连认领面都不存在。
- **当前行为（实测，A6）**：换新 station 后 reconcile → `PreparedResidue`
  （缺位明细，`journal=[prepared, committed]`——settle/committed 补缺发生在
  缺位检查之前，journal 面尽力收敛）；重试只能在新库开新轮次
  （adj-2 首走，`receipts=2`）；旧账每轮 reconcile 仍留检，永不收敛。
- **建议修法方向**：为该形态提供运营者收敛工具（按回执事实把意图账补
  `completed` 并登记 superseded，或把任务面按意图账**重建**入队后走正常
  重放臂）；至少在 `PreparedResidue` 明细里区分「admit 未提交」与「任务
  缺位」两类留检（当前共用同一变体，语义不同）。

## G-5（A7 实证，低）：受理账漂移的双判读不对称（reconcile 静默留检 vs 重放臂 fail-loud），任务永久悬空

- **缺口形态**：`committed` 线在账、受理账对应回执**缺位**（受理账在落账后
  被损坏/误删）。同一漂移形态两个补偿面给出**不同**判读：
  `committed_receipt`（`adjudicate.rs:763`，认领路径）→ fail-loud
  `Err("…跨库账面漂移，拒绝重放，留检")`；`reconcile_one`（`adjudicate.rs:1137`，
  扫描路径）→ 落入普通 `PreparedResidue`（措辞还是「admit 未提交或被拒」，
  与 committed 线在账的事实矛盾）。
- **触发条件**：`admissions.jsonl` 在 admit 落账（fsync）之后被外力损坏/删除
  （盘损或误运维；正常运行中 admit 自身原子，不会自造此态）。
- **当前行为（实测，A7）**：注入后 `journal=[prepared, committed]`、
  `receipts=0`；reconcile → `PreparedResidue`（误导性措辞）；重放臂 →
  `Err(Queue("…跨库账面漂移…"))`，任务卡 Claimed 永不收敛（fail-loud 是
  诚实的设计选择，但按判据=悬空态，且无人值守面没有对应可读的正确判读）。
- **建议修法方向**：`reconcile_one` 对「committed/completed 线在而回执缺位」
  复用与 `committed_receipt` 同一 fail-loud 判读（独立 `ReconcileStatus::
  LedgerDrift{..}` 变体），两边同词；恢复工具按意图账 committed 线与受理账
  备份对账后人工裁决。

---

## 附：验收通过面（无差距，可复跑）

| 场景 | 判据读数 | 测试 |
|---|---|---|
| W1.5 Revise 多 rework **部分物化**崩溃 | reconcile 幂等补齐（rw1 不双落、rw2 补上）、双跑零新写入、重放收敛、受理账恒 1 笔 | `acc_a1_partial_rework_settlement_converges` |
| W1.5' Reject **半程 abort** 崩溃 | reconcile 幂等结算（cancel 恒 1 行）+ committed 补缺、双跑零新写入、重放收敛 | `acc_a2_mid_abort_converges` |
| 注入点返回错误（admit OR2106 拒） | 受理账零写入、prepared 残留可检；正权角色重试按首走收敛、受理账恒 1 笔 | `acc_a8_admit_rejection_leaves_residue_then_first_walk_converges` |
| （复核）既有 W0/W1/W2/W3/Reject 臂/编号 6 项 | 基线全绿（1578/0/9 内含） | `crates/af-order/tests/adjudicate_fault_injection.rs` |

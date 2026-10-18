# RESULT.md — poc-station 人工任务账本（crates/af-station）

## ① 结论

**通过**。8/8 测试全绿（含两线程抢同一任务、4 线程压测 40 任务零重复认领、租约过期接管），并发用例连跑 10 轮无偶发失败。

## ② 做了什么

工作目录：`af/crates/af-station`（未动根 Cargo.toml，依赖只加进本 crate）。

- **表结构**（`src/lib.rs:69-83`）：`task(id TEXT PK, payload_ref, required_role, state CHECK IN ('queued','claimed','done'), claimant, lease_until(unix 毫秒), idempotency_key UNIQUE)` + `(required_role, state)` 索引。开库设 WAL + busy_timeout 5s + synchronous NORMAL，供多连接并发。
- **原子认领**（`claim`，`src/lib.rs:151-181`）：按任务书模式——先按角色查出候选 id，再执行守卫 UPDATE `SET state='claimed', claimant=?1, lease_until=?2 WHERE id=?3 AND claimant IS NULL`，用 `Connection::execute` 返回的受影响行数判定成败（1=认领成功，0=被抢先，换下一个候选重试）。SET 里除 claimant 外带上 state/lease_until，因为 schema 要求二者同步翻转。
- **租约心跳**（`heartbeat`，`src/lib.rs:184-192`）：`UPDATE task SET lease_until=? WHERE id=? AND claimant=? AND state='claimed' AND lease_until > now`——只有现任 claimant 且租约**未过期**才能续期，过期后原主人心跳失败，防止僵尸复活。
- **过期惰性回收**（`reclaim_expired`，`src/lib.rs:139-148`）：`UPDATE ... SET state='queued', claimant=NULL, lease_until=NULL WHERE state='claimed' AND lease_until < now`；在每次 `claim` 入口调用，无需后台线程，下一次查询顺手回炉，他人即可接管。
- **幂等键**（`new_task`，`src/lib.rs:110-136`）：UNIQUE 列 + `INSERT ... ON CONFLICT(idempotency_key) DO NOTHING` + 受影响行数判定；撞键返回 `Enqueued::Duplicate{existing_id}`，不产生第二行。key=NULL 不参与去重（SQLite UNIQUE 允许多个 NULL）。
- **角色路由**：`claim(worker_id, role, lease_ms)` 的候选查询强制 `required_role = role` 精确匹配，不匹配者领不到、任务保持 queued。
- **测试 8 个**（`src/lib.rs:203-373`）：入账往返、幂等去重、角色路由、心跳续期+非 claimant 不可用、租约过期接管（旧主不能心跳/完成，新主接管后完成）、**两线程（各自独立 SQLite 连接 + Barrier 同步）抢同一任务恰好一个赢**、**4 线程清空 40 任务无重复认领**、端到端生命周期。
- 并发稳定性：上述并发+租约 3 个用例 `--  two_threads_race stress_four_threads expired_lease` 连跑 10 轮，全部通过。

## ③ 证据

命令（均在 `af/` 下，`source ~/.cargo/env` 后）：

```
$ cargo test -p af-station -j 4
running 8 tests
test tests::enqueue_and_get_roundtrip ... ok
test tests::role_routes_claim_to_matching_worker_only ... ok
test tests::end_to_end_lifecycle ... ok
test tests::idempotency_key_prevents_duplicate_enqueue ... ok
test tests::two_threads_race_on_same_task_exactly_one_wins ... ok
test tests::stress_four_threads_drain_forty_tasks_without_double_claim ... ok
test tests::heartbeat_renews_lease_and_only_claimant_can_use_it ... ok
test tests::expired_lease_is_reclaimed_and_takeover_works ... ok
test result: ok. 8 passed; 0 failed; ... finished in 0.35s

$ cargo build -p af-station -j 4
    Finished `dev` profile target(s) in 0.03s
```

bundled 确认：`cargo tree -p af-station -e features -i libsqlite3-sys` 输出
`libsqlite3-sys v0.30.1 ├── feature "bundled" └── rusqlite feature "bundled" └── af-station`，
且 `target/debug/build/libsqlite3-sys-*/output` 存在（bundled C 编译产物）。

稳定性：并发/租约 3 用例 10 轮连跑，每轮 `test result: ok. 3 passed; 0 failed`。

## ④ 对最终方案的反馈

1. **接口建议**：`Station::open(path)` 一连接一 worker 是正确抽象——正确性完全不依赖进程内锁，天然支持多进程（多个人工坐席/多个 Agent 进程）。最终方案若上 WAL + busy_timeout（本 PoC 5s），SQLite 单文件扛这种账本并发足够。
2. **任务书给的认领模式是对的，但要补两处**：`UPDATE ... WHERE claimant IS NULL` 直接可用，但 (a) SET 需同步写 `state='claimed'` 和 `lease_until`，否则状态机断裂；(b) 心跳必须加 `lease_until > now` 守卫，否则过期 worker 的一次心跳就能把已被逻辑上判死的租约复活（竞态窗口：回收是惰性的，回收发生前行仍显示 claimed）。
3. **坑：幂等键用 `ON CONFLICT DO NOTHING` 时注意它对所有 UNIQUE 冲突生效**。本表 `id` 主键也是 UNIQUE，若调用方重复 id 且不同 key，会被 `id` 冲突拦下但表现为裸 Sqlite 错误而非 Duplicate。最终方案建议要么显式 `ON CONFLICT(idempotency_key)`（本 PoC 已如此，PostgreSQL 语法同源可平移），要么在错误层把主键冲突翻译成业务错误。
4. **lease_until 用 unix 毫秒 INTEGER 而非 ISO 字符串**，比较、索引、跨语言都省事；心跳以 now 为基准重置（而非旧值累加），语义更稳。
5. 时间源是本机时钟。若最终方案跨机部署账本，惰性回收的过期判定依赖各连接时钟基本同步；单机部署（当前 PoC 形态）无此问题。

## ⑤ 遗留问题

- 未做：done 后的审计查询接口（列出某 claimant 的历史）、任务优先级、任务取消/作废状态——均超任务书范围，账本表结构可平滑加列。
- 压测规模为 4 线程 × 40 任务（任务书只要求 2 线程 × 1 任务，已超额验证）；万级任务下的 `LIMIT 64` 候选窗口策略未压测。
- 回收与接管之间，过期原主有一次失败的心跳/完成调用属预期语义（返回 false），未做更细的"宽限期"。
- SQLite 单写者模型：极高写入并发下 busy_timeout 会放大延迟；PoC 规模无感，最终方案若账本写 QPS 上百需重估。

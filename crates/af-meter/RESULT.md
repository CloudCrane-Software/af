# RESULT.md — poc-meter 可组合计量器（crates/af-meter）

## ① 结论

**通过**。Meter 树（Stock / Window / Concurrency / Rate + 组合子 Min / Max）、模拟时钟评估
`given(now) → Reading{available, recovers_at}`、路由消费示例全部实现并真实跑通；
核心场景"日窗耗尽但周窗有余 → 日窗恢复前转 API、日窗重置后切回订阅"由集成测试
`daily_exhausted_but_weekly_surplus_switches_to_api_until_daily_resets` 验证通过。

## ② 做了什么

文件（全部在 `crates/af-meter/` 内，未动 workspace 根与其他 crate）：

- `src/lib.rs` — 计量模型：
  - 叶子：`Stock{remaining}`（一次性额度池）、`Window{window,capacity,remaining,reset_at}`
    （固定窗口，错过多窗不累积）、`Concurrency{limit,in_use,releases}`（占用带释放时刻，`acquire/release`）、
    `Rate{per_second,burst,tokens,last}`（令牌桶，派生计量，按时间连续恢复）；
  - 组合子：`Min(Vec<Meter>)` = 约束合取（消耗时**每个子计量各扣一份**）；
    `Max(Vec<Meter>)` = 择优/故障转移（**只扣选中的子计量**，不聚合）；
  - 评估：`refresh(now)` 推进模拟时钟（重置/释放/回填，幂等），`given(now) -> Reading{available, recovers_at}`；
    `try_consume(need, now)`、`acquire(now, release_at)`、`MeterError::Insufficient{need,available,recovers_at}`；
  - 恢复时刻语义：`recovers_at` = 可用量下次**严格增加**的时刻；`Min` = 绑定子计量（available==min）
    恢复时刻的**最大值**，任一绑定者永不恢复（如空 Stock）则整体 `None`；`Max` = 各子计量恢复时刻的最小值（下界）。
- `src/demo.rs` + `examples/routing.rs` — 路由消费示例：
  订阅 = `Min(周窗1000, 日窗200)`，API = `Stock 10000`；`route(&mut [Supply], need, now)`
  按任务预计消耗、优先级序选第一路能覆盖的供给并扣减，都不够则 `Defer{recovers_at}`。
- `tests/routing.rs` — 场景集成测试 5 个；`src/lib.rs` 内单元测试 10 个（含 serde 往返）。
- 依赖只加在本 crate：serde(derive)/serde_json/thiserror。

## ③ 证据

命令（均在本机实际执行，`source ~/.cargo/env` 后）：

1. `cargo build -j 4 -p af-meter` → `Finished \`dev\` profile ... in 0.40s` ✅
2. `cargo test -j 4 -p af-meter` → **10 passed（单元）+ 5 passed（集成）+ 0 failed** ✅，关键测试名：
   `daily_exhausted_but_weekly_surplus_switches_to_api_until_daily_resets ... ok`、
   `nested_weekly_window_binds_and_recovers_at_weekly_reset ... ok`（周窗绑定，恢复=周重置而非日重置）、
   `rate_token_bucket_refills_continuously ... ok`、`concurrency_acquire_and_release ... ok`、
   `max_deducts_only_chosen_child ... ok`、`serde_roundtrip_nested_meter ... ok`。
3. `cargo run -j 4 -p af-meter --example routing`（节选 6/8 行）：

```
d0 01:00:00   80  subscription  subscription: 0 [min[window(800/1000)], min[window(0/200)]] | api: 10000
d0 02:00:00    5  api           subscription: 0 [min[window(800/1000)], min[window(0/200)]] | api: 9995
d0 22:13:20    7  api           subscription: 0 [min[window(800/1000)], min[window(0/200)]] | api: 9988
d1 01:00:00  150  subscription  subscription: 50 [min[window(650/1000)], min[window(50/200)]] | api: 9988
d1 02:23:20  300  api           subscription: 50 [min[window(650/1000)], min[window(50/200)]] | api: 9688
d7 00:00:00  150  subscription  subscription: 50 [min[window(850/1000)], min[window(50/200)]] | api: 9438
```

逐行即剧情：d0 01:00 日窗耗尽但**周窗仍有 800**（周窗有余）；d0 02:00 起日窗恢复前任务全部**转 API**；
d1 00:00 日窗重置，d1 01:00 **切回订阅**；need=300 超日窗上限 200 → 直接走 API（按预计消耗选供给）；
d7 00:00 周窗重置恢复订阅。集成测试 `golden_timeline_over_schedule` 对整条时间线 + API 余量 9438 + 周窗 850 做了断言。

## ④ 对最终方案的反馈

1. **Min/Max 语义不对称必须在接口文档里写死**：Min=约束合取（各扣一份，`周窗套日窗`天然成立）；
   Max=择优（只扣一份、不聚合）。实现时最容易理解错的就是这里。
2. **`recovers_at` 的"可用量下次严格增加"语义 + Min 的绑定子计量取 max 规则**是好用的原语：
   路由只需 `available` 和 `recovers_at` 两个字段就能表达"用/等/换"，建议正式 API 保留这对字段，
   并把语义写进类型文档。
3. **Max 的 `recovers_at` 目前是下界**（各子计量恢复时刻的最小值，不保证届时 max 真的增长）。
   若正式方案需要精确，建议 `Reading` 增加 `recovers_to: Option<Amount>`（恢复后能到多少）。
4. **计量数值类型建议从 f64 换成整数最小单位**（调用数 u64、金额 i64 分）：PoC 用 f64 是为了
   令牌桶的连续回填方便，但对钱不安全；正式方案可用 u64 + 纳秒时间戳，Rate 的部分回填取整策略需定义。
5. **Concurrency 与数量计量分属两个操作**（acquire/release vs try_consume），语义不同（占坑 vs 扣量），
   正式方案建议拆成两个 trait 或统一为 Reservation API，不要硬塞进一个 `consume`。
6. **Meter 枚举 + serde 可直接当配置格式**（示例第一段就是 JSON 表达式），建议正式方案给它加版本号字段；
   窗口建议支持"对齐自然日/自然周 + 时区"（PoC 是从创建时刻滚动的相对窗口）。
7. `route()` 是优先级贪心 + 覆盖判断，已够本场景；若要支持"任务拆分/部分执行/大任务预留"，
   需要在路由层新增组合逻辑，计量树本身不用改。

## ⑤ 遗留问题

- 多消费者并发安全未做：`Meter` 是 `&mut self` 状态机，假设单消费者；真实系统要加锁或 Actor 化。
- 无持久化：状态在内存，重启即丢；正式方案要落库（如 rusqlite bundled）并考虑"重启后按 now 重放 refresh"。
- 固定窗口在峰值边界有双倍突发问题（业界通病），滑动窗口/滑动日志未实现。
- Max 恢复时刻为下界（见④-3）；Min 的恢复时刻在"绑定子计量恢复后仍不足 need"时只给出"可用量增长点"，
  不是"达到 need 的时刻"（需要 `time_when_available(need)` 才精确，PoC 未做）。
- `route` 未考虑部分执行与任务排队，`Defer` 只给了恢复时刻，没有回调/唤醒机制。
- 未做 property 测试（时间倒流、非法参数如 window<=0 只做了防御性 return）。

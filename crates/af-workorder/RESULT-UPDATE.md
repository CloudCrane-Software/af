# RESULT-UPDATE.md — T4 af-workorder 接 af-calendar

## ① 结论

**完成**。`Sla.deadline` 从 `Option<String>` 一次性切换为 `Option<DateTime<Utc>>`。截止由注入的 `af_calendar::Calendar::sla_deadline` 解析后归一到 UTC,订单逾期判定(`Sla::is_overdue` / `WorkOrder::is_overdue`)只比较这个绝对时刻。

`cargo test -p af-workorder` 全绿:36 单测 + 6 集成测试(原 34 单测 + 3 集成测试都在,另加 2 单测、3 集成测试)。本 crate `clippy --all-targets --no-deps -D warnings` 零警告。

字面命令 `cargo clippy --all-targets -p af-workorder -- -D warnings` **退出码 101**,但失败点不在本 crate:clippy 默认连 path 依赖一起 lint,打到 `af-calendar` 里既有的 `clippy::needless_range_loop`(`calendar.rs:114`,已在 `ci/clippy-baseline.txt`)。本任务不许改 af-calendar,所以没有去修它。详见证据与遗留。

## ② 步骤

1. 读了 `crates/af-workorder/RESULT.md` 与 `crates/af-calendar/RESULT.md`。沿用已验证语义:日历只答时间;工时按流逝时间;春跳间隙不计工时;秋拨重复小时只计一次;歧义取较早偏移;恰在 deadline 不算逾期。
2. **一次性切换,不留 deprecated 字符串字段**。理由:
   - `deadline` 这个槽位就是"完成时限"。旧的 `Option<String>` 是 PoC 占位(workorder RESULT ⑤ 写明最终方案接时间类型或由 af-calendar 出时间),不是需要并存的第二份数据。
   - chrono 0.4 的 `DateTime<Utc>` serde 线格式就是 RFC3339(UTC 用 `Z`)。旧 JSON `"deadline":"2026-10-20T00:00:00Z"` 能直接读入;带非零偏移的文本折成同一绝对时刻(单测锁了 `+08:00` → UTC)。
   - 再留一个字符串字段会有两个源,和"内部判定用 `DateTime<Utc>`"冲突。
   - 全部调用点都在本 crate,已一次改完。
3. 依赖:`af-calendar`(path);`chrono` 0.4 feature `serde`(锁定 **0.4.45**,与 af-calendar 相同)。必须直接依赖 chrono,否则本 crate 无法在公开签名里写出 `DateTime<Utc>`(传递依赖不能 `use`)。提交 `549cb42`。
4. 解析入口 `Sla::from_calendar(calendar, placed_at, promised_working, priority)`:输入时区原样交给日历,返回前 `with_timezone(Utc)`。日历不嵌进 `WorkOrder`(日历不是 `Eq`,每个订单复制一份配置也没有判定用途)。判定用订单上已经写死的绝对时刻,状态迁移不改 deadline。负时长原样返回 `CalendarError::NegativeDuration`。
5. 集成测试 `tests/calendar_sla.rs`(提交 `e7a19c5`)。dev-dependency `chrono-tz` 0.10(锁定 **0.10.4**,与 af-calendar 相同):`Calendar::weekday_single_shift` 的时区参数是 `chrono_tz::Tz`,DST 用例要构造 `Europe/Berlin`。时刻全部注入,无 sleep。
6. `Cargo.lock` 只给 `af-workorder` 补上 `af-calendar` / `chrono` / `chrono-tz` 三条边。这三个包原本就在锁文件里,没有新的 crates.io 版本。锁文件在 crate 目录外,但是路径依赖不写锁文件就不完整。未改任何其他 crate 的源码。

## ③ 证据

命令均在仓库根、先 `source /data/af-factory/env.sh`。`cargo fmt` 用了 `-p af-workorder`,避免全仓 fmt 改到别的 crate。

`cargo check -p af-workorder`(类型切换后):

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.39s
```

`cargo fmt -p af-workorder -- --check` → 退出码 0。

`cargo clippy --all-targets --no-deps -p af-workorder -- -D warnings` → `Finished dev profile`,退出码 0(含 example 与 tests)。

`cargo test -p af-workorder`:

```text
running 36 tests
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured
     Running tests/calendar_sla.rs
running 3 tests
test calendar_deadline_enters_lifecycle_and_judges_by_absolute_instant ... ok
test fall_back_order_deadline_is_post_fold_utc_instant ... ok
test spring_forward_order_deadline_is_post_gap_utc_instant ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
     Running tests/lifecycle.rs
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
   Doc-tests af_workorder
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured
```

测试数:原 34 单测仍在;新增 `order_overdue_uses_stored_absolute_deadline`、`from_calendar_rejects_negative_duration`。原 3 个 `tests/lifecycle.rs` 仍在;新增上述 3 个集成测试。

`cargo run -p af-workorder --example gate_demo` 仍打印原来的门控拒绝原因,并以 `Closed` 结束。

字面 clippy(退出码 101,未改 af-calendar):

```text
error: the loop variable `day` is used to index `WEEKDAY_NAMES`
 --> crates/af-calendar/src/calendar.rs:114:20
 = note: `-D clippy::needless-range-loop` implied by `-D warnings`
error: could not compile `af-calendar` (lib) due to 1 previous error
```

集成测试锁住的绝对时刻(与 af-calendar 已验证的班次规则一致,不是新算法):

| 场景 | 下单 | 承诺 | 订单上的 deadline |
|---|---|---|---|
| 上海工作日 08:00–17:00 | 2026-10-23 16:00+08(周五) | 4 工时 | 2026-10-26 03:00:00Z(周一 11:00+08) |
| 柏林春跳(03-29 02:00→03:00) | 2026-03-27 16:00 CET | 4 工时 | 2026-03-30 09:00:00Z(周一 11:00 CEST)。固定 +01 会误成 10:00Z |
| 柏林秋拨(10-25 03:00→02:00) | 2026-10-23 16:00 CEST | 4 工时 | 2026-10-26 10:00:00Z(周一 11:00 CET)。残留 +02 会误成 09:00Z |

判定:恰在 deadline 不算逾期;deadline 之后 1 秒算逾期。春跳用例在 09:30Z(周一 11:30 CEST)判逾期——这个时刻早于错误的 10:00Z。秋拨用例在 09:30Z(周一 10:30 CET)判未逾期——这个时刻晚于错误的 09:00Z。两条都在 Created → Released → InProgress → Closed 之后仍是同一个 `DateTime<Utc>`。上海用例另外锁了 `10:59+08`(绝对时刻 02:59Z)不算逾期;旧字典序会把 `"…T03:00:00Z" < "…T10:59:00+08:00"` 判成已逾期。

## ④ 反馈

1. **判定与解析分开**。日历只在建 SLA 时把承诺工时折成绝对时刻;之后逾期只比 `DateTime<Utc>`。这样 DST 偏移不会在每次查询时被重新解释,也和日历 RESULT ④-1"日历只答时间"一致。
2. **不要把 `Calendar` 嵌进每张工单**。判定不需要它,而且 `Calendar` 没有 `Eq`,塞进去会拆掉 `WorkOrder` 的相等性。日历作为配置注入给 `Sla::from_calendar` 即可。
3. **线格式兼容靠 chrono 的 RFC3339,不靠双字段**。若上游还有非 RFC3339 的旧字符串,那是另一类数据,本切换覆盖不了。
4. **字面 `clippy -p af-workorder -D warnings` 会 lint path 依赖**。af-calendar 的既有警告因此变成硬错误。建议这条质量阶梯写成 `--no-deps`,或者继续用仓库的基线门(只拦新增签名)。否则每个"只许改一个 crate、但要依赖另一个已有警告的 crate"的任务都会在 clippy 步被既有警告挡住。

## ⑤ 遗留

- 字面 clippy 命令因 af-calendar 既有 `needless_range_loop` 不能退出 0。本 crate 自身无警告。修不修那两条循环由 af-calendar 的主人决定(基线里该文件记了 2 次,本次只露出第 1 个就停了)。
- `payload_hash` 仍是 FNV-1a64,签名仍是占位结构。与本次 SLA 无关,workorder RESULT ⑤ 的原遗留还在。
- 预算凭证、权限法定人数、`dyn Clock` 仍未做。逾期判定的 `now` 由调用方注入,没有默认系统钟。
- 夜班两窗口的 1ns 接缝、开工点不可作为完工时刻,仍是 af-calendar 已记录的坑。本 crate 原样保存日历返回的绝对时刻,没有再取整。

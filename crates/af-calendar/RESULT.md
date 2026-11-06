# RESULT.md — poc-calendar「通用日历与SLA计时」

## ① 结论

**通过**。五点要求全部实现并真实执行验证:`cargo test -j 4 -p af-calendar` 18 个集成测试全绿(含题设两条验收:周五 16:00 下单承诺 4 工时 → 下周一截止;黄金周/调休跨越正确);`cargo run -p af-calendar --example sla_demo` 端到端跑通(JSON 配置加载 → SLA 正向 → 截单派生 → UTC 消费者)。

## ② 做了什么

crates/af-calendar(原空壳,workspace 根未动),依赖仅 chrono 0.4.45 + chrono-tz 0.10.4 + serde/serde_json + thiserror(无 tokio/rusqlite)。三个文件:

| 要求 | 实现 | 位置 |
|---|---|---|
| 班次日历 | `Calendar { tz, weekly:[Vec<Shift>;7], holidays:BTreeSet<NaiveDate>, day_overrides:BTreeMap<NaiveDate,Vec<Shift>>, horizon_days }`。日期→班次解析优先级:**指定日覆盖(调休,空 Vec=强制放假) > 假日表 > 周模板**;支持一天多班次(午休拆窗);窗口 `[start,end)` 左闭右开;`validate()` 校验窗口倒置/重叠(反序列化绕过构造器,加载配置后必须调) | `src/calendar.rs` |
| next_working_instant(now, duration) | 按日历推进有效工时:逐日逐窗消耗,now 在非班次时间先滚到开工点;duration=0 语义"下一可开工时刻";负时长类型化报错;搜索视界(默认 10 年)耗尽报 `HorizonExceeded` 防死循环 | `src/advance.rs:13-60` |
| SLA 截止 | `sla_deadline(placed_at, promised_working)` = 正向推进的工单语义包装 | `src/advance.rs:97-108` |
| 截单派生 | `latest_order_at(due_at, required)`:从交付承诺时刻**逆向**沿日历消耗所需工时(仅 due 当天有上界,往前窗口完整可用);due 不在班次内先收敛到"≤ due 的最后完工时刻"。与正向构成一对:截单点=日历派生值,不是一等字段(与底座设计纪律 §74 一致) | `src/advance.rs:110-175` |
| 跨时区 | 三个接口全部泛型 `Tz: TimeZone + Clone`:输入输出保持**调用方**时区(UTC 消费者拿 UTC,FixedOffset 进 FixedOffset 出),内部一律换算到日历时区计算;DST 歧义取较早偏移、间隙日跳过该班次(Asia/Shanghai 无 DST 不受影响) | `src/calendar.rs:local_dt` + 各接口 |

测试 18 条(`tests/sla.rs`):题设两条 + 调休周六上班/普通周六停工 + 黄金周长跨(9-30 16:00 +4h → 10-08 11:00)+ UTC/FixedOffset 消费者 + 下班后/周末滚动 + 多班次跳午休 + 正反互逆 roundtrip + 班次外交付的**最晚性**断言(派生点+1min 必然错过交付)+ 视界耗尽/负时长/窗口重叠校验 + JSON 配置 roundtrip + 窗口末完工边界(13:00+4h=17:00 整,+1s 滚到次日)。

## ③ 证据

命令 1:`cargo test -j 4 -p af-calendar`(cwd=af workspace 根,rustc/cargo 1.99.1,chrono 0.4.45):

```text
running 18 tests
test friday_16h_order_4h_deadline_is_next_monday ... ok
test golden_week_crossing_holiday_table ... ok
test makeup_workday_saturday_counts_as_working ... ok
test utc_consumer_gets_same_instant_in_utc ... ok
test latest_order_roundtrip_forward_equals_due ... ok
test latest_order_is_maximal_even_when_due_off_shift ... ok
test multi_shift_calendar_skips_lunch ... ok
test json_config_roundtrip_reproduces_deadline ... ok
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured
```

命令 2:`cargo run -j 4 -p af-calendar --example sla_demo`(关键输出):

```text
[SLA 正向] 周五 16:00 下单,承诺 4 工时
  截止  : 2026-10-26 11:00:00 CST  ← 下周一 11:00
  UTC 视图: 2026-10-26 03:00:00 UTC
[假日跨越] 国庆前周三 16:00 下单 4 工时
  截止  : 2026-10-08 11:00:00 CST  ← 黄金周后首个工作日 11:00
[截单派生] 交付承诺 10-26 11:00 Mon 需 4 工时
  最晚下单: 10-23 16:00 Fri  ← 正反互逆(与 SLA 正向同一对时刻)
[跨时区] UTC 消费者下单 2026-10-23T08:00:00+00:00
  截止  : 2026-10-26T03:00:00+00:00(返回保持 UTC;上海读回 = 11:00 CST)
全部演示断言通过。
```

诚实说明:①假日表(黄金周 10-01..10-07、09-26 调休)是**测试夹具数据**,非官方放假安排,断言只对夹具自洽;②workspace 全量 `cargo build -j 4` 通过,其 future-incompat 警告经 `cargo tree -i proc-macro-error2` 定位来自 **af-credential**(他人 crate),af-calendar 依赖树干净。

## ④ 对最终方案的反馈

1. **接口定成"日历只答时间,不答业务"**:`next_working_instant` / `sla_deadline` / `latest_order_at` 三个纯函数 + `shifts_on`/`is_working_instant` 两个查询,足够工位计时、时间窗放置、容量池四类消费者复用;建议最终方案保持日历本体无 SLA 字段、无优先级概念(截单点=派生值已验证好用)。
2. **跨时区接口要"进什么时区、出什么时区"**:泛型 `Tz: TimeZone + Clone` 让 UTC 上游系统和上海排班表共用同一日历而互不污染——测试证明 UTC 下单拿回 UTC 且与上海结果同一时刻。最终方案若接受字符串 RFC3339 入参,建议统一转 UTC 再进日历,输出方再换时区。
3. **坑:正反互逆在"交付点不在班次内"时不是恒等式**。完工时刻只可能落在工时时刻上,交付承诺 20:00(下班后)时,逆向派生收敛到"≤ due 的最后完工时刻"(如 17:00),回算=17:00 ≠ 20:00 但确实是最晚可行下单点。测试用"≤ due + 最晚性(派生点+1min 必超期)"替代严格恒等。**建议最终方案规定**:交付承诺必须规范化到日历可达时刻,或 API 文档明示 slack 语义。
4. **坑:req=0 是两个谓词的接缝**。逆向 0 工时返回"最后完工时刻"(窗口末算完工),正向 0 工时返回"下一开工时刻"(窗口末不可开工)——17:00 这个点上两者结论相反。已如实写进测试注释;最终方案建议干脆禁用 req=0 或单独定义。
5. **假日表要"假日+调休"两种条目**:中国真实日历 = 法定假日(周中放假)+ 调休(周末上班),单纯 BTreeSet<日期> 不够;本 PoC 用"指定日覆盖"统一表达(空窗口=强制放假,非空=上班),优先级高于假日表,已验证两种方向都正确。建议最终方案保留此二元结构。
6. **搜索视界必须显式**:日历全空或需求过大时,朴素循环会死循环;`horizon_days` + `HorizonExceeded{ searched_days }` 让上层可报警。校验也是同理——serde 反序列化绕过构造器,`validate()` 必须由加载方调用,最终方案建议做成 `TryFrom<Config>` 或自定义 deserializer 收口。

## ⑤ 遗留问题

- ~~**跨午夜班次(夜班 22:00-06:00)不支持**:`Shift` 强制 start<end,夜班需拆两天两窗口;最终方案若要夜班,建议 Shift 增加跨日标志并在推进算法按小时切片。~~ **已补测(2026-10-19,见⑥)**:按"两天两窗口"建模实测了夜班跨午夜的正逆向派生、假日/调休交互,接缝损耗 1 tick/晚已量化记录;"Shift 加跨日标志"的建议保留。
- ~~**DST 时区未实测**:Asia/Shanghai 无夏令时,`local_dt` 的歧义取早/间隙跳过逻辑未经真实 DST 日历验证(chrono-tz 自带数据,理论上正确);接海外工厂前补一条 New York 日历测试。~~ **已补测(2026-10-19,见⑥)**:Europe/Berlin 春跳/秋拨逐点实测 + America/New_York 多年扫描,语义裁决=流逝时间推进、歧义取较早偏移,与原设计一致。
- **半日假/按小时的窗口级假日**:假日表粒度是"整日";半天假需用 day_overrides 写全窗口,可表达但啰嗦。
- **与 af-workorder 的衔接**:其 RESULT 已指出 deadline 现为字符串比较;衔接时由 af-workorder 持有 `Calendar` 实例(注入),`Sla.deadline` 改存本 crate 输出的 `DateTime<Utc>`。
- **性能未压测**:O(视界天数×窗口数) 对 10 年视界也只需万次循环,单次计算微秒级;若未来要做大批量订单排程,可预展开工作窗口索引。

## ⑥ 2026-10-19 补测:DST/夜班(⑤节前两项遗留)

新增 `tests/dst_night_shift.rs`(11 个常规测试 + 1 个 `#[ignore]` 密集扫描),未改任何 src 语义。

### 结论

**夜班跨午夜**:`Shift` 仍不支持跨午夜窗口(start<end 构造期即拒绝),按文档建议用"两天两窗口"建模:当天 `[22:00, 24:00)`(用 23:59:59.999999999 逼近 24:00)+ 次日 `[00:00, 06:00)`。实测:周五 22:00 + 4 工时 → 周六 02:00:00.000000001;周六 23:30 + 1 工时跨到周日 00:30:00.000000001;连续两晚(4h/12h)正反互逆成立,截单点恰回到原始起点周五 22:00;假日(该日全部窗口取消)与调休(非空覆盖=上班)对正向推进和逆向截单一致生效,与④-5 的二元假日表语义自洽。上海(无 DST)同模型同结果,证明行为与 DST 无关。

**DST 语义裁决(全部实测,非推理)**:窗口按本地钟面定义,工时按**流逝时间**(elapsed/绝对时间)推进——chrono 的 `DateTime + TimeDelta` 是 UTC 流逝算术,crate 的 `avail = we − entry`、`done = entry + remaining` 全部继承:
- **春跳**(Berlin 2026-03-29 02:00→03:00):跳失的小时**不计工时**。凌晨窗 `[00:00,06:00)` 钟面 6h、流逝容量只有 5h;整晚名义 8h 实际 7h−1ns。跨缺失段的加法不落在不存在的 02:xx 钟面,直接落到 **03:00**(01:00+1 工时 → 03:00:钟面 +2h、流逝恰好 1h)。
- **秋拨**(Berlin 2026-10-25 03:00→02:00):重复的一小时是真实时间,容量**计入一次**。凌晨窗钟面 6h、流逝 7h;周六 22:00 + 8 工时 → 周日 05:00:00.000000001 CET(钟面只走 7h+1ns,流逝 8h+1ns)。
- **歧义时刻消歧**:班次边界落在歧义钟面时取**较早偏移**(`local_dt` 的 `.earliest()`,与原设计一致)——实测窗口末 02:30 按第一次出现(CEST)计,窗口容量 2.5h 而非 3.5h,2h40m 即溢出;`is_working_instant` 是纯钟面判定**不消歧**,歧义时刻两次出现同答"班次内";工时算术从任一出现出发按流逝推进(第一次 02:30 + 1 工时 → 第二次的 02:30)。

**多年扫描稳定性**:2024-01-01T00:00Z 起按步进扫 3 年(至 2027-01-01),Europe/Berlin 与 America/New_York 各覆盖 3 次春跳 + 3 次秋拨;每个点做"逆向派生 8 工时 → 正向回算 8 工时"。6h UTC 网格 4384 点/时区 × (夜班日历 + 24×7 日历);`#[ignore]` 密集 1h 网格 26304 点/时区 × 2 日历 × 2 时区 = 105,216 点已实跑(1.35s)。结果:**回算从未超过交付点(back ≤ p 恒成立);不互逆点只落在下述已记录的两类(班次外交付点=已知坑④-3、开工点=新边界坑),24×7 日历 6h 网格全部严格互逆(含全部 DST 边界日与转换时刻),无任何新不一致**。

### 证据

`cargo test -j 6 -p af-calendar`:

```text
running 12 tests   (tests/dst_night_shift.rs)
test fall_back_ambiguous_instant_resolution ... ok
test due_at_window_start_is_not_completion_reachable ... ok
test fall_back_repeated_hour_counts_once_as_elapsed_time ... ok
test night_shift_friday_22h_plus_4h_lands_saturday_02h ... ok
test night_shift_semantics_identical_without_dst ... ok
test night_shift_saturday_2330_plus_1h_crosses_to_sunday ... ok
test night_shift_with_holiday_and_makeup_override ... ok
test spring_forward_addition_into_missing_hour_lands_after_gap ... ok
test spring_forward_gap_hour_not_counted_as_working_time ... ok
test three_year_scan_6h_grid_berlin_and_new_york ... ok
test two_consecutive_nights_forward_reverse_roundtrip ... ok
test dense_hourly_scan_3y_berlin_and_new_york ... ignored(慢速全表扫描)
test result: ok. 11 passed; 0 failed; 1 ignored
   (tests/sla.rs 原 18 个亦全绿,未受影响)
```

`cargo test -p af-calendar --test dst_night_shift -- --ignored`(密集扫描实跑):

```text
test dense_hourly_scan_3y_berlin_and_new_york ... ok
test result: ok. 1 passed; 0 failed; finished in 1.35s
```

### 新发现坑(只记录,修不修由人拍板)

1. **两窗口夜班在午夜接缝丢 1 tick**:半开区间 `[22:00, 23:59:59.999999999)` ∪ `[00:00, 06:00)` 之间恰好空出一个纳秒,每跨一夜完工点晚 1ns(已精确断言);连续 n 晚累计 n ns。若用秒粒度 end=23:59:59 则每晚丢 1s。根因:`Shift` 无跨日标志、chrono 的 `NaiveTime` 无 24:00(`NaiveTime::MAX` 还是私有项,公开 API 只能 `from_hms_nano_opt` 构造)。建议最终方案给 `Shift` 加跨日标志或允许 end=24:00,推进算法按小时切片。
2. **开工点不可作为完工时刻(互逆的新边界例外)**:交付承诺恰在开工点时 `is_working_instant = true`(窗口左闭),但"恰在该时刻完工"不可达——逆向派生把该班次整窗跳过(`ws >= bound` 即 continue),回算点 < due;≤ 交付与最晚性(+1ns 即不可行)均保持,已定点测试记录。lib.rs/advance.rs 中"due 在班次内恒互逆"的措辞在开工点不成立,建议改为"due 在工时可达时刻(即严格晚于某开工点)时恒互逆"。该坑与 DST 无关,上海日历同样触发。
3. **`is_working_instant` 对 DST 歧义时刻不消歧**:两次出现同答(钟面判定)。需要区分"第一次/第二次"的消费者(如时间窗放置)须在日历外用 `.earliest()`/`.latest()` 自行规约,建议 API 文档明示。
4. 扫描结论:除坑 2 与已知坑④-3 两类外,3 年 × 2 时区 × 2 日历、最密 1h 网格上无其他不互逆点,亦无"回算超期"。

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

- **跨午夜班次(夜班 22:00-06:00)不支持**:`Shift` 强制 start<end,夜班需拆两天两窗口;最终方案若要夜班,建议 Shift 增加跨日标志并在推进算法按小时切片。
- **DST 时区未实测**:Asia/Shanghai 无夏令时,`local_dt` 的歧义取早/间隙跳过逻辑未经真实 DST 日历验证(chrono-tz 自带数据,理论上正确);接海外工厂前补一条 New York 日历测试。
- **半日假/按小时的窗口级假日**:假日表粒度是"整日";半天假需用 day_overrides 写全窗口,可表达但啰嗦。
- **与 af-workorder 的衔接**:其 RESULT 已指出 deadline 现为字符串比较;衔接时由 af-workorder 持有 `Calendar` 实例(注入),`Sla.deadline` 改存本 crate 输出的 `DateTime<Utc>`。
- **性能未压测**:O(视界天数×窗口数) 对 10 年视界也只需万次循环,单次计算微秒级;若未来要做大批量订单排程,可预展开工作窗口索引。

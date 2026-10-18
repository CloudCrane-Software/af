//! af-calendar — 通用日历与 SLA 计时(PoC)。
//!
//! 三个核心语义:
//! 1. **班次日历** [`Calendar`]:时区 + 每周班次模板 + 假日表 + 指定日覆盖(调休)。
//!    日历内容 = 运营者配置(serde 可 JSON 序列化);工位计时 / SLA 承诺 / 时间窗放置 /
//!    容量池都是它的消费者(见 底座设计纪律_v2.1 §通用日历)。
//! 2. **工时推进** [`Calendar::next_working_instant`]:按日历推进"有效工时",
//!    非班次时间(夜间/周末/假日)自动跳过。
//! 3. **SLA 双向**:正向 [`Calendar::sla_deadline`](下单+承诺工时→实际截止)与
//!    反向 [`Calendar::latest_order_at`](交付承诺−所需工时→最晚下单,截单点=日历派生值,
//!    不是一等字段)。
//!
//! 边界约定:
//! - 班次窗口 `[start, end)` 左闭右开;恰好推进到 `end` = 工时在该时刻用尽。
//! - 班次必须落在单个本地日内,跨午夜班次不支持(拆两天两窗口)。
//! - 输入输出泛型保持调用方时区(UTC 消费者拿 UTC),内部一律按日历时区计算;
//!   含 DST 时区的歧义取较早偏移,间隙日的该班次跳过(Asia/Shanghai 无 DST,不受影响)。

mod advance;
mod calendar;
mod error;

pub use calendar::{Calendar, Shift};
pub use error::CalendarError;

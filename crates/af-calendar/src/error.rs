//! 错误类型:全部类型化,上层可程序化区分"日历配置坏了"和"视界内找不到工时"。

use chrono::NaiveTime;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CalendarError {
    /// 班次窗口起止倒置(start >= end)。跨午夜班次本 PoC 不支持,应拆成两天两个窗口。
    #[error("班次窗口倒置: {start} >= {end}(跨午夜班次请拆成两天)")]
    WindowInverted { start: NaiveTime, end: NaiveTime },

    /// 同一天内的班次窗口重叠或未按升序排列(校验失败)。
    #[error("{date} 的班次窗口重叠/乱序: [{a_start},{a_end}) 与 [{b_start},{b_end})")]
    OverlappingWindows {
        date: String,
        a_start: NaiveTime,
        a_end: NaiveTime,
        b_start: NaiveTime,
        b_end: NaiveTime,
    },

    /// 负时长。零时长合法,语义=滚动到下一个可开工时刻。
    #[error("时长必须非负,得到 {seconds} 秒")]
    NegativeDuration { seconds: i64 },

    /// 在搜索视界内找不到足够工时(日历全空 / 需求时长过大 / 假日表过长)。
    #[error("在 {searched_days} 天内找不到足够工时(日历可能为空、假日过长或需求过大)")]
    HorizonExceeded { searched_days: u32 },
}

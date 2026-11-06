//! 账本与版本边界的错误。未知版本、声明不符、损坏的落盘都从这里失败，不降级继续。

use serde_json::Error as JsonError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// JSON 或调用方结构上的 `v` 不是当前版本。
    #[error("未知版本 {found}，期望 {expected}")]
    UnknownVersion { found: u32, expected: u32 },

    /// 文档里没有 `v`。缺少版本按拒绝处理，不默认成 1。
    #[error("缺少版本字段")]
    MissingVersion,

    /// `v` 存在，但不是能放进版本号的无符号整数。
    #[error("版本字段无法识别：{raw}")]
    UnreadableVersion { raw: String },

    /// SQLite `user_version` 不是本账本能打开的 schema。
    #[error("未知的账本 schema 版本 {found}，期望 {expected}")]
    UnknownSchema { found: i64, expected: i64 },

    /// 重入时已落盘的声明与本次声明不一致。
    #[error("声明与已落盘状态不符：{detail}")]
    DeclarationMismatch { detail: String },

    /// 库能打开，但里面的行违反本 crate 的不变量。
    #[error("账本损坏：{0}")]
    Corrupt(String),

    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json: {0}")]
    Json(#[from] JsonError),
}

//! 基准三态。合法方向只有 draft → frozen → retired。

use std::fmt;

/// 基准所处的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 可增删用例。还不能拿来评测。
    Draft,
    /// 内容已封存。评测只能读这一态的快照。
    Frozen,
    /// 不再接受新的评测。内容仍然不可变。
    Retired,
}

impl Phase {
    /// 落盘与错误信息用的稳定记号。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Frozen => "frozen",
            Self::Retired => "retired",
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

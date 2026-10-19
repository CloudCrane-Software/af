//! 标识与正文的形状。非法输入在写账本之前拒绝。

use crate::error::{EvalError, Result};

/// 谱系、用例、运行、配方、被评对象的标识上限。
pub(crate) const MAX_IDENT_LEN: usize = 256;
/// 用例正文与配方正文的上限。
pub(crate) const MAX_BODY_LEN: usize = 65_536;

/// ASCII 可见字符，不含空格与控制符。这样标识能放进日志且不含分隔空白。
pub(crate) fn require_ident(field: &'static str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_IDENT_LEN
        || !value.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(EvalError::invalid(field));
    }
    Ok(())
}

/// 非空正文，按字节计长度。允许换行与非 ASCII。
pub(crate) fn require_body(field: &'static str, value: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.len() > max {
        return Err(EvalError::invalid(field));
    }
    Ok(())
}

pub(crate) fn require_version(version: u64) -> Result<i64> {
    if version == 0 {
        return Err(EvalError::invalid("version"));
    }
    i64::try_from(version).map_err(|_| EvalError::invalid("version"))
}

pub(crate) fn require_budget(budget_ms: i64) -> Result<()> {
    if budget_ms <= 0 {
        Err(EvalError::BadBudget)
    } else {
        Ok(())
    }
}

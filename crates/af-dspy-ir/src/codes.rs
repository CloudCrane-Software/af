//! 护栏账本用的稳定字符串。
//!
//! IR 语义错误仍用 DI1xxx，预筛拒绝时原样抄进原因，不在这里重编。
//! 这里只放崩溃分类和护栏自己的 JG 码。

/// 基础设施崩溃。纯崩溃时允许修复。
pub const E_INFRA: &str = "E_INFRA";
/// 候选程序自身崩溃。纯崩溃时允许修复。
pub const E_SUBJECT: &str = "E_SUBJECT";
/// 低分。一律拒绝修复。
pub const E_SCORE: &str = "E_SCORE";
/// 低分的另一种写法，与 [`E_SCORE`] 同一类。
pub const E_LOW_SCORE: &str = "E_LOW_SCORE";

/// 候选 id 为空。
pub const JG_BLANK_CANDIDATE: &str = "JG1001";
/// 同一批里候选 id 重复。后来的那条不进评测。
pub const JG_DUPLICATE_CANDIDATE: &str = "JG1002";
/// 批次 id 为空。整批不进评测。
pub const JG_BLANK_BATCH: &str = "JG1003";
/// 低分类。一律拒绝修复。
pub const JG_LOW_SCORE: &str = "JG2001";
/// 失败码不是崩溃类。
pub const JG_NOT_A_CRASH: &str = "JG2002";
/// 崩溃码上附带了评测分数。
pub const JG_SCORE_ON_CRASH: &str = "JG2003";
/// 运行 id 为空，无法把修复决定记到一次运行上。
pub const JG_BLANK_RUN: &str = "JG2004";

pub(crate) const REPAIR_DENY_CODES: &[&str] = &[
    JG_LOW_SCORE,
    JG_NOT_A_CRASH,
    JG_SCORE_ON_CRASH,
    JG_BLANK_RUN,
];

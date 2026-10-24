//! 拒绝式版本信封（E0001-E0004）。承 af-harness `decode_spec` 与 af-dspy-ir
//! `Program.version` 双先例：version 非 1 → 整份拒绝、不解析 body。

use crate::error::{ErrorCode, LoadError, Report};

/// 信封合法的顶层键：version + 六段（七段 = version/meta/types/stores/nodes/edges/policies）。
pub const BODY_KEYS: [&str; 6] = ["meta", "types", "stores", "nodes", "edges", "policies"];

/// 信封检查：缺键 / 非整数 / 非 1 / 未知键 → `LoadError::Envelope`（整份拒绝）。
pub fn check(value: &serde_yaml::Value) -> Result<(), LoadError> {
    todo!("M1: 信封检查（E0001-E0004）")
}

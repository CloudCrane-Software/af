//! 语义层校验编排（M1）：手写词表 → 重复 id → 引用闭包 → INV-2，一次收齐。
//! 校验顺序固定以保证违例输出确定性（承提案 B §3.1 顺序说明的 M1 前缀）。

use crate::error::Report;
use crate::model::Document;

pub fn validate(doc: &Document) -> Report {
    todo!("M1: 语义校验编排")
}

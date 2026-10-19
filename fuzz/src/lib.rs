//! fuzz 目标与 `cargo test` 共用的输入构造。
//!
//! 字节布局放在这里，避免 harness 和种子测试各写一套。

pub mod sampling;

//! 幂等键、载荷指纹和声明时订单态快照。
//!
//! 逻辑在 [`af_fuzz::gateway`]，这样 `cargo test` 能复跑同一条路径。
//! `declare_intent` / `execute` 都递交 [`af_workorder::WorkOrder`]。

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    af_fuzz::gateway::fuzz_one(data);
});

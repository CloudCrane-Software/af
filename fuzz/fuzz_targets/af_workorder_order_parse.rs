//! 订单 JSON 与门控状态机。
//!
//! 逻辑在 [`af_fuzz::workorder`]。解析走当前的拒绝式信封，不补 `stable_id`。

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    af_fuzz::workorder::fuzz_one(data);
});

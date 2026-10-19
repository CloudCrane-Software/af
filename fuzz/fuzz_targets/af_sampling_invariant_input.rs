#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // 骨架占位：下一步换成从字节构造配方/事件序列并调用升迁引擎。
    let _ = data;
});

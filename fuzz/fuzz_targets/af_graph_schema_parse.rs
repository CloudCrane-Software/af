#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    af_fuzz::graph::fuzz_one(data);
});

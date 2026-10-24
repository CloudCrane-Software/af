//! 端点档案与密钥解析（af-bench 吸收 P1，RED 骨架）。
//!
//! 目标形态（绿提交落位，原样搬入）：
//! - `Profile`/`profile()` ← af-bench main.rs:156-327（key_cmd 字符串逐字保留；
//!   密钥纪律：明文只从解析器命令出来进请求头，不打印、不落文档——main.rs:10）；
//! - `resolve_key()` ← af-bench main.rs:329-350。

use af_harness::EndpointSpec;

pub struct Profile {
    pub endpoint: EndpointSpec,
    pub key_cmd: &'static str,
}

pub fn profile(_name: &str) -> Result<Profile, String> {
    unimplemented!("P1 RED 桩：实现随绿提交自 af-bench main.rs:163-327 原样搬入")
}

pub fn resolve_key(_cmd: &str) -> Result<String, String> {
    unimplemented!("P1 RED 桩：实现随绿提交自 af-bench main.rs:329-350 原样搬入")
}

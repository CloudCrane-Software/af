//! INV 组（theorems-v1 A 组 16 条）Kani 镜像穷尽 harness。
//!
//! 模式 = FV-0.2 已验证镜像法（FV0-工具链PoC-20261024.md §2）：
//! - 纯逻辑层复刻：串→有界整数/布尔域抽象，无 BTree/format!/serde/YAML——
//!   绕开 FV-0 阻断一（kani-compiler 0.68.0 对 validate 可达链 ICE）与
//!   阻断二（CBMC 对串/树容器病态），**绝不** harness af-dsl validate 全链。
//! - 全枚举：kani::any() 符号位 + assert = CBMC 对声明的有界域全域判定；
//!   每 harness 另含一个手构具体角点 sanity assert（非空洞性守护）。
//! - 镜像逐分支锚定真实源码行号（af-wt-phase2-dsl@phase2-g2b
//!   crates/af-dsl/src/validate/inv{1,2,3,4}.rs），映射总表见 MAPPING.md。
//!
//! 裁判：Kani 0.68.0（tmpfs /dev/shm/fv0/kani-0.68.0，FV-0 保留）
//! `VERIFICATION:- SUCCESSFUL` / `VERIFIED-SUCCESS` 为唯一合格判据。

pub mod mirror_inv1;
pub mod mirror_inv2;
pub mod mirror_inv3;
pub mod mirror_inv4;

#[cfg(feature = "conflict-demo")]
pub mod conflict_demo;

// ============ 稳定码位表（error.rs:160-179 as_str 口径，镜像共用位集） ============
// 每码一位；镜像函数返回触发码的并集（Report 的码集合镜像）。
pub const C_E2301: u32 = 1 << 0; // ContextSourceUnresolvable  error.rs:168
pub const C_E2302: u32 = 1 << 1; // QuotaNotPositive           error.rs:169
pub const C_E2303: u32 = 1 << 2; // AgentDirectStoreRead       error.rs:170
pub const C_E2304: u32 = 1 << 3; // InputPortUnreferenced      error.rs:171
pub const C_E2402: u32 = 1 << 4; // ArgNotInParams             error.rs:173
pub const C_E2311: u32 = 1 << 8; // StoreMultiWriter           error.rs:175
pub const C_E2312: u32 = 1 << 9; // MapSharedStoreWrite        error.rs:176
pub const C_E2211: u32 = 1 << 10; // CarriedNotCovering        error.rs:167
pub const C_E2321: u32 = 1 << 13; // SelfVerification          error.rs:177
pub const C_E2322: u32 = 1 << 14; // SafetyPointNotVerifier    error.rs:178
pub const C_E2331: u32 = 1 << 16; // EvalInputVisible          error.rs:179

pub const FAM_INV1: u32 = C_E2301 | C_E2302 | C_E2303 | C_E2304;

//! 配置包真实现宿主（G4 软件工程包 v1，设计《G4-软件工程包设计-v1》§2.1）。
//!
//! 落位纪律：实现住执行面**进程内注册表**（落地契约 §2.2 tool 契约原文），全部
//! 在 `af-execline` 的包内新模块——不改 DSL/编译器语言核心（任务中立铁律，
//! 设计 §1.3），不改既有 crate 公共接口语义。两模块：
//!
//! - [`sweng`]：15 件组件真实现（11 tool + 4 unit；E2E 路径件 T1/U1 真实现，
//!   其余单元级实测——裁决17 分级口径）+ ToolDecl 表 + impl_ref 锚约定
//!   （裁决13 双形态：`executor:builtin/<名>@<crate>`）。
//! - [`prompts`]：template_ref 解析唯一机制（裁决10：`pkg.<包>.<名>[@<版本>]`
//!   → `<包根>/<包>/prompts/<名>.md` + `MANIFEST.sha256` 清单核对；两包共用，
//!   解析器一套；报告包模板落 `reportkit/prompts/` 走同一解析器）。
//!
//! `planbind.rs` 通用装订器（E9004→EX4004 执行面消费唯一正本，裁决9）由
//! G4 Phase 2（reportkit 批）落位——届时 digest 核对与模板解析在此汇流；
//! 本批先行落模板解析器与 15 件实现（EX4xxx 段已开：EX4001-4003）。

pub mod prompts;
pub mod sweng;

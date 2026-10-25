//! G4 软件工程包 v1 包资产验收（与包文件同 commit 落盘——裁决§二 TDD 语义：
//! 断言与被断言现实同一提交）。
//!
//! 验收面：
//! - 提示词载荷迁移纪律（设计 §3.2-3）：`sweng/prompts/swev_system.md` 与
//!   `swev_instance_protocol.md` 与 Rust 冻结常量**字节一致**（测量语义载荷
//!   一个字节不动——swev.rs:6-8 纪律原文）。
//! - template_ref 机制对仓库正本的闭环（裁决10）：图内引用可解析、MANIFEST
//!   闭集（文件集 = 登记集）、版本段核对、aci_system 在册。
//! - `load_swev_prompts`（SwevUnitSpec 装载化正路入口）返回字节等于常量。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use af_execline::pkg::prompts;
use af_execline::suite::swev::{SWEV_INSTANCE_PROTOCOL, SWEV_SYSTEM_PROMPT};

fn repo_packages_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn prompts_dir() -> PathBuf {
    repo_packages_root().join("sweng").join("prompts")
}

#[test]
fn migrated_prompt_files_are_byte_identical_to_frozen_constants() {
    let system = std::fs::read(prompts_dir().join("swev_system.md"))
        .expect("sweng/prompts/swev_system.md 应在盘");
    assert_eq!(
        system,
        SWEV_SYSTEM_PROMPT.as_bytes(),
        "迁移纪律：swev_system.md 与 SWEV_SYSTEM_PROMPT 字节一致（sha256 核对正本在 MANIFEST）"
    );
    let protocol = std::fs::read(prompts_dir().join("swev_instance_protocol.md"))
        .expect("sweng/prompts/swev_instance_protocol.md 应在盘");
    assert_eq!(
        protocol,
        SWEV_INSTANCE_PROTOCOL.as_bytes(),
        "迁移纪律：swev_instance_protocol.md 与 SWEV_INSTANCE_PROTOCOL 字节一致"
    );
}

#[test]
fn repo_template_refs_resolve_and_version_check_holds() {
    let root = repo_packages_root();
    // 图 fix_swe_task@1.0 的 template_ref（pkg.sweng.swev_system@1，版本段给出）。
    let asset = prompts::resolve_template(&root, "pkg.sweng.swev_system@1")
        .expect("图内 template_ref 应可解析");
    assert_eq!(asset.content, SWEV_SYSTEM_PROMPT);
    assert_eq!(asset.version.as_deref(), Some("1"));

    // 无版本段（可选语法）同解；instance protocol 同闭环。
    let asset = prompts::resolve_template(&root, "pkg.sweng.swev_instance_protocol")
        .expect("无版本段引用应可解析");
    assert_eq!(asset.content, SWEV_INSTANCE_PROTOCOL);

    // aci_system（pkg.sweagent.aci_system@1 的解析目标，暂驻 sweng/prompts——
    // sweagent 建包（G4+ 真包化）前不设包目录，登记于设计 §3.1）。
    let asset =
        prompts::resolve_template(&root, "pkg.sweng.aci_system@1").expect("aci_system 应登记在册");
    assert!(asset.content.contains("agent_action"), "{}", asset.content);

    // 版本不符拒（EX4003）——清单版本核对有牙齿。
    let err = prompts::resolve_template(&root, "pkg.sweng.swev_system@2").unwrap_err();
    assert!(err.to_string().starts_with("EX4003"), "{err}");
}

#[test]
fn sweng_prompt_manifest_is_closed_over_files_on_disk() {
    let dir = prompts_dir();
    let manifest = std::fs::read_to_string(dir.join("MANIFEST.sha256")).unwrap();
    let listed: BTreeSet<String> = manifest
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().nth(1).map(str::to_string))
        .collect();
    let mut on_disk: BTreeSet<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("md"))
        .map(|p| format!("prompts/{}", p.file_name().unwrap().to_string_lossy()))
        .collect();
    on_disk.extend(
        [
            "prompts/swev_system.md",
            "prompts/swev_instance_protocol.md",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    assert_eq!(
        listed, on_disk,
        "MANIFEST.sha256 与盘上 .md 文件互为闭集（新增模板不登记=门红）"
    );
    // 每条登记值与文件实测 sha256 逐位一致（resolve 已核对其一，此处全量）。
    for rel in &listed {
        let file = repo_packages_root().join("sweng").join(rel);
        let data = std::fs::read(&file).unwrap_or_default();
        let sha = {
            use sha2::{Digest, Sha256};
            let mut out = String::new();
            for b in Sha256::digest(&data) {
                out.push_str(&format!("{b:02x}"));
            }
            out
        };
        assert!(
            manifest.contains(&sha),
            "{rel} 的实测 sha256 未在 MANIFEST 登记"
        );
    }
}

#[test]
fn load_swev_prompts_entrypoint_returns_frozen_bytes() {
    let (system, protocol) =
        prompts::load_swev_prompts(&repo_packages_root()).expect("装载化入口对仓库正本应成功");
    assert_eq!(system, SWEV_SYSTEM_PROMPT);
    assert_eq!(protocol, SWEV_INSTANCE_PROTOCOL);
}

#[test]
fn e2e_graph_template_refs_close_over_repo_prompts() {
    // 图正本的 context.sections[].template_ref 全部可解析（装载期门预演——
    // 编译器只记账字符串，真正核对在装订器；本测试钉死包内闭环）。
    let graph = std::fs::read_to_string(
        repo_packages_root()
            .join("sweng")
            .join("graphs")
            .join("fix_swe_task@1.0.afb"),
    )
    .expect("E2E 图正本应在盘（设计 §4.2 逐字节采用）");
    let refs: Vec<&str> = graph
        .lines()
        .filter_map(|l| {
            l.split("template_ref:")
                .nth(1)
                .map(|rest| rest.split(',').next().unwrap_or("").trim())
        })
        .filter(|r| !r.is_empty())
        .collect();
    assert_eq!(refs, vec!["pkg.sweng.swev_system@1"], "图恰 1 处模板引用");
    for r in refs {
        prompts::resolve_template(&repo_packages_root(), r)
            .unwrap_or_else(|e| panic!("图内 template_ref {r} 应解析: {e}"));
    }
}

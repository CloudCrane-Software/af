//! E1 缺口② + G3：任务包内容 hash 进提交校验（防重放语义内置编译链）。
//!
//! 语义（E1-报告缺口② + W2-C55）：任务包任何改动须清态换号重提——
//! manifest 在组包一刻留档 task.json/plan.json 内容 sha256；提交侧重算
//! 核对，不符即拒并点名行动指令。G3 起 manifest 缺席 = 显式
//! `ManifestVerification::Absent`（未核对状态，可机检、不与「核对过」
//! 同形）；要求核对的路径（af-order preflight）对缺席 fail-closed 拒绝。

use std::path::PathBuf;

use af_compiler::{
    build_package_manifest, verify_package_manifest, write_package_manifest, ManifestVerification,
    PACKAGE_MANIFEST_FILE, PACKAGE_MANIFEST_VERSION,
};

fn fixture() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "af-compiler-pkg-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("task.json"),
        "{\"task_id\":\"T-1\",\"task_book\":\"建\"}".as_bytes(),
    )
    .unwrap();
    std::fs::write(dir.join("plan.json"), "{\"v\":1,\"body\":{}}".as_bytes()).unwrap();
    dir
}

#[test]
fn manifest_write_then_verify_roundtrip() {
    let dir = fixture();
    let manifest = write_package_manifest(&dir, "plan.x@1.0+abc123").unwrap();
    assert_eq!(manifest.format_version, PACKAGE_MANIFEST_VERSION);
    assert_eq!(manifest.plan_id, "plan.x@1.0+abc123");
    assert!(dir.join(PACKAGE_MANIFEST_FILE).is_file());

    let ManifestVerification::Verified(verified) = verify_package_manifest(&dir).unwrap() else {
        panic!("manifest 在场且一致应 Verified");
    };
    assert_eq!(verified, manifest);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_rejects_tampered_task_or_plan_with_replay_contract_wording() {
    let dir = fixture();
    write_package_manifest(&dir, "plan.x@1.0+abc123").unwrap();

    // 判据面被改：fail-loud + 清态换号重提指令。
    std::fs::write(
        dir.join("task.json"),
        "{\"task_id\":\"T-1\",\"task_book\":\"改了\"}".as_bytes(),
    )
    .unwrap();
    let err = verify_package_manifest(&dir).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("task.json"), "应点名被改文件：{msg}");
    assert!(
        msg.contains("清态换号重提"),
        "防重放契约行动指令必须在文案面：{msg}"
    );

    // 计划面被改（预算帽偷换模型——E2G2 实证面）：同样拒。
    std::fs::write(
        dir.join("task.json"),
        "{\"task_id\":\"T-1\",\"task_book\":\"建\"}".as_bytes(),
    )
    .unwrap();
    std::fs::write(
        dir.join("plan.json"),
        "{\"v\":1,\"body\":{\"budget\":{}}}".as_bytes(),
    )
    .unwrap();
    let err = verify_package_manifest(&dir).unwrap_err();
    assert!(err.to_string().contains("plan.json"), "{err}");
    assert!(err.to_string().contains("清态换号重提"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_marks_absent_manifest_as_explicit_absent_state() {
    let dir = fixture();
    // G3（原 verify_passes_through_when_manifest_absent：断言 Ok(None)
    // 「缺席 = 不核对」——正是本批关掉的破窗口径）。缺席现返回显式
    // Absent：可机检、不报错、不可与「核对过且通过」（Verified 携留档，
    // roundtrip 测试覆盖）混同；要求核对的路径由调用方对 Absent
    // fail-closed（af-order preflight 集成测试覆盖）。
    assert_eq!(
        verify_package_manifest(&dir).unwrap(),
        ManifestVerification::Absent,
        "manifest 缺席 = 显式 Absent（未核对）状态，非静默通过"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_rejects_malformed_manifest() {
    let dir = fixture();
    // 形状完整但版本未知：拒绝式整份拒（版本先行于摘要核对）。
    std::fs::write(
        dir.join(PACKAGE_MANIFEST_FILE),
        "{\"format_version\":99,\"task_sha256\":\"a\",\"plan_sha256\":\"b\",\
         \"plan_id\":\"p\",\"compiler_version\":\"0\"}",
    )
    .unwrap();
    let err = verify_package_manifest(&dir).unwrap_err();
    assert!(
        err.to_string().contains("format_version=99"),
        "未知版本整份拒绝：{err}"
    );
    // 缺必填键 = 坏形（serde deny_unknown_fields 同口径）。
    let dir2 = fixture();
    std::fs::write(dir2.join(PACKAGE_MANIFEST_FILE), "{\"format_version\":1}").unwrap();
    let err2 = verify_package_manifest(&dir2).unwrap_err();
    assert!(
        err2.to_string().contains("坏形"),
        "缺键 manifest 应坏形拒绝：{err2}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn build_manifest_fails_loud_when_package_incomplete() {
    let dir = fixture();
    std::fs::remove_file(dir.join("plan.json")).unwrap();
    let err = build_package_manifest(&dir, "plan.x@1.0+abc123").unwrap_err();
    assert!(err.to_string().contains("plan.json"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manifest_serialization_is_deterministic() {
    let dir = fixture();
    let a = write_package_manifest(&dir, "plan.x@1.0+abc123").unwrap();
    let bytes_a = std::fs::read(dir.join(PACKAGE_MANIFEST_FILE)).unwrap();
    let b = write_package_manifest(&dir, "plan.x@1.0+abc123").unwrap();
    let bytes_b = std::fs::read(dir.join(PACKAGE_MANIFEST_FILE)).unwrap();
    assert_eq!(a, b);
    assert_eq!(bytes_a, bytes_b, "同内容同字节（确定性纪律）");
    let _ = std::fs::remove_dir_all(&dir);
}

// ============================================================
// G6：compiler_version 从只溯源到被核对
// ============================================================

/// 把已落盘 manifest 的 compiler_version 改写为 rogue 值（其余键不动）。
fn rewrite_rogue_version(dir: &std::path::Path, rogue: &str) {
    let path = dir.join(PACKAGE_MANIFEST_FILE);
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    m["compiler_version"] = serde_json::json!(rogue);
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();
}

/// G6：manifest 携带的 af-compiler 版本与当前构建不符 → 拒
/// （`CompilerVersionMismatch`，文案点名「重新组包」行动指令）。
#[test]
fn verify_rejects_compiler_version_mismatch() {
    let dir = fixture();
    write_package_manifest(&dir, "plan.x@1.0+abc123").unwrap();
    rewrite_rogue_version(&dir, "0.0.1-rogue");

    let err = verify_package_manifest(&dir).unwrap_err();
    assert_eq!(
        err,
        af_compiler::PackageError::CompilerVersionMismatch {
            recorded: "0.0.1-rogue".to_string(),
            actual: af_compiler::COMPILER_VERSION.to_string(),
        },
        "版本不符 = 专用错误变体（不留档值/判据值双点名）"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("compiler_version") && msg.contains("0.0.1-rogue"),
        "文案应点名核对面与 rogue 值：{msg}"
    );
    assert!(msg.contains("重新组包"), "行动指令在文案面：{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// G6 核对序钉死：内容被改 + 版本不符并存 → `ContentTampered` 先报
/// （既有错误优先级不变：坏形 → 内容改 → 编译器不符）。
#[test]
fn verify_keeps_content_tamper_precedence_over_version_mismatch() {
    let dir = fixture();
    write_package_manifest(&dir, "plan.x@1.0+abc123").unwrap();
    rewrite_rogue_version(&dir, "0.0.1-rogue");
    std::fs::write(
        dir.join("task.json"),
        "{\"task_id\":\"T-1\",\"task_book\":\"改了\"}".as_bytes(),
    )
    .unwrap();

    let err = verify_package_manifest(&dir).unwrap_err();
    assert!(
        matches!(err, af_compiler::PackageError::ContentTampered { .. }),
        "内容被改优先于版本不符：{err}"
    );
    assert!(err.to_string().contains("清态换号重提"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// G6 缺字段口径：manifest 缺 compiler_version 键（其余键齐备）→ 坏形拒
/// （schema 必填 + deny_unknown_fields——缺席即整份拒，不静默、不「未核对」）。
#[test]
fn verify_rejects_manifest_missing_compiler_version_field() {
    let dir = fixture();
    write_package_manifest(&dir, "plan.x@1.0+abc123").unwrap();
    let path = dir.join(PACKAGE_MANIFEST_FILE);
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    m.as_object_mut().unwrap().remove("compiler_version");
    std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let err = verify_package_manifest(&dir).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("坏形") && msg.contains("compiler_version"),
        "缺 compiler_version 键 = 坏形拒绝：{msg}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// G6 正门：当前构建产 manifest（组包即盖当前版本章）→ 核对过
/// （roundtrip 已覆盖 Verified；本测试把「版本面单独钉死」写成显式断言）。
#[test]
fn verify_passes_when_manifest_carries_current_compiler_version() {
    let dir = fixture();
    let manifest = write_package_manifest(&dir, "plan.x@1.0+abc123").unwrap();
    assert_eq!(manifest.compiler_version, af_compiler::COMPILER_VERSION);
    match verify_package_manifest(&dir).unwrap() {
        ManifestVerification::Verified(verified) => {
            assert_eq!(verified.compiler_version, af_compiler::COMPILER_VERSION);
        }
        other => panic!("当前构建产 manifest 应核对过：{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

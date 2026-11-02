//! E1 缺口②：任务包内容 hash 进提交校验（防重放语义内置编译链）。
//!
//! 语义（E1-报告缺口② + W2-C55）：任务包任何改动须清态换号重提——
//! manifest 在组包一刻留档 task.json/plan.json 内容 sha256；提交侧重算
//! 核对，不符即拒并点名行动指令。manifest 缺席 = 不核对（旧包同口径）。

use std::path::PathBuf;

use af_compiler::{
    build_package_manifest, verify_package_manifest, write_package_manifest, PACKAGE_MANIFEST_FILE,
    PACKAGE_MANIFEST_VERSION,
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

    let verified = verify_package_manifest(&dir)
        .unwrap()
        .expect("manifest 在场应返回留档");
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
fn verify_passes_through_when_manifest_absent() {
    let dir = fixture();
    assert_eq!(
        verify_package_manifest(&dir).unwrap(),
        None,
        "manifest 缺席 = 不核对（旧任务包零破窗）"
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

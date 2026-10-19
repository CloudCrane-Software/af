//! 错误分类学断言：每个坏样例各触发一次，断言错误码。
//! 样例文件由 ../make_bad_samples.py 生成（从真实 cue export 产物变异而来）。

use line_loader::{load_str, LoadError, SUPPORTED_SCHEMA_VERSION};

fn samples_dir() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // verify-load/
    p.join("samples")
}

fn load_sample(name: &str) -> Result<line_loader::LoadedBundle, LoadError> {
    let p = samples_dir().join(name);
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
    load_str(&text)
}

/// (坏样例文件, 期望错误码) 全表 —— 每类一个，逐一触发并断言
const BAD_SAMPLES: &[(&str, &str)] = &[
    ("bad-e0001-version-unsupported.json", "E0001"),
    ("bad-e0002-version-missing.json", "E0002"),
    ("bad-e0002-version-wrongtype.json", "E0002"),
    ("bad-e0003-envelope-unknown-field.json", "E0003"),
    ("bad-e1001-unknown-field.json", "E1001"),
    ("bad-e1002-type-mismatch.json", "E1002"),
    ("bad-e1003-missing-field.json", "E1003"),
    ("bad-e1004-unknown-variant.json", "E1004"),
    ("bad-e1007-kind-params-mismatch.json", "E1007"),
    ("bad-e1008-unknown-kind.json", "E1008"),
    ("bad-e2001-dangling-combo.json", "E2001"),
    ("bad-e2001-dangling-trigger.json", "E2001"),
    ("bad-e2001-dangling-required.json", "E2001"),
    ("bad-e2002-critical-below-warn.json", "E2002"),
    ("bad-e2002-escalation-chain-flat.json", "E2002"),
    ("bad-e2003-duplicate-id.json", "E2003"),
    ("bad-e2004-empty-notify.json", "E2004"),
];

#[test]
fn taxonomy_every_bad_sample_hits_its_code() {
    for (file, want) in BAD_SAMPLES {
        let err = load_sample(file).err().unwrap_or_else(|| panic!("{file}: 预期拒绝却装载成功"));
        assert_eq!(err.code(), *want, "{file}: 期望 {want}，得到 {} ({err})", err.code());
    }
}

#[test]
fn good_bundle_loads() {
    let b = load_sample("good-bundle.json").expect("正例必须通过");
    assert_eq!(b.schema_version, SUPPORTED_SCHEMA_VERSION);
    assert_eq!(b.line.id, "line-a");
    assert_eq!(b.rules.len(), 10);
    assert_eq!(b.line.tools.len(), 3);
    // serde default 落地：CUE *true | bool / *0 | int 的映射
    assert!(b.rules[0].enabled);
    assert_eq!(b.line.andon_rules[1].escalate_after_ms, 0); // 未写 escalateAfterMs → 默认 0
    assert!(b.line.andon_rules[1].escalate_to.is_empty()); // escalateTo? 缺省 → 空
}

#[test]
fn dangling_combo_error_names_the_target() {
    let err = load_sample("bad-e2001-dangling-combo.json").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("rule-ghost"), "错误信息要点名悬空目标: {msg}");
    assert!(msg.contains("/rules/"), "错误信息要带 JSON 路径: {msg}");
}

#[test]
fn malformed_json_is_type_mismatch_at_root() {
    let err = load_str("{not json").unwrap_err();
    assert_eq!(err.code(), "E1002");
    assert_eq!(err.kind_name(), "TYPE_MISMATCH");
}

// ============ ④ 版本策略测试 ============
// 策略：schemaVersion 是信封层非负整数（大版本）；在 payload 解析之前检查；
//       未知大版本一律拒绝（E0001），不做兼容尝试。

#[test]
fn version_gate_runs_before_payload_parse() {
    // payload 里再离谱的坏内容，也不该盖过版本错误 —— 版本门禁在最前
    let doc = r#"{"schemaVersion": 99, "line": 42, "rules": "nope"}"#;
    let err = load_str(doc).unwrap_err();
    assert_eq!(err.code(), "E0001", "版本门禁必须先于 payload 解析");
}

#[test]
fn version_zero_and_future_are_rejected() {
    for sv in [0u64, 2, 3, 42, u64::MAX] {
        let doc = format!(r#"{{"schemaVersion": {sv}, "line": null, "rules": []}}"#);
        let err = load_str(&doc).unwrap_err();
        assert_eq!(err.code(), "E0001", "schemaVersion={sv} 应拒绝");
        assert!(err.to_string().contains(&sv.to_string()), "错误信息应回显收到的版本");
    }
}

#[test]
fn version_must_be_non_negative_integer() {
    for bad in [r#""1""#, "-1", "1.5", "true", "null", "{}"] {
        let doc = format!(r#"{{"schemaVersion": {bad}, "line": null, "rules": []}}"#);
        let err = load_str(&doc).unwrap_err();
        assert_eq!(err.code(), "E0002", "schemaVersion={bad} 应判 E0002");
    }
}

#[test]
fn missing_version_is_rejected_even_with_valid_payload() {
    // 没有 schemaVersion 的文档即使 payload 完全合法也拒绝（不允许"猜版本"）
    let p = samples_dir().join("good-bundle.json");
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    v.as_object_mut().unwrap().remove("schemaVersion");
    let err = load_str(&v.to_string()).unwrap_err();
    assert_eq!(err.code(), "E0002");
}

#[test]
fn only_the_three_envelope_keys_are_allowed() {
    let doc = r#"{"schemaVersion": 1, "line": null, "rules": [], "rules_v2": []}"#;
    let err = load_str(doc).unwrap_err();
    assert_eq!(err.code(), "E0003");
}

// ============ E9xxx 部署期 CUE 阶段：真实触发 cue 并归类 ============
// 装载器自身不产出 E9001/E9002；管线工具（cue vet/export 包装层）用同一枚举上报。
// 这里真实运行 cue 二进制，验证触发条件与归类路径。

fn cue_bin() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("CUE_BIN") {
        return p.into();
    }
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // verify-load/
    p.pop(); // poc-cue/
    p.join("bin").join("cue")
}

fn run_cue(args: &[&str]) -> (i32, String) {
    let out = std::process::Command::new(cue_bin()).args(args).output().expect("运行 cue 失败");
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn cue_stage_default_marker_missing_is_e9001() {
    let dir = samples_dir().join("cue");
    // a) 无默认（连 *标记 都没有）→ export incomplete，exit 1
    let (code, stderr) = run_cue(&["export", &dir.join("marker-missing.cue").to_string_lossy()]);
    assert_eq!(code, 1);
    assert!(stderr.contains("incomplete"), "stderr={stderr}");
    // b) 忘了 * 标记（30 | int 裸具体值被吸收）→ 同样 incomplete
    let (code, stderr) = run_cue(&["export", &dir.join("marker-lost.cue").to_string_lossy()]);
    assert_eq!(code, 1);
    assert!(stderr.contains("incomplete"), "stderr={stderr}");
    // c) 裸具体值 + L4 覆盖 → conflicting（不是覆盖）
    let (code, stderr) = run_cue(&[
        "export",
        &dir.join("marker-bare.cue").to_string_lossy(),
        &dir.join("marker-override.cue").to_string_lossy(),
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("conflicting values"), "stderr={stderr}");
    // 归类：管线把这三类 cue 失败统一报 E9001
    let e = LoadError::DefaultMarkerMissing { cue_stderr: stderr };
    assert_eq!(e.code(), "E9001");
}

#[test]
fn cue_stage_ok_marker_exports_and_overrides_cleanly() {
    let dir = samples_dir().join("cue");
    let (code, _stderr) = run_cue(&["export", &dir.join("marker-ok.cue").to_string_lossy()]);
    assert_eq!(code, 0);
    let out = std::process::Command::new(cue_bin())
        .args(["export", &dir.join("marker-ok.cue").to_string_lossy(),
               &dir.join("marker-override.cue").to_string_lossy()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("15"), "L4 覆盖应生效: {s}");
}

#[test]
fn cue_vet_rejection_maps_to_e9002() {
    // cue vet 对 closed schema 未知字段的拒绝（管线第一道闸）→ E9002
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf();
    let (code, stderr) = run_cue(&[
        "vet",
        &base.join("line").join("line.cue").to_string_lossy(),
        &base.join("line").join("line-bad.cue").to_string_lossy(),
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("field not allowed"), "stderr={stderr}");
    let e = LoadError::CueStageRejected { cue_stderr: stderr };
    assert_eq!(e.code(), "E9002");
}

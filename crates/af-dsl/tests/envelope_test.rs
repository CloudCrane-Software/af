//! 信封层验收测试（E0001-E0004）：拒绝式版本信封——version 非 1 整份拒绝、
//! 不解析 body（承 af-harness decode_spec / af-dspy-ir Program.version 先例）。

use af_dsl::{load_str, LoadError};

fn envelope_codes(src: &str) -> Vec<&'static str> {
    match load_str(src) {
        Err(LoadError::Envelope(r)) => r.codes(),
        other => panic!("期望信封层拒绝，实得 {other:?}"),
    }
}

#[test]
fn missing_version_key_is_e0001() {
    let src = include_str!("../testdata/reject/env_e0001_missing.yaml");
    assert_eq!(envelope_codes(src), ["E0001"]);
}

#[test]
fn non_mapping_root_is_e0001() {
    let src = include_str!("../testdata/reject/env_e0001_not_mapping.yaml");
    assert_eq!(envelope_codes(src), ["E0001"]);
}

#[test]
fn yaml_syntax_error_is_e0001() {
    assert_eq!(envelope_codes("version: 1\nmeta: [unclosed\n"), ["E0001"]);
}

#[test]
fn string_version_is_e0002() {
    let src = include_str!("../testdata/reject/env_e0002_string.yaml");
    assert_eq!(envelope_codes(src), ["E0002"]);
}

#[test]
fn float_version_is_e0002() {
    let src = include_str!("../testdata/reject/env_e0002_float.yaml");
    assert_eq!(envelope_codes(src), ["E0002"]);
}

#[test]
fn bool_version_is_e0002() {
    assert_eq!(envelope_codes("version: true\n"), ["E0002"]);
}

#[test]
fn version_two_rejects_without_parsing_body() {
    // body 是合法 YAML 但形状全坏：若误解析 body 会漏出 E1xxx/E0004——
    // 码集必须精确等于 [E0003]，钉住「整份拒绝、不解析 body」。
    let src = include_str!("../testdata/reject/env_e0003_not_one.yaml");
    assert_eq!(envelope_codes(src), ["E0003"]);
}

#[test]
fn negative_version_is_e0003() {
    assert_eq!(envelope_codes("version: -1\n"), ["E0003"]);
}

#[test]
fn unknown_top_key_is_e0004() {
    let src = include_str!("../testdata/reject/env_e0004_unknown_key.yaml");
    assert_eq!(envelope_codes(src), ["E0004"]);
}

#[test]
fn multiple_unknown_top_keys_collect_all() {
    assert_eq!(envelope_codes("version: 1\nb1: 1\nb2: 2\n"), ["E0004", "E0004"]);
}

#[test]
fn valid_envelope_proceeds_to_shape_layer() {
    // 信封合法、body 缺 meta → 形状层 E1002（不是信封层），钉住信封过后解析 body。
    match load_str("version: 1\n") {
        Err(LoadError::Shape(r)) => assert_eq!(r.codes(), ["E1002"]),
        other => panic!("期望形状层拒绝，实得 {other:?}"),
    }
}

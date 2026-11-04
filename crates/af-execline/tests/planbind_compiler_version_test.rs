//! G6：计划面 `compiled_from.compiler_version` 从只溯源到被核对——
//! planbind（执行面消费唯一正本，裁决9）装订期与当前构建链接的 af-compiler
//! （`af_compiler::COMPILER_VERSION` 判据正本）核对，不符即拒。
//!
//! 三口径（与任务书测试判据一一对应）：
//! - 版本相符（真编译器全链产计划）→ 装订过；
//! - 版本不符（改 compiled_from.compiler_version 为 rogue 值）→ EX4005
//!   `PlanBindMismatch` 拒，文案点名「不是当前编译器产的」；
//! - 缺版本字段（删 compiled_from.compiler_version 键）→ decode 拒
//!   （`CompiledFrom` schema 必填 + deny_unknown_fields——坏形即整份拒，
//!   不静默、不「显式未核对」）。
//!
//! 判据来源登记：不用仓内钉值（会与实际链接的 crate 漂移成第二真相）、
//! 不用消费 crate 自身 `env!`（af-execline 的版本 ≠ 编译器的版本）——
//! 用链接进本测试二进制的 af-compiler 版本，与 registry_digest 先例同构
//! （编译期值随计划携带，消费侧以本地正本对照）。

use std::path::{Path, PathBuf};

use af_execline::error::ErrorCode;
use af_execline::{planbind_bind, BindExpectations};

fn repo_packages() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn compile_sop_full() -> String {
    let src = std::fs::read_to_string(
        repo_packages()
            .join("sweng")
            .join("graphs")
            .join("sop_full@1.1.afb"),
    )
    .expect("sop_full@1.1.afb 仓库正本应在盘");
    let doc = af_dsl::load_str(&src).expect("sop_full 应装载");
    let verdict = af_dsl::validate(&doc);
    assert!(verdict.is_ok(), "sop_full 应零违例：{verdict}");
    let reg = af_compiler::ComponentRegistry::load(repo_packages()).expect("仓库注册表应装载");
    af_compiler::plan_json(&doc, &reg).expect("sop_full 计划应生成")
}

fn bind(plan_src: &str) -> Result<af_execline::PlanBind, af_execline::error::ExeclineError> {
    planbind_bind(
        plan_src,
        &repo_packages(),
        &[],
        &BindExpectations::default(),
    )
}

/// 相符 ⇒ 过：真编译器全链产计划（盖当前版本章）装订通过。
#[test]
fn planbind_passes_when_plan_carries_current_compiler_version() {
    let plan_src = compile_sop_full();
    let bound = bind(&plan_src).expect("当前构建产计划应装订过");
    assert_eq!(
        bound.plan.body.compiled_from.compiler_version,
        af_compiler::COMPILER_VERSION,
        "真编译产物携带当前版本（判据正本同一值）"
    );
}

/// 不符 ⇒ 拒：compiled_from.compiler_version 改 rogue 值 → EX4005 拒，
/// 文案双点名（计划携带值 ≠ 当前构建值）并给「重新编译组包」行动指令。
#[test]
fn planbind_rejects_plan_from_different_compiler_version() {
    let plan_src = compile_sop_full();
    let mut plan: serde_json::Value = serde_json::from_str(&plan_src).unwrap();
    plan["body"]["compiled_from"]["compiler_version"] = serde_json::json!("0.0.1-rogue");

    let err = bind(&serde_json::to_string(&plan).unwrap())
        .expect_err("版本不符的计划应拒装订（G6 前：照过——破窗已实证）");
    assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{err}");
    let msg = err.to_string();
    assert!(
        msg.contains("compiler_version 不符") && msg.contains("0.0.1-rogue"),
        "文案应点名核对面与 rogue 值：{msg}"
    );
    assert!(
        msg.contains(af_compiler::COMPILER_VERSION),
        "判据值（当前构建 af-compiler）应在文案面：{msg}"
    );
    assert!(msg.contains("重新编译组包"), "行动指令在文案面：{msg}");
}

/// 缺字段 ⇒ 拒（坏形）：删 compiled_from.compiler_version 键 → decode 拒
/// （`CompiledFrom` 必填；缺席即整份拒——G6 前后同口径，此处钉死防回退）。
#[test]
fn planbind_rejects_plan_missing_compiler_version_field() {
    let plan_src = compile_sop_full();
    let mut plan: serde_json::Value = serde_json::from_str(&plan_src).unwrap();
    plan["body"]["compiled_from"]
        .as_object_mut()
        .unwrap()
        .remove("compiler_version");

    let err = bind(&serde_json::to_string(&plan).unwrap())
        .expect_err("缺 compiler_version 键的计划应坏形拒");
    assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{err}");
    assert!(
        err.to_string().contains("missing field `compiler_version`"),
        "缺键 = decode 坏形拒绝（非静默、非未核对）：{err}"
    );
}

//! 判分可信度协议验收测试（S17 · st-b-C022，债 D-57，设计稿 §七.4）：
//! `af_execline::pkg::reliab` 的三值判定 + 多评委校准 + 判分污染审计。
//!
//! 承 `pkg_sweng.rs` 先例的纯函数面断言风格（无容器、无子进程、无 LLM）。
//! 覆盖：
//! - `Decision` 三值 + serde snake_case + `from_resolved` 保 null 语义；
//! - `audit_contamination` / `judge_vote` 污染检测各分支；
//! - `calibrate` 全分支（含供给面失败透传 null / 全污染判 undecidable）；
//! - **污染注入负例**（→undecidable / →flip，与 `naive_majority` 对照）；
//! - 反混淆不变量：`None`（供给面失败）与 `Some(Undecidable)` 永不等同。

use af_execline::pkg::reliab;
use af_execline::pkg::reliab::{Decision, JudgeVote};
use serde_json;

/// 禁泄漏标样（v1 工作定义：金标 patch 指纹，干净判分里绝不该出现）。
const FORBIDDEN: &[&str] = &["GOLD_PATCH"];

fn vote(judge_id: &str, resolved: Option<bool>, contaminated: bool) -> JudgeVote {
    JudgeVote {
        judge_id: judge_id.to_string(),
        resolved,
        contaminated,
    }
}

// ============================================================
// Decision 三值 + serde + from_resolved（保 null 语义）
// ============================================================

#[test]
fn decision_three_values_and_as_str_snake_case() {
    assert_eq!(Decision::Pass.as_str(), "pass");
    assert_eq!(Decision::Fail.as_str(), "fail");
    assert_eq!(Decision::Undecidable.as_str(), "undecidable");
    // serde rename_all=snake_case（与 af_eval::GateVerdict 同款序列化口径）。
    assert_eq!(
        serde_json::to_string(&Decision::Undecidable).unwrap(),
        "\"undecidable\""
    );
    let back: Decision = serde_json::from_str("\"pass\"").unwrap();
    assert_eq!(back, Decision::Pass);
}

#[test]
fn decision_from_resolved_is_lossless_and_never_maps_none_to_undecidable() {
    // 旧二值+null 契约 → 三值契约的唯一安全搬运方向（设计稿 §七.4 契约立场）：
    // Some(true)=Pass / Some(false)=Fail / **None 仍是 None（供给面失败）**。
    assert_eq!(Decision::from_resolved(Some(true)), Some(Decision::Pass));
    assert_eq!(Decision::from_resolved(Some(false)), Some(Decision::Fail));
    // 反混淆锚：None 绝不变 Undecidable——供给面失败 ≠ 不可判定。
    assert_eq!(Decision::from_resolved(None), None);
    assert_ne!(Decision::from_resolved(None), Some(Decision::Undecidable));
}

// ============================================================
// 判分污染审计（audit_contamination / judge_vote）
// ============================================================

#[test]
fn audit_contamination_clean_when_no_marker_present() {
    let report = reliab::audit_contamination("resolved: all f2p pass, patch complete", FORBIDDEN);
    assert!(!report.contaminated());
    assert!(report.markers.is_empty());
}

#[test]
fn audit_contamination_flags_leakage_marker() {
    let report = reliab::audit_contamination("resolved: reproduces GOLD_PATCH line-for-line", FORBIDDEN);
    assert!(report.contaminated());
    assert_eq!(report.markers, vec!["GOLD_PATCH".to_string()]);
}

#[test]
fn audit_contamination_skips_empty_marker_and_dedupes() {
    // 空串标样不得误命中全文本；重复标样去重（文本须真含 GOLD_PATCH 才命中）。
    let report = reliab::audit_contamination("resolved: leaked GOLD_PATCH here", &["", "GOLD_PATCH", "GOLD_PATCH"]);
    assert_eq!(report.markers, vec!["GOLD_PATCH".to_string()]);
    // 无禁标样集 ⇒ 恒干净。
    let empty = reliab::audit_contamination("anything at all", &[]);
    assert!(!empty.contaminated());
}

#[test]
fn judge_vote_inlines_contamination_flag() {
    let clean = reliab::judge_vote("j1", Some(true), "resolved: all f2p pass", FORBIDDEN);
    assert!(!clean.contaminated);
    assert_eq!(clean.resolved, Some(true));
    let dirty = reliab::judge_vote("j2", Some(true), "resolved: saw GOLD_PATCH", FORBIDDEN);
    assert!(dirty.contaminated);
    assert_eq!(dirty.judge_id, "j2");
}

// ============================================================
// calibrate 全分支
// ============================================================

#[test]
fn calibrate_unanimous_clean_pass_and_fail() {
    let pass = vec![vote("a", Some(true), false), vote("b", Some(true), false)];
    assert_eq!(reliab::calibrate(&pass), Some(Decision::Pass));
    let fail = vec![vote("a", Some(false), false), vote("b", Some(false), false)];
    assert_eq!(reliab::calibrate(&fail), Some(Decision::Fail));
}

#[test]
fn calibrate_clean_disagreement_is_undecidable() {
    // 干净票分歧（unanimity 口径）⇒ 不可信 ⇒ Undecidable。
    let split = vec![vote("a", Some(true), false), vote("b", Some(false), false)];
    assert_eq!(reliab::calibrate(&split), Some(Decision::Undecidable));
}

#[test]
fn calibrate_discards_contaminated_votes() {
    // 被污染的 pass 票不计票；仅剩干净 fail ⇒ Fail。
    let votes = vec![
        vote("a", Some(true), true),
        vote("b", Some(true), true),
        vote("c", Some(false), false),
    ];
    assert_eq!(reliab::calibrate(&votes), Some(Decision::Fail));
}

#[test]
fn calibrate_ignores_supply_side_failed_votes_but_keeps_clean_consensus() {
    // 供给面失败的票（resolved=None）不是结论，不计票；干净多数仍可判。
    let votes = vec![
        vote("a", None, false), // 供给面失败
        vote("b", Some(true), false),
        vote("c", Some(true), false),
    ];
    assert_eq!(reliab::calibrate(&votes), Some(Decision::Pass));
}

#[test]
fn calibrate_all_supply_side_failure_transparents_none_not_undecidable() {
    // 反混淆锚①：无一票有结论（全供给面失败）⇒ None（透传 null），
    // **不是** Some(Undecidable)。sweng.rs:705-706 null=供给面失败语义不被篡改。
    let all_none = vec![vote("a", None, false), vote("b", None, false)];
    assert_eq!(reliab::calibrate(&all_none), None);
    assert_ne!(reliab::calibrate(&all_none), Some(Decision::Undecidable));
    // 空票组同样透传 None。
    assert_eq!(reliab::calibrate(&[]), None);
}

#[test]
fn calibrate_all_contaminated_is_undecidable_not_none() {
    // 反混淆锚②：全体有结论但全被污染 ⇒ Some(Undecidable)（判分了但不可采信），
    // **不是** None。这一格与上一格互为补，钉死 Undecidable ≠ 供给面失败。
    let all_dirty = vec![
        vote("a", Some(true), true),
        vote("b", Some(false), true),
    ];
    assert_eq!(reliab::calibrate(&all_dirty), Some(Decision::Undecidable));
    assert_ne!(reliab::calibrate(&all_dirty), None);
}

// ============================================================
// 污染注入负例（st-b-C022 claim：污染制造方向性假结论，
// 门禁须能判 undecidable 或翻转结论）
// ============================================================

#[test]
fn negative_fixture_contamination_flips_false_pass_to_fail() {
    // 负例·翻转：两条泄漏票（判定文本含 GOLD_PATCH）伪造出 pass 多数，
    // 一条干净 fail 票被淹没。朴素多数（污染无感）= 方向性假结论 Pass；
    // 污染感知校准丢弃泄漏票 ⇒ 翻转为 Fail。
    let votes = vec![
        reliab::judge_vote("j1", Some(true), "resolved: reproduced GOLD_PATCH exactly", FORBIDDEN),
        reliab::judge_vote("j2", Some(true), "resolved: matches GOLD_PATCH line-for-line", FORBIDDEN),
        reliab::judge_vote("j3", Some(false), "resolved: f2p still failing, patch incomplete", FORBIDDEN),
    ];
    assert_eq!(reliab::naive_majority(&votes), Some(Decision::Pass), "朴素多数被污染骗成假 Pass");
    assert_eq!(reliab::calibrate(&votes), Some(Decision::Fail), "污染审计丢弃泄漏票 ⇒ 翻转为 Fail");
    assert_ne!(reliab::calibrate(&votes), reliab::naive_majority(&votes));
}

#[test]
fn negative_fixture_contamination_downgrades_false_pass_to_undecidable() {
    // 负例·undecidable：一条泄漏 pass 票 tipping 出假共识；干净票实际
    // pass/fail 分歧。朴素多数 = 假 Pass；校准后干净票分歧 ⇒ Undecidable
    // （拒判，不制造方向性假结论）。
    let votes = vec![
        reliab::judge_vote("j1", Some(true), "resolved: all f2p pass", FORBIDDEN),
        reliab::judge_vote("j2", Some(true), "resolved: reproduces GOLD_PATCH", FORBIDDEN),
        reliab::judge_vote("j3", Some(false), "resolved: p2p regression detected", FORBIDDEN),
    ];
    assert_eq!(reliab::naive_majority(&votes), Some(Decision::Pass), "朴素多数被污染 tipping 成假 Pass");
    assert_eq!(reliab::calibrate(&votes), Some(Decision::Undecidable), "干净票分歧 ⇒ 拒判 Undecidable");
}

// ============================================================
// naive_majority 对照臂（污染无感）
// ============================================================

#[test]
fn naive_majority_counts_all_verdicts_and_transparents_none() {
    // 对照臂：计所有有结论票（含污染）；无一票有结论 ⇒ None（不伪造结论）。
    let votes = vec![
        vote("a", Some(true), true),
        vote("b", Some(false), false),
        vote("c", Some(false), false),
    ];
    assert_eq!(reliab::naive_majority(&votes), Some(Decision::Fail));
    let all_none = vec![vote("a", None, false), vote("b", None, false)];
    assert_eq!(reliab::naive_majority(&all_none), None);
}

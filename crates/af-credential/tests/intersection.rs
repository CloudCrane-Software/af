//! 交集判定器 4 用例：有效权限 = Cedar allow ∩ 凭证 capability。
//!
//! 矩阵：
//! | Cedar | 凭证 | 期望 |
//! |-------|------|------|
//! | allow | allow | 放行 |
//! | allow | 无该能力（衰减后被封） | 拒 |
//! | deny  | 有能力 | 拒 |
//! | deny  | deny  | 拒 |

use af_credential::{
    attenuate, evaluate, issue_root, serialize, AccessQuery, ChildSpec, RootSpec,
};

fn root_spec() -> RootSpec {
    RootSpec {
        tools: vec!["git".into(), "test".into()],
        namespace: "line-a".into(),
        budget_cents: 1000,
    }
}

fn child_spec() -> ChildSpec {
    ChildSpec { tools: vec!["test".into()], spend_cap_cents: 500 }
}

fn q(role: &str, tool: &str) -> AccessQuery {
    AccessQuery {
        role: role.into(),
        tool: tool.into(),
        namespace: "line-a".into(),
        spend_cents: 100,
    }
}

/// 用例 1：双方都允许 → 放行。
/// reviewer + test：Cedar p1 放行，根凭证携带 test。
#[test]
fn case1_both_allow_pass() {
    let root = issue_root(&root_spec()).unwrap();
    let v = evaluate(&root.token, &q("reviewer", "test")).unwrap();
    assert!(v.cedar_allow, "{}", v.reason);
    assert!(v.biscuit_allow, "{}", v.reason);
    assert!(v.allow, "{}", v.reason);
}

/// 用例 2：Cedar 允许但凭证（衰减后）无该能力 → 拒。
/// builder + git：Cedar p2 放行；但子 agent 凭证的衰减块把 request_tool 封到 ["test"]。
#[test]
fn case2_cedar_allows_but_token_lacks_deny() {
    let root = issue_root(&root_spec()).unwrap();
    let child = attenuate(&root.token, &child_spec()).unwrap();
    let token = serialize(&child).unwrap();
    let child = af_credential::deserialize_verify(&token, &root.keypair.public()).unwrap();

    let v = evaluate(&child, &q("builder", "git")).unwrap();
    assert!(v.cedar_allow, "{}", v.reason);
    assert!(!v.biscuit_allow, "{}", v.reason);
    assert!(!v.allow, "{}", v.reason);
    // 有意思的点：git 事实其实还在 token 里（单调性），封死它的是衰减 check
    assert!(v.token_carries_capability, "{}", v.reason);

    // 对照：同一查询打在根凭证上是过的（证明拒的原因确实是衰减）
    let v_root = evaluate(&root.token, &q("builder", "git")).unwrap();
    assert!(v_root.allow, "{}", v_root.reason);
}

/// 用例 3：凭证有但 Cedar 拒 → 拒。
/// reviewer + git：根凭证携带 git，但 Cedar 没有 reviewer→git 的 permit。
#[test]
fn case3_token_has_but_cedar_denies() {
    let root = issue_root(&root_spec()).unwrap();
    let v = evaluate(&root.token, &q("reviewer", "git")).unwrap();
    assert!(!v.cedar_allow, "{}", v.reason);
    assert!(v.biscuit_allow, "{}", v.reason);
    assert!(v.token_carries_capability, "{}", v.reason);
    assert!(!v.allow, "{}", v.reason);
}

/// 用例 4：双方都无 → 拒。
/// reviewer + deploy：不在凭证白名单，Cedar 也没有对应 permit。
#[test]
fn case4_neither_allows_deny() {
    let root = issue_root(&root_spec()).unwrap();
    let v = evaluate(&root.token, &q("reviewer", "deploy")).unwrap();
    assert!(!v.cedar_allow, "{}", v.reason);
    assert!(!v.biscuit_allow, "{}", v.reason);
    assert!(!v.allow, "{}", v.reason);
}

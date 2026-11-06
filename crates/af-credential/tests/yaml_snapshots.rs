//! 策略求交与凭证签发的 YAML 快照。
//!
//! 快照只收确定性字段。密钥与签名每次签发都不同，不进快照。
//! 语义与 RESULT.md 一致：`Cents` = i64 美分；衰减只加 check、不删事实；
//! `FailedLogic` 是拒绝，不是真错误。

use af_credential::{
    attenuate, authorize, denial, deserialize_verify, evaluate, inspect, issue_root, serialize,
    AccessQuery, Cents, ChildSpec, Denial, DenialBody, DenialCheck, DenialEffect, DenialPolicy,
    RootSpec, TokenCaps, Verdict, CEDAR_POLICIES, F_BUDGET_CENTS, F_NAMESPACE, F_REQUEST_NS,
    F_REQUEST_TOOL, F_SPEND_CENTS, F_TOOL,
};
use serde::Serialize;

fn root_spec() -> RootSpec {
    RootSpec {
        tools: vec!["git".into(), "test".into()],
        namespace: "line-a".into(),
        budget_cents: 1000,
    }
}

fn child_spec() -> ChildSpec {
    ChildSpec {
        tools: vec!["test".into()],
        spend_cap_cents: 500,
    }
}

macro_rules! roundtrip {
    ($token:expr, $pk:expr) => {{
        let bytes = serialize($token).unwrap();
        deserialize_verify(&bytes, $pk).unwrap()
    }};
}

#[derive(Serialize)]
struct VerdictView {
    allow: bool,
    cedar_allow: bool,
    biscuit_allow: bool,
    token_carries_capability: bool,
    reason: String,
}

impl From<&Verdict> for VerdictView {
    fn from(v: &Verdict) -> Self {
        Self {
            allow: v.allow,
            cedar_allow: v.cedar_allow,
            biscuit_allow: v.biscuit_allow,
            token_carries_capability: v.token_carries_capability,
            reason: v.reason.clone(),
        }
    }
}

#[derive(Serialize)]
struct ProbeView {
    name: &'static str,
    role: &'static str,
    tool: &'static str,
    namespace: &'static str,
    spend_cents: Cents,
    root: VerdictView,
    attenuated: VerdictView,
}

#[derive(Serialize)]
struct MinimumGrant {
    role: &'static str,
    tool: &'static str,
    namespace: &'static str,
    max_spend_cents: Cents,
}

fn sorted_tools(caps: &TokenCaps) -> Vec<String> {
    let mut tools: Vec<String> = caps.tools.iter().cloned().collect();
    tools.sort();
    tools
}

fn tool_intersection(left: &[String], right: &[String]) -> Vec<String> {
    let mut tools: Vec<String> = left
        .iter()
        .filter(|tool| right.iter().any(|other| other == *tool))
        .cloned()
        .collect();
    tools.sort();
    tools
}

/// 两策略求交（Cedar permit ∩ 衰减后的 biscuit check）的最小权限。
#[test]
fn two_policy_intersection_minimum() {
    let spec = root_spec();
    let child = child_spec();
    let root = issue_root(&spec).unwrap();
    let root_token = roundtrip!(&root.token, &root.keypair.public());
    let attenuated = attenuate(&root_token, &child).unwrap();
    let attenuated = roundtrip!(&attenuated, &root.keypair.public());
    let facts = inspect(&attenuated).unwrap();

    let probes_in = [
        ("reviewer_test_inside_cap", "reviewer", "test", 100),
        ("reviewer_test_at_child_cap", "reviewer", "test", 500),
        ("reviewer_test_just_over_child_cap", "reviewer", "test", 501),
        ("reviewer_test_at_root_budget", "reviewer", "test", 1000),
        ("reviewer_test_over_root_budget", "reviewer", "test", 1001),
        ("builder_git", "builder", "git", 100),
        ("reviewer_git", "reviewer", "git", 100),
        ("builder_test", "builder", "test", 100),
        ("reviewer_deploy", "reviewer", "deploy", 100),
    ];

    let mut probes = Vec::with_capacity(probes_in.len());
    for (name, role, tool, spend_cents) in probes_in {
        let query = AccessQuery {
            role: role.into(),
            tool: tool.into(),
            namespace: spec.namespace.clone(),
            spend_cents,
        };
        let root_verdict = evaluate(&root_token, &query).unwrap();
        let attenuated_verdict = evaluate(&attenuated, &query).unwrap();
        probes.push(ProbeView {
            name,
            role,
            tool,
            namespace: "line-a",
            spend_cents,
            root: VerdictView::from(&root_verdict),
            attenuated: VerdictView::from(&attenuated_verdict),
        });
    }

    let mut minimum = Vec::<MinimumGrant>::new();
    for probe in &probes {
        if !probe.attenuated.allow {
            continue;
        }
        if let Some(grant) = minimum
            .iter_mut()
            .find(|grant| grant.role == probe.role && grant.tool == probe.tool)
        {
            if probe.spend_cents > grant.max_spend_cents {
                grant.max_spend_cents = probe.spend_cents;
            }
        } else {
            minimum.push(MinimumGrant {
                role: probe.role,
                tool: probe.tool,
                namespace: probe.namespace,
                max_spend_cents: probe.spend_cents,
            });
        }
    }

    let declared_spend_cap = spec.budget_cents.min(child.spend_cap_cents);
    let observed_max = minimum.iter().map(|grant| grant.max_spend_cents).max();
    assert_eq!(observed_max, Some(declared_spend_cap));
    assert_eq!(minimum.len(), 1);
    assert_eq!(minimum[0].role, "reviewer");
    assert_eq!(minimum[0].tool, "test");

    let builder_git = probes
        .iter()
        .find(|probe| probe.name == "builder_git")
        .unwrap();
    assert!(builder_git.root.allow, "{}", builder_git.root.reason);
    assert!(
        !builder_git.attenuated.allow,
        "{}",
        builder_git.attenuated.reason
    );
    assert!(
        builder_git.attenuated.token_carries_capability,
        "git 事实仍在，封死它的是衰减 check：{}",
        builder_git.attenuated.reason
    );

    let over_child = probes
        .iter()
        .find(|probe| probe.name == "reviewer_test_just_over_child_cap")
        .unwrap();
    assert!(over_child.root.allow, "{}", over_child.root.reason);
    assert!(
        !over_child.attenuated.allow,
        "{}",
        over_child.attenuated.reason
    );

    let tools = tool_intersection(&spec.tools, &child.tools);
    assert_eq!(tools, vec!["test".to_string()]);
    let fact_tools = sorted_tools(&facts);
    assert!(fact_tools.iter().any(|tool| tool == "git"));
    assert!(fact_tools.iter().any(|tool| tool == "test"));

    let snapshot = IntersectionSnapshot {
        kind: "two_policy_intersection_minimum",
        cedar_policies: CEDAR_POLICIES,
        root_tools: spec.tools.clone(),
        root_namespace: spec.namespace.clone(),
        root_budget_cents: spec.budget_cents,
        child_tools: child.tools.clone(),
        child_spend_cap_cents: child.spend_cap_cents,
        tool_intersection: tools,
        declared_spend_cap_cents: declared_spend_cap,
        attenuated_facts_tools: fact_tools,
        attenuated_namespace: facts.namespace.clone(),
        attenuated_budget_cents: facts.budget_cents,
        probes,
        minimum_grants: minimum,
    };
    insta::assert_yaml_snapshot!(snapshot);
}

#[derive(Serialize)]
struct IntersectionSnapshot {
    kind: &'static str,
    cedar_policies: &'static str,
    root_tools: Vec<String>,
    root_namespace: String,
    root_budget_cents: Cents,
    child_tools: Vec<String>,
    child_spend_cap_cents: Cents,
    tool_intersection: Vec<String>,
    declared_spend_cap_cents: Cents,
    attenuated_facts_tools: Vec<String>,
    attenuated_namespace: Option<String>,
    attenuated_budget_cents: Option<Cents>,
    probes: Vec<ProbeView>,
    minimum_grants: Vec<MinimumGrant>,
}

/// 金额词表 `Cents`（i64 美分）签发后、经序列化往返仍在载荷里的事实与 check。
#[test]
fn cents_issuance_payload() {
    let spec = root_spec();
    let child = child_spec();
    let root = issue_root(&spec).unwrap();
    let root_bytes = serialize(&root.token).unwrap();
    let root_again = issue_root(&spec).unwrap();
    let root_bytes_again = serialize(&root_again.token).unwrap();
    assert_eq!(
        root_bytes.len(),
        root_bytes_again.len(),
        "同一载荷的线上长度不随密钥变化"
    );
    let root_token = deserialize_verify(&root_bytes, &root.keypair.public()).unwrap();
    let root_caps = inspect(&root_token).unwrap();

    let attenuated = attenuate(&root_token, &child).unwrap();
    let child_bytes = serialize(&attenuated).unwrap();
    let attenuated_again = attenuate(&root_again.token, &child).unwrap();
    let child_bytes_again = serialize(&attenuated_again).unwrap();
    assert_eq!(child_bytes.len(), child_bytes_again.len());
    let child_token = deserialize_verify(&child_bytes, &root.keypair.public()).unwrap();
    let child_caps = inspect(&child_token).unwrap();

    assert_eq!(root_caps.budget_cents, Some(spec.budget_cents));
    assert_eq!(root_caps.block_count, 1);
    assert_eq!(child_caps.block_count, 2);
    assert_eq!(child_caps.block_sources[0], root_caps.block_sources[0]);
    assert_eq!(child_caps.budget_cents, Some(spec.budget_cents));
    assert!(
        root_caps.block_sources[0].contains("budget_cents(1000)"),
        "{}",
        root_caps.block_sources[0]
    );
    assert!(
        child_caps.block_sources[1].contains("500"),
        "{}",
        child_caps.block_sources[1]
    );
    assert_eq!(std::any::type_name::<Cents>(), "i64");

    let snapshot = IssuanceSnapshot {
        kind: "cents_issuance_payload",
        vocabulary: Vocabulary {
            alias: "Cents",
            rust_type: std::any::type_name::<Cents>(),
            unit: "cents",
            tool: F_TOOL,
            namespace: F_NAMESPACE,
            budget_cents: F_BUDGET_CENTS,
            request_tool: F_REQUEST_TOOL,
            request_ns: F_REQUEST_NS,
            spend_cents: F_SPEND_CENTS,
        },
        root_tools: spec.tools.clone(),
        root_namespace: spec.namespace.clone(),
        root_budget_cents: spec.budget_cents,
        root_wire_len: root_bytes.len(),
        root_block_count: root_caps.block_count,
        root_tools_in_facts: sorted_tools(&root_caps),
        root_namespace_in_facts: root_caps.namespace.clone(),
        root_budget_cents_in_facts: root_caps.budget_cents,
        root_block_sources: root_caps.block_sources.clone(),
        child_tools: child.tools.clone(),
        child_spend_cap_cents: child.spend_cap_cents,
        child_wire_len: child_bytes.len(),
        child_block_count: child_caps.block_count,
        authority_block_unchanged: child_caps.block_sources[0] == root_caps.block_sources[0],
        child_budget_cents_in_facts: child_caps.budget_cents,
        child_block_sources: child_caps.block_sources,
    };
    insta::assert_yaml_snapshot!(snapshot);
}

#[derive(Serialize)]
struct Vocabulary {
    alias: &'static str,
    rust_type: &'static str,
    unit: &'static str,
    tool: &'static str,
    namespace: &'static str,
    budget_cents: &'static str,
    request_tool: &'static str,
    request_ns: &'static str,
    spend_cents: &'static str,
}

#[derive(Serialize)]
struct IssuanceSnapshot {
    kind: &'static str,
    vocabulary: Vocabulary,
    root_tools: Vec<String>,
    root_namespace: String,
    root_budget_cents: Cents,
    root_wire_len: usize,
    root_block_count: usize,
    root_tools_in_facts: Vec<String>,
    root_namespace_in_facts: Option<String>,
    root_budget_cents_in_facts: Option<Cents>,
    root_block_sources: Vec<String>,
    child_tools: Vec<String>,
    child_spend_cap_cents: Cents,
    child_wire_len: usize,
    child_block_count: usize,
    authority_block_unchanged: bool,
    child_budget_cents_in_facts: Option<Cents>,
    child_block_sources: Vec<String>,
}

#[derive(Serialize)]
struct RejectionCase {
    name: &'static str,
    tool: &'static str,
    namespace: &'static str,
    spend_cents: Cents,
    authorize_allows: bool,
    denial: Option<DenialView>,
}

#[derive(Serialize)]
struct DenialView {
    display: String,
    kind: &'static str,
    matched_policy: Option<PolicyView>,
    failed_checks: Vec<CheckView>,
    invalid_block_id: Option<u32>,
    invalid_block_rule: Option<String>,
}

#[derive(Serialize)]
struct PolicyView {
    effect: &'static str,
    index: usize,
}

#[derive(Serialize)]
struct CheckView {
    site: &'static str,
    block_id: Option<u32>,
    check_id: u32,
    rule: String,
}

impl From<Denial> for DenialView {
    fn from(denial: Denial) -> Self {
        match denial.body {
            DenialBody::Unauthorized {
                policy,
                failed_checks,
            } => Self {
                display: denial.display,
                kind: "unauthorized",
                matched_policy: Some(PolicyView::from(policy)),
                failed_checks: failed_checks.into_iter().map(CheckView::from).collect(),
                invalid_block_id: None,
                invalid_block_rule: None,
            },
            DenialBody::NoMatchingPolicy { failed_checks } => Self {
                display: denial.display,
                kind: "no_matching_policy",
                matched_policy: None,
                failed_checks: failed_checks.into_iter().map(CheckView::from).collect(),
                invalid_block_id: None,
                invalid_block_rule: None,
            },
            DenialBody::InvalidBlockRule { block_id, rule } => Self {
                display: denial.display,
                kind: "invalid_block_rule",
                matched_policy: None,
                failed_checks: Vec::new(),
                invalid_block_id: Some(block_id),
                invalid_block_rule: Some(rule),
            },
            DenialBody::AuthorizerNotEmpty => Self {
                display: denial.display,
                kind: "authorizer_not_empty",
                matched_policy: None,
                failed_checks: Vec::new(),
                invalid_block_id: None,
                invalid_block_rule: None,
            },
        }
    }
}

impl From<DenialPolicy> for PolicyView {
    fn from(policy: DenialPolicy) -> Self {
        Self {
            effect: match policy.effect {
                DenialEffect::Allow => "allow",
                DenialEffect::Deny => "deny",
            },
            index: policy.index,
        }
    }
}

impl From<DenialCheck> for CheckView {
    fn from(check: DenialCheck) -> Self {
        let site = if check.block_id.is_some() {
            "block"
        } else {
            "authorizer"
        };
        Self {
            site,
            block_id: check.block_id,
            check_id: check.check_id,
            rule: check.rule,
        }
    }
}

/// `authorize` 拒绝路径（`FailedLogic`）的错误结构。放行对照是 `denial: null`。
#[test]
fn rejection_error_structure() {
    let spec = root_spec();
    let child = child_spec();
    let root = issue_root(&spec).unwrap();
    let root_token = roundtrip!(&root.token, &root.keypair.public());
    let attenuated = attenuate(&root_token, &child).unwrap();
    let attenuated = roundtrip!(&attenuated, &root.keypair.public());

    let cases = [
        ("attenuated_git_revoked", false, "git", "line-a", 100),
        (
            "attenuated_spend_between_caps",
            false,
            "test",
            "line-a",
            800,
        ),
        (
            "attenuated_spend_over_both_caps",
            false,
            "test",
            "line-a",
            1200,
        ),
        ("attenuated_unknown_tool", false, "deploy", "line-a", 100),
        ("attenuated_wrong_namespace", false, "test", "line-b", 100),
        ("root_spend_over_budget", true, "test", "line-a", 1200),
        ("attenuated_test_within_cap", false, "test", "line-a", 100),
    ];

    let mut rows = Vec::with_capacity(cases.len());
    for (name, on_root, tool, namespace, spend_cents) in cases {
        let token = if on_root { &root_token } else { &attenuated };
        let authorize_allows = authorize(token, tool, namespace, spend_cents).unwrap();
        let found = denial(token, tool, namespace, spend_cents).unwrap();
        assert_eq!(authorize_allows, found.is_none(), "{name}");
        if name == "attenuated_git_revoked" {
            let text = &found.as_ref().unwrap().display;
            assert!(text.contains("allow policy matched"), "{text}");
            assert!(text.contains("request_tool"), "{text}");
            assert!(text.contains("test"), "{text}");
        }
        if name == "attenuated_test_within_cap" {
            assert!(found.is_none());
        } else {
            assert!(found.is_some(), "{name}");
        }
        rows.push(RejectionCase {
            name,
            tool,
            namespace,
            spend_cents,
            authorize_allows,
            denial: found.map(DenialView::from),
        });
    }

    let snapshot = RejectionSnapshot {
        kind: "rejection_error_structure",
        cases: rows,
    };
    insta::assert_yaml_snapshot!(snapshot);
}

#[derive(Serialize)]
struct RejectionSnapshot {
    kind: &'static str,
    cases: Vec<RejectionCase>,
}

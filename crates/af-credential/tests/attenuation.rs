//! 凭证签发、衰减、旧权限不可回收、预算衰减、防篡改。

use af_credential::{
    attenuate, authorize, deserialize_verify, inspect, issue_root, serialize, ChildSpec, RootSpec,
    F_TOOL,
};

fn root_spec() -> RootSpec {
    RootSpec {
        tools: vec!["git".into(), "test".into()],
        namespace: "line-a".into(),
        budget_cents: 1000, // usd 10.0，biscuit 6 无 Float，用美分整数
    }
}

fn child_spec() -> ChildSpec {
    ChildSpec {
        tools: vec!["test".into()],
        spend_cap_cents: 500,
    }
}

/// 1a. 签发：反序列化 + 验签，断言凭证事实（工具白名单 / 命名空间 / 预算）。
#[test]
fn issue_and_verify_root_token_facts() {
    let root = issue_root(&root_spec()).unwrap();
    let bytes = serialize(&root.token).unwrap();
    let token = deserialize_verify(&bytes, &root.keypair.public()).unwrap();

    let caps = inspect(&token).unwrap();
    assert_eq!(caps.block_count, 1);
    assert!(caps.tools.contains("git"), "tools={:?}", caps.tools);
    assert!(caps.tools.contains("test"), "tools={:?}", caps.tools);
    assert_eq!(caps.tools.len(), 2);
    assert_eq!(caps.namespace.as_deref(), Some("line-a"));
    assert_eq!(caps.budget_cents, Some(1000));

    // 反序列化后逐块断言 Datalog 源：authority 块确实带着 tool("git") 等事实
    let src0 = &caps.block_sources[0];
    assert!(
        src0.contains(&format!("{F_TOOL}(\"git\")")),
        "block0={src0}"
    );
    assert!(src0.contains("namespace(\"line-a\")"), "block0={src0}");
    assert!(src0.contains("budget_cents(1000)"), "block0={src0}");

    // 错误的根公钥验不过
    let other = af_credential::issue_root(&root_spec()).unwrap();
    assert!(
        deserialize_verify(&bytes, &other.keypair.public()).is_err(),
        "foreign root key must fail verification"
    );
}

/// 1b. 衰减：子 agent 块只能“加 check”，根块事实原封不动；但旧权限（git）实际拿不回来。
#[test]
fn attenuation_keeps_facts_but_revokes_git() {
    let root = issue_root(&root_spec()).unwrap();
    let child = attenuate(&root.token, &child_spec()).unwrap();

    // 序列化传输 → 反序列化验签（模拟子 agent 拿到的是字节串）
    let bytes = serialize(&child).unwrap();
    let token = deserialize_verify(&bytes, &root.keypair.public()).unwrap();

    assert_eq!(token.block_count(), 2, "衰减 = append 一个块");

    let caps = inspect(&token).unwrap();
    // 单调性：tool("git") 事实仍在 authority 块里，衰减删不掉
    assert!(
        caps.block_sources[0].contains(&format!("{F_TOOL}(\"git\")")),
        "authority block facts are immutable: {}",
        caps.block_sources[0]
    );
    // 衰减块里是那条白名单 check
    assert!(
        caps.block_sources[1].contains("contains"),
        "attenuation block holds the whitelist check: {}",
        caps.block_sources[1]
    );
    assert!(
        caps.tools.contains("git"),
        "facts unchanged (git still listed)"
    );

    // 但行为上：git 已不可用（check 封死），test 仍可用 → 旧权限拿不回来
    assert!(
        !authorize(&token, "git", "line-a", 100).unwrap(),
        "git must be revoked by attenuation"
    );
    assert!(
        authorize(&token, "test", "line-a", 100).unwrap(),
        "test must survive attenuation"
    );

    // 其他命名空间也拿不到：凭证绑定 line-a
    assert!(
        !authorize(&token, "test", "line-b", 100).unwrap(),
        "cross-namespace must fail"
    );
}

/// 1c. 预算衰减：根 1000 美分（usd 10.0）→ 子 500 美分（usd 5.0）。
#[test]
fn budget_attenuation_caps_spend() {
    let root = issue_root(&root_spec()).unwrap();
    let child = attenuate(&root.token, &child_spec()).unwrap();

    // 根凭证：受根块 check 约束（spend <= 1000）
    assert!(authorize(&root.token, "test", "line-a", 800).unwrap());
    assert!(
        !authorize(&root.token, "test", "line-a", 1200).unwrap(),
        "over root budget"
    );

    // 衰减后：两条 check 同时生效，取交集 → 上限 500
    assert!(authorize(&child, "test", "line-a", 300).unwrap());
    assert!(
        !authorize(&child, "test", "line-a", 800).unwrap(),
        "800 <= 1000 passes root check but breaks child cap 500 — must be denied"
    );
}

/// 1d. 篡改：改动任一字节都验不过签名链；删块同理（衰减块不可剥离）。
#[test]
fn tampered_token_fails_verification() {
    let root = issue_root(&root_spec()).unwrap();
    let child = attenuate(&root.token, &child_spec()).unwrap();
    let mut bytes = serialize(&child).unwrap();
    let n = bytes.len();
    bytes[n - 1] ^= 0x01;
    assert!(
        deserialize_verify(&bytes, &root.keypair.public()).is_err(),
        "byte-flip must break the signature chain"
    );
}

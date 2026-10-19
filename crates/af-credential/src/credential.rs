//! biscuit 凭证：签发、衰减、反序列化验签、事实提取、单凭证判定。

use std::collections::HashSet;
use std::time::Duration;

use anyhow::Result;
use biscuit_auth::builder as dlog;
use biscuit_auth::{Authorizer, AuthorizerLimits, Biscuit, BlockBuilder, KeyPair, PublicKey};

/// Datalog 执行限额：事实数/迭代数维持 biscuit 默认，墙钟上限从默认 1ms 放宽到 50ms。
/// 默认 1ms 在满载并行（cargo test -j 6）下会被 CPU 争用挤爆，把正常判定炸成
/// "Reached Datalog execution limits" 硬错误——授权判定不能押在库默认值上。
fn verify_limits() -> AuthorizerLimits {
    AuthorizerLimits {
        max_facts: 1000,
        max_iterations: 100,
        max_time: Duration::from_millis(50),
    }
}

/// 事实词表。biscuit 6 没有词表绑定机制（无 schema），谓词名是自由字符串，
/// 词表只能在这里用常量钉死，靠 code review 保证签发/验证两端一致。
pub const F_TOOL: &str = "tool";
pub const F_NAMESPACE: &str = "namespace";
/// biscuit 6.0 已删除 Float 类型（`builder::Term` 无 Float 变体），预算用“美分”整数编码。
pub const F_BUDGET_CENTS: &str = "budget_cents";
/// 以下三个是“验证方声明”的事实（由网关在 authorize 时注入，不是凭证携带的）。
pub const F_REQUEST_TOOL: &str = "request_tool";
pub const F_REQUEST_NS: &str = "request_ns";
pub const F_SPEND_CENTS: &str = "spend_cents";

/// 金额最小单位：美分，有符号。
///
/// `Cents` 是 `i64` 的别名。af-meter 的 `Cents` 同样是 `i64`：
/// `budget_cents`、`spend_cap_cents`、`spend_cents` 这三个词在两边都是这个类型，
/// 签发值可以原样交给账本。
pub type Cents = i64;

/// 根凭证签发参数。
pub struct RootSpec {
    pub tools: Vec<String>,
    pub namespace: String,
    /// 根块 per-call 上限（美分）。事实名 `budget_cents`，check 为 `spend_cents <= budget_cents`。
    pub budget_cents: Cents,
}

/// 子 agent 衰减参数。
pub struct ChildSpec {
    /// 衰减后允许的工具白名单（是根白名单的子集才有意义）。
    pub tools: Vec<String>,
    /// 衰减后单次调用开销上限（美分）。与 af-meter `try_spend` 的同名参数是同一个整数。
    pub spend_cap_cents: Cents,
}

/// 根凭证：根密钥 + 已签发的 token。
pub struct RootCredential {
    pub keypair: KeyPair,
    pub token: Biscuit,
}

/// 签发根凭证：工具白名单 + 命名空间 + 预算，外加一条预算护栏 check。
///
/// 根块 check：`spend_cents <= budget_cents`——把验证方声明的开销跟凭证预算挂钩。
/// （预算本身不能放在衰减块里“改小”，见 attenuate 的注释。）
pub fn issue_root(spec: &RootSpec) -> Result<RootCredential> {
    let keypair = KeyPair::new();
    let mut b = Biscuit::builder();
    for t in &spec.tools {
        b = b.fact(dlog::fact(F_TOOL, &[dlog::string(t)]))?;
    }
    b = b.fact(dlog::fact(F_NAMESPACE, &[dlog::string(&spec.namespace)]))?;
    b = b.fact(dlog::fact(F_BUDGET_CENTS, &[dlog::int(spec.budget_cents)]))?;
    // builder 只收 &str（TryFrom<&str>），不收 String
    let guard = format!("check if {F_SPEND_CENTS}($s), {F_BUDGET_CENTS}($v), $s <= $v");
    b = b.check(guard.as_str())?;
    let token = b.build(&keypair)?;
    Ok(RootCredential { keypair, token })
}

/// 序列化（线上传输格式）。
pub fn serialize(token: &Biscuit) -> Result<Vec<u8>> {
    Ok(token.to_vec()?)
}

/// 反序列化 + 根公钥验签（签名链校验不过会直接报错）。
pub fn deserialize_verify(bytes: &[u8], root_pk: &PublicKey) -> Result<Biscuit> {
    Ok(Biscuit::from(bytes, root_pk)?)
}

/// 衰减：holder 侧 append 一个新块（只能加 check，不能删事实）。
///
/// - 工具白名单收窄：`request_tool` 必须落在子白名单里。根块里的 `tool("git")`
///   事实**依然存在于 token**（无法删除），但任何 authorize 都要过这条 check，
///   旧权限拿不回来。
/// - 预算衰减：加一条更紧的 `spend_cents <= cap`。根块的 `$s <= budget_cents`
///   与子块的 `$s <= cap` 同时生效，取交集。
///   坑：不能写 `check if budget_cents($v), $v <= cap` 来“降预算”——根块事实
///   budget_cents(1000) 是不可变的，1000 <= 500 恒假，check 会拒绝一切请求。
pub fn attenuate(token: &Biscuit, child: &ChildSpec) -> Result<Biscuit> {
    let whitelist: Vec<String> = child.tools.iter().map(|t| format!("\"{t}\"")).collect();
    let wl_check = format!(
        "check if {F_REQUEST_TOOL}($t), [{}].contains($t)",
        whitelist.join(", ")
    );
    let cap_check = format!(
        "check if {F_SPEND_CENTS}($s), $s <= {}",
        child.spend_cap_cents
    );
    let b = BlockBuilder::new()
        .check(wl_check.as_str())?
        .check(cap_check.as_str())?;
    Ok(token.append(b)?)
}

/// 从（可能衰减过的）token 提取事实视图。
#[derive(Debug, Clone)]
pub struct TokenCaps {
    pub tools: HashSet<String>,
    pub namespace: Option<String>,
    pub budget_cents: Option<Cents>,
    pub block_count: usize,
    /// 每个 block 的 Datalog 源码（供测试断言与展示）。
    pub block_sources: Vec<String>,
}

/// 反序列化后的 token 上跑 authorizer query，结构化提取凭证事实。
pub fn inspect(token: &Biscuit) -> Result<TokenCaps> {
    let mut auth: Authorizer = token.authorizer()?;
    let pairs: Vec<(String, String)> = auth.query_with_limits(
        format!("cap($ns, $t) <- {F_NAMESPACE}($ns), {F_TOOL}($t)").as_str(),
        verify_limits(),
    )?;
    let mut tools = HashSet::new();
    for (_, t) in pairs {
        tools.insert(t);
    }
    let nb: Vec<(String, Cents)> = auth.query_with_limits(
        format!("nb($ns, $v) <- {F_NAMESPACE}($ns), {F_BUDGET_CENTS}($v)").as_str(),
        verify_limits(),
    )?;
    let (namespace, budget_cents) = nb
        .into_iter()
        .next()
        .map(|(a, v)| (Some(a), Some(v)))
        .unwrap_or((None, None));

    let block_count = token.block_count();
    let mut block_sources = Vec::with_capacity(block_count);
    for i in 0..block_count {
        block_sources.push(token.print_block_source(i)?);
    }
    Ok(TokenCaps {
        tools,
        namespace,
        budget_cents,
        block_count,
        block_sources,
    })
}

/// 单凭证判定：验证方声明 request_tool / request_ns / spend_cents，
/// 凭证侧 allow 当且仅当（a）策略匹配到凭证携带的能力且命名空间一致，（b）所有
/// check（含衰减块里的）都通过。
///
/// 语义坑：biscuit 的 check 是**存在性**查询——若验证方注入多个 request_tool 事实，
/// 只要有一个落在白名单 check 就过。因此网关必须保证“一次请求只声明一个
/// request_tool”（此处按约定实现，未在 biscuit 内强制）。
///
/// `FailedLogic` 折成 `Ok(false)` 并写 stderr。同一条路径的结构见 [`denial`]。
pub fn authorize(token: &Biscuit, tool: &str, namespace: &str, spend_cents: Cents) -> Result<bool> {
    match run_authorize(token, tool, namespace, spend_cents) {
        Ok(_) => Ok(true),
        // 「拒绝」是正常判定结果：无 allow 命中，或某条 check 失败。
        Err(e @ biscuit_auth::error::Token::FailedLogic(_)) => {
            log_denial(tool, namespace, &e);
            Ok(false)
        }
        // 其余（超限、执行错误等）按真错误上抛。
        Err(e) => Err(e.into()),
    }
}

/// 一次拒绝的结构化记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    /// `Token` 的 Display，与 `authorize` 打到 stderr 的正文一致（不含日志前缀）。
    pub display: String,
    pub body: DenialBody,
}

/// `FailedLogic` 的四种形态。前两种是正常拒绝；后两种 biscuit 同样归进 `FailedLogic`，
/// `authorize` 也把它们当成拒绝而不是真错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenialBody {
    /// 有策略命中，但至少一条 check 失败。
    Unauthorized {
        policy: DenialPolicy,
        failed_checks: Vec<DenialCheck>,
    },
    /// 没有命中任何策略。checks 仍可能失败。
    NoMatchingPolicy { failed_checks: Vec<DenialCheck> },
    /// 块内规则产生了未绑定变量。
    InvalidBlockRule { block_id: u32, rule: String },
    /// authorizer 里已经绑过 token。
    AuthorizerNotEmpty,
}

/// 命中的那条 authorizer 策略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenialPolicy {
    pub effect: DenialEffect,
    pub index: usize,
}

/// 命中策略的效果。`Allow` 表示 allow 策略命中（拒绝来自 check）；`Deny` 表示 deny 策略命中。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialEffect {
    Allow,
    Deny,
}

/// 一条失败的 check。`block_id == None` 表示 check 写在 authorizer 上，不在 token 块里。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenialCheck {
    pub block_id: Option<u32>,
    pub check_id: u32,
    pub rule: String,
}

/// 与 [`authorize`] 走同一条判定。
///
/// 放行是 `Ok(None)`。`FailedLogic` 是 `Ok(Some(_))`，不写 stderr。
/// 执行限额、格式错误等仍是 `Err`（真错误，不是拒绝）。限额与 [`authorize`] 相同。
pub fn denial(
    token: &Biscuit,
    tool: &str,
    namespace: &str,
    spend_cents: Cents,
) -> Result<Option<Denial>> {
    match run_authorize(token, tool, namespace, spend_cents) {
        Ok(_) => Ok(None),
        Err(biscuit_auth::error::Token::FailedLogic(logic)) => {
            let display = biscuit_auth::error::Token::FailedLogic(logic.clone()).to_string();
            Ok(Some(denial_from_logic(display, logic)))
        }
        Err(e) => Err(e.into()),
    }
}

fn run_authorize(
    token: &Biscuit,
    tool: &str,
    namespace: &str,
    spend_cents: Cents,
) -> std::result::Result<usize, biscuit_auth::error::Token> {
    let allow_policy = format!(
        "allow if {F_REQUEST_TOOL}($t), {F_TOOL}($t), {F_REQUEST_NS}($n), {F_NAMESPACE}($n)"
    );
    let authorizer = biscuit_auth::AuthorizerBuilder::new()
        .fact(dlog::fact(F_REQUEST_TOOL, &[dlog::string(tool)]))?
        .fact(dlog::fact(F_REQUEST_NS, &[dlog::string(namespace)]))?
        .fact(dlog::fact(F_SPEND_CENTS, &[dlog::int(spend_cents)]))?
        .policy(allow_policy.as_str())?
        .set_limits(verify_limits());
    authorizer.build(token)?.authorize()
}

fn denial_from_logic(display: String, logic: biscuit_auth::error::Logic) -> Denial {
    use biscuit_auth::error::{Logic, MatchedPolicy};

    let body = match logic {
        Logic::Unauthorized { policy, checks } => DenialBody::Unauthorized {
            policy: match policy {
                MatchedPolicy::Allow(index) => DenialPolicy {
                    effect: DenialEffect::Allow,
                    index,
                },
                MatchedPolicy::Deny(index) => DenialPolicy {
                    effect: DenialEffect::Deny,
                    index,
                },
            },
            failed_checks: checks.into_iter().map(denial_check).collect(),
        },
        Logic::NoMatchingPolicy { checks } => DenialBody::NoMatchingPolicy {
            failed_checks: checks.into_iter().map(denial_check).collect(),
        },
        Logic::InvalidBlockRule(block_id, rule) => DenialBody::InvalidBlockRule { block_id, rule },
        Logic::AuthorizerNotEmpty => DenialBody::AuthorizerNotEmpty,
    };
    Denial { display, body }
}

fn denial_check(check: biscuit_auth::error::FailedCheck) -> DenialCheck {
    use biscuit_auth::error::FailedCheck;
    match check {
        FailedCheck::Block(block) => DenialCheck {
            block_id: Some(block.block_id),
            check_id: block.check_id,
            rule: block.rule,
        },
        FailedCheck::Authorizer(authorizer) => DenialCheck {
            block_id: None,
            check_id: authorizer.check_id,
            rule: authorizer.rule,
        },
    }
}

/// 拒绝原因打进 stderr，便于 PoC 观察（正式实现应走 tracing）。
fn log_denial(tool: &str, namespace: &str, e: &biscuit_auth::error::Token) {
    eprintln!("[af-credential] denied {tool}@{namespace}: {e}");
}

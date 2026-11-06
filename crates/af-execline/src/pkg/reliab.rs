//! 判分可信度协议（strategies17 · st-b-C022，债 D-57，设计稿 §七.4）：
//! **三值判定（pass/fail/undecidable）+ 多评委校准 + 判分污染审计**。
//!
//! # 为什么需要这个模块（要拆的混淆点）
//!
//! 现 verdict 契约是二值 + null：`resolved: Option<bool>`——
//! - `Some(true)` = 有结论·通过；`Some(false)` = 有结论·不通过；
//! - `None` = **供给面失败**（无判定：判分链没跑出结论），语义锚
//!   `pkg/sweng.rs:705-706`（`resolved ∈ {true,false}=有结论；null=供给面
//!   失败（不通过）`），构造点 `:732`（`verdict_from_verify` 在无 judge JSON
//!   时落 `{resolved: null, error: note}`）；执行面同款
//!   `exec.rs:119-120`（`None = 无判定（供给面失败/未跑完）`）。
//!
//! 于是「无标准答案评审门禁」缺一刀：判分器**跑了、也给了结论**，但结论
//! **不可信**（多评委分歧 / 判分被污染）——这种「判分了却不可采信」既不是
//! pass/fail，也**不该**塞进 `None`（那会把「不可信」和「供给面失败」混成
//! 一坨）。既有 `af_eval::GateVerdict`（`af-eval/src/gate.rs:112-118`，
//! `{Pass, Fail, NeedsHuman}`）已示范三值枚举，但它在消费端被压回
//! `GateVerdict::NeedsHuman → None`（`gate_unit.rs:471-474`、
//! `graph_unit.rs:2832-2834`）——即把「需人工」折成与「供给面失败」同形的
//! null。本卡正要不重蹈此覆辙。
//!
//! # 本模块的契约立场（断言级）
//!
//! - [`Decision`] = 三值判定，**只在「判分器给出了结论」的域内**取值；
//! - 供给面失败仍由外层 `Option` 的 `None` 承载——[`Decision::from_resolved`]
//!   把旧 `Option<bool>` **无损**搬过来（`None` 原样仍是 `None`，**绝不**变
//!   `Undecidable`）；
//! - 本模块**故意不提供** `Decision → Option<bool>` 的回转：`Undecidable`
//!   在 `Option<bool>` 里没有忠实落点（只能撞 `None` = 供给面失败），这个
//!   「缺失」就是本卡要守住的边界——新消费面直接匹配 [`Decision`]，不经会丢
//!   信息的回转。
//!
//! # 落位纪律
//!
//! 纯函数 + 纯数据结构，零 I/O、零子进程、零 LLM——判分器本体仍走
//! `judge.rs` 子进程边界（独立性铁律 §3.2-1），本模块只做判分**产出**的
//! 校准与污染审计，不碰判分执行。模块结构承本目录 `pkg/sweng.rs`（在册）与
//! 同族 S17 `pkg/iface.rs`（注册批 A）先例：详注头 + 语义内联 + 纯函数；
//! `af-bench` 的 check/gate 只消费
//! 判定结果、不表达校准与污染协议（`strategies/st-b-C022.yaml:13` na_reason
//! 原文），故本能力落在执行面库模块而非新注册组件（不增 components.json 计数）。

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// 三值判定（st-b-C022 的 pass/fail/undecidable）。
///
/// 与 `resolved: Option<bool>` 的 `null=供给面失败` 语义**正交**：本枚举
/// 表达「判分器给出了结论」域内的三态；供给面失败由外层 `Option::None`
/// 承载，二者绝不互换（模块头 §契约立场）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// 通过（校准后的可信通过结论）。
    Pass,
    /// 不通过（校准后的可信不通过结论）。
    Fail,
    /// 不可判定：判分器跑了、但结论不可信（多评委分歧 / 结论被污染）。
    /// **不是**供给面失败——供给面失败是 `Option::None`。
    Undecidable,
}

impl Decision {
    /// 稳定字符串形态（snake_case，与 serde `rename_all` 一致；落 verdict
    /// JSON / 日志用）。
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Pass => "pass",
            Decision::Fail => "fail",
            Decision::Undecidable => "undecidable",
        }
    }

    /// 从既有 `resolved: Option<bool>` 读判——**保语义、不混淆 null**：
    /// `Some(true)`→`Some(Pass)`；`Some(false)`→`Some(Fail)`；
    /// `None`→`None`（仍是供给面失败，**绝不变**`Undecidable`）。
    ///
    /// 这是旧契约到三值契约唯一安全的搬运方向（无损）。反向
    /// （`Decision → Option<bool>`）会丢 `Undecidable`，故本模块不提供。
    pub fn from_resolved(resolved: Option<bool>) -> Option<Decision> {
        resolved.map(|r| if r { Decision::Pass } else { Decision::Fail })
    }
}

/// 单评委原始票。
#[derive(Debug, Clone)]
pub struct JudgeVote {
    /// 评委标识（审计/读数面溯源用）。
    pub judge_id: String,
    /// 该评委的判定；`None` = 该评委**供给面失败**（无结论，语义承
    /// `sweng.rs` `SwebenchVerdict.resolved`）。
    pub resolved: Option<bool>,
    /// 污染审计命中（该票不可采信，见 [`audit_contamination`]）。
    pub contaminated: bool,
}

/// 判分污染审计报告（v1 工作定义：禁泄漏标样命中清单）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContaminationReport {
    /// 命中的禁泄漏标样（去重、按标样清单序）。
    pub markers: Vec<String>,
}

impl ContaminationReport {
    /// 是否判污染（命中非空）。
    pub fn contaminated(&self) -> bool {
        !self.markers.is_empty()
    }
}

/// 判分污染检测（st-b-C022「判分污染检测」）：判定产物文本里出现**禁泄漏
/// 标样** ⇒ 判污染。
///
/// v1 工作定义 = 子串包含匹配（大小写敏感）；`forbidden_markers` = 本不应
/// 出现在干净判分里的泄漏指纹（参考答案片段 / 金标 patch id / 题面隐藏
/// 信息——由调用方按任务装配，本函数不预设标样集）。返回命中清单（空 = 干净）；
/// 空串标样跳过（防空标样误命中全文本）。调用方把结果落到
/// [`JudgeVote::contaminated`]。
pub fn audit_contamination(verdict_text: &str, forbidden_markers: &[&str]) -> ContaminationReport {
    let mut markers: Vec<String> = Vec::new();
    for &marker in forbidden_markers {
        if marker.is_empty() {
            continue;
        }
        if verdict_text.contains(marker) && !markers.iter().any(|m| m == marker) {
            markers.push(marker.to_string());
        }
    }
    ContaminationReport { markers }
}

/// 便利建票：从原始判定（`resolved` + 判分文本 + 禁泄漏标样）造一条评委票，
/// 污染位由 [`audit_contamination`] 就地判定——调用面不必手写 `contaminated`。
pub fn judge_vote(
    judge_id: &str,
    resolved: Option<bool>,
    verdict_text: &str,
    forbidden_markers: &[&str],
) -> JudgeVote {
    JudgeVote {
        judge_id: judge_id.to_string(),
        resolved,
        contaminated: audit_contamination(verdict_text, forbidden_markers).contaminated(),
    }
}

/// 多评委校准 → 三值判定（`Option<Decision>`；污染感知，保 null 语义）。
///
/// 规则（按序）：
/// 1. 只采信**未污染且 `resolved` 有值**的票（`clean_pass`/`clean_fail`）；
///    被污染的票与供给面失败的票都不计票（fail-closed：被污染的结论不得算数，
///    没跑出结论的票不是结论）。
/// 2. `(0, 0)`（无干净票）：若还存在**任何有结论的票**（即全被污染）⇒
///    `Some(Undecidable)`（判分了但不可采信）；若一票结论都没有（全供给面
///    失败）⇒ `None`（**透传 null**，不伪造 undecidable）。
/// 3. `(_, 0)`（干净票全 pass）⇒ `Some(Pass)`；`(0, _)`（干净票全 fail）⇒
///    `Some(Fail)`——即干净票须**一致**（unanimity），任一干净评委唱反调即
///    不可信。
/// 4. 干净票既有 pass 又有 fail（分歧）⇒ `Some(Undecidable)`。
///
/// 选 unanimity 而非 majority：本协议的可信度优先于吞吐——分歧即不可信。
pub fn calibrate(votes: &[JudgeVote]) -> Option<Decision> {
    let clean_pass = votes
        .iter()
        .filter(|v| !v.contaminated && v.resolved == Some(true))
        .count();
    let clean_fail = votes
        .iter()
        .filter(|v| !v.contaminated && v.resolved == Some(false))
        .count();
    match (clean_pass, clean_fail) {
        (0, 0) => {
            if votes.iter().any(|v| v.resolved.is_some()) {
                Some(Decision::Undecidable)
            } else {
                None
            }
        }
        (_, 0) => Some(Decision::Pass),
        (0, _) => Some(Decision::Fail),
        _ => Some(Decision::Undecidable),
    }
}

/// 污染**无感**的朴素**多数决**（负例夹具的对照臂）：把**所有**有结论的票
/// （含被污染）一并计入，票多者胜；平局（双方均 >0）⇒ `Undecidable`；无一票
/// 有结论 ⇒ `None`（透传 null，不伪造结论）。用于证明「污染会制造方向性假
/// 结论」——与 [`calibrate`]（干净票 unanimity）的差即污染审计的价值：多数决
/// 会被少数泄漏票 tipping / 淹没，unanimity 校准则拒判或翻转。
pub fn naive_majority(votes: &[JudgeVote]) -> Option<Decision> {
    let pass = votes.iter().filter(|v| v.resolved == Some(true)).count();
    let fail = votes.iter().filter(|v| v.resolved == Some(false)).count();
    match pass.cmp(&fail) {
        Ordering::Greater => Some(Decision::Pass),
        Ordering::Less => Some(Decision::Fail),
        Ordering::Equal => {
            if pass == 0 {
                None
            } else {
                Some(Decision::Undecidable)
            }
        }
    }
}

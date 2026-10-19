//! 类型化结构 —— 与 af/poc/poc-cue/line/line.cue (#Line) 和 rules/rules.cue (#Rule) 对应。
//! closed 纪律：所有 struct 加 deny_unknown_fields（等价 CUE definition 的 closed）。
//! CUE 里的 `*默认 | 类型` 映射为 serde default（enabled/escalateAfterMs/escalateTo）。

use serde::Deserialize;
use std::collections::BTreeMap;

fn default_true() -> bool {
    true
}
fn default_zero() -> u64 {
    0
}

// ============ 产线定义（#Line） ============

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Line {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub graph_templates: BTreeMap<String, GraphTemplateRef>,
    #[serde(default)]
    pub eval_functions: BTreeMap<String, EvalFunction>,
    #[serde(default)]
    pub tools: BTreeMap<String, Tool>,
    #[serde(default)]
    pub andon_rules: Vec<AndonRule>,
    pub artifact_schema: ArtifactSchema,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GraphTemplateRef {
    pub template_id: String,
    pub version: String,
    pub entry: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EvalFunction {
    pub name: String,
    pub kind: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub driver: ToolDriver,
    pub endpoint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ToolDriver {
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "mqtt")]
    Mqtt,
    #[serde(rename = "opcua")]
    Opcua,
    #[serde(rename = "modbus")]
    Modbus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AndonRule {
    pub id: String,
    pub severity: Severity,
    pub notify: Vec<String>,
    #[serde(default = "default_zero")]
    pub escalate_after_ms: u64,
    #[serde(default)]
    pub escalate_to: Vec<String>,
    pub trigger: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Severity {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "critical")]
    Critical,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ArtifactSchema {
    pub kind: String,
    pub fields: BTreeMap<String, ArtifactField>,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ArtifactField {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub enum_values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum FieldType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "timestamp")]
    Timestamp,
    #[serde(rename = "enum")]
    Enum,
}

// ============ 配方规则（#Rule，kind 判别 + per-kind params） ============

/// 结构层原始规则：kind 先按字符串收，params 留 Value，分派在 loader.rs 做
/// （flatten + deny_unknown_fields 在 serde 里不兼容，故不用 internally-tagged enum）
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RawRule {
    pub id: String,
    pub kind: String,
    pub desc: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub id: String,
    pub kind: RuleKind,
    pub desc: String,
    pub enabled: bool,
    pub params: RuleParams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    Threshold,
    Ratio,
    TimeWindow,
    Dwell,
    Count,
    Fingerprint,
    Escalation,
    Combo,
}

impl RuleKind {
    pub fn from_str(s: &str) -> Option<RuleKind> {
        Some(match s {
            "threshold" => RuleKind::Threshold,
            "ratio" => RuleKind::Ratio,
            "timeWindow" => RuleKind::TimeWindow,
            "dwell" => RuleKind::Dwell,
            "count" => RuleKind::Count,
            "fingerprint" => RuleKind::Fingerprint,
            "escalation" => RuleKind::Escalation,
            "combo" => RuleKind::Combo,
            _ => return None,
        })
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleKind::Threshold => "threshold",
            RuleKind::Ratio => "ratio",
            RuleKind::TimeWindow => "timeWindow",
            RuleKind::Dwell => "dwell",
            RuleKind::Count => "count",
            RuleKind::Fingerprint => "fingerprint",
            RuleKind::Escalation => "escalation",
            RuleKind::Combo => "combo",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuleParams {
    Threshold(ThresholdParams),
    Ratio(RatioParams),
    TimeWindow(TimeWindowParams),
    Dwell(DwellParams),
    Count(CountParams),
    Fingerprint(FingerprintParams),
    Escalation(EscalationParams),
    Combo(ComboParams),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThresholdParams {
    pub metric: String,
    pub op: CmpOp,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatioParams {
    pub numerator: String,
    pub denominator: String,
    pub warn: f64,
    pub critical: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeWindowParams {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DwellParams {
    pub metric: String,
    pub op: CmpOp,
    pub value: f64,
    pub samples: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountParams {
    pub metric: String,
    pub op: CmpOp,
    #[serde(rename = "atLeast")]
    pub at_least: u64,
    pub within: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FingerprintParams {
    pub fields: Vec<String>,
    pub ttl: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EscalationParams {
    pub chain: Vec<EscalateStep>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EscalateStep {
    pub after_ms: u64,
    pub notify: Vec<String>,
    pub action: EscalateAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EscalateAction {
    #[serde(rename = "notify")]
    Notify,
    #[serde(rename = "page")]
    Page,
    #[serde(rename = "stop-line")]
    StopLine,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComboParams {
    #[serde(rename = "allOf")]
    pub all_of: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum CmpOp {
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
}

impl CmpOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
        }
    }
}

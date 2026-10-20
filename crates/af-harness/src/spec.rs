//! 四元组规格。对外是拒绝式信封：`{"v":1,"body":...}`。
//!
//! 缺 `v`、`v` 不是整数、`v` 不是 1、信封或 body 里有未声明字段，都直接拒绝。
//! 版本不对时不解析 body。

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// 当前规格版本。
pub const SPEC_VERSION: u32 = 1;

const NAME_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_.:-";

/// 协议。`openai-chat-completions` 覆盖 bigmodel / kimi / step 这类供应商形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[serde(rename = "openai-chat-completions")]
    OpenaiChatCompletions,
    #[serde(rename = "anthropic-messages")]
    AnthropicMessages,
}

/// 推理档位。适配器把它写成各协议的字段，规格里只存这一档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningTier {
    Off,
    Low,
    Medium,
    High,
}

/// 副作用分类。纯函数本地执行；另外两类一律走 effect gateway。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectClass {
    /// 纯函数。
    Pure,
    /// 需落盘。
    Persist,
    /// 一次性外部操作。
    ExternalOnce,
}

impl EffectClass {
    pub const fn is_side_effect(self) -> bool {
        !matches!(self, EffectClass::Pure)
    }
}

/// 预算单位。`cents` 走 [`af_meter::BudgetLedger`]，`count` 走 [`af_meter::Meter::try_consume`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetUnit {
    Cents,
    Count,
}

/// 认证形态。缺省按协议推导：openai → bearer，anthropic → x_api_key。
/// mavis 网关与 bigmodel coding plan 的 anthropic 端点都要求 bearer，需显式覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStyle {
    Bearer,
    XApiKey,
}

/// 端点。`key_ref` 只存引用名，不存明文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointSpec {
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    pub key_ref: String,
    pub reasoning_tier: ReasoningTier,
    /// 为真时走 SSE 组装。默认非流式。
    #[serde(default)]
    pub stream: bool,
    /// 认证形态覆盖。缺省按协议推导。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthStyle>,
    /// 单次响应 max_tokens 上限。None=协议默认（Anthropic 8192； thinking 预算另叠加）。
    /// 2026-10-20 事故修复前硬编码 1024，导致 52.7% 运行被截断杀死（详见变更日志）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 采样温度（毫度，700=0.7）。None=厂商默认。测评可复现性地基：显式写入请求体。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_milli: Option<u32>,
    /// top_p（毫度，950=0.95）。None=厂商默认。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p_milli: Option<u32>,
}

/// 系统提示模板。`slots` 是允许出现的变量名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextTemplate {
    pub system: String,
    pub slots: Vec<String>,
}

/// 一条工具声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDecl {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// JSON Schema。必须是对象。运行时只核对 `required` 数组里的键是否存在。
    pub parameters: Value,
    pub effect: EffectClass,
}

/// 停止条件里由规格自己表达的部分。指纹连击交给 [`af_stop::StopEngine`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopCondition {
    /// 完成信号词。fix2 语义：本轮任一**工具输出文本的首行**（trim 后）完整等于
    /// 其中任一词即完成——完成必须由真实执行的工具调用产出，助手叙述文本不触发。
    /// 空串拒绝。
    pub completion_signals: Vec<String>,
    /// 连续无工具调用的轮数上限。达到即停。0 表示出现一轮无工具就停。
    /// fix2：未达上限的无工具轮会立刻收到一条教学反馈（user 消息注入），不再静默倒计时。
    pub tool_free_turn_limit: u32,
}

/// 预算维度引用。账本本身由调用方注入，规格只点名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetRef {
    pub dimension: String,
    /// 每一模型轮扣多少。单位由 [`BudgetRef::unit`] 决定。
    pub per_turn: u64,
    pub unit: BudgetUnit,
}

/// 轮次、停止、预算。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub max_turns: u32,
    pub stop: StopCondition,
    pub budget_ref: BudgetRef,
}

/// 四元组。版本在信封上，不在 body 里。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSpec {
    pub endpoint: EndpointSpec,
    pub context_template: ContextTemplate,
    pub toolset: Vec<ToolDecl>,
    pub policy: Policy,
}

impl AgentSpec {
    pub fn tool(&self, name: &str) -> Option<&ToolDecl> {
        self.toolset.iter().find(|tool| tool.name == name)
    }

    pub fn validate(&self) -> Result<()> {
        validate_endpoint(&self.endpoint)?;
        validate_template(&self.context_template)?;
        validate_tools(&self.toolset)?;
        validate_policy(&self.policy)?;
        Ok(())
    }
}

/// 写成 `v = 1` 的信封。规格不合法时不写出。
pub fn encode_spec(spec: &AgentSpec) -> Result<String> {
    spec.validate()?;
    let envelope = serde_json::json!({ "v": SPEC_VERSION, "body": spec });
    Ok(serde_json::to_string_pretty(&envelope)?)
}

/// 读信封。版本不对、字段多余、规格不合法，都拒绝。
pub fn decode_spec(text: &str) -> Result<AgentSpec> {
    let value: Value = serde_json::from_str(text)?;
    let obj = value
        .as_object()
        .ok_or_else(|| Error::InvalidSpec("信封必须是对象".into()))?;
    for key in obj.keys() {
        if key != "v" && key != "body" {
            return Err(Error::UnknownField(key.clone()));
        }
    }
    match obj.get("v") {
        None => return Err(Error::MissingVersion),
        Some(Value::Number(number)) => {
            let found = number.as_u64().ok_or_else(|| Error::UnreadableVersion {
                raw: number.to_string(),
            })?;
            if found != u64::from(SPEC_VERSION) {
                return Err(Error::UnknownVersion {
                    found,
                    expected: SPEC_VERSION,
                });
            }
        }
        Some(other) => {
            return Err(Error::UnreadableVersion {
                raw: other.to_string(),
            });
        }
    }
    let body = obj.get("body").ok_or(Error::MissingBody)?;
    let spec: AgentSpec = serde_json::from_value(body.clone())?;
    spec.validate()?;
    Ok(spec)
}

fn validate_endpoint(endpoint: &EndpointSpec) -> Result<()> {
    let base = endpoint.base_url.as_str();
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return Err(Error::InvalidSpec(
            "base_url 必须以 http:// 或 https:// 开头".into(),
        ));
    }
    if base.contains('@') || base.chars().any(char::is_whitespace) {
        return Err(Error::InvalidSpec(
            "base_url 不允许用户信息或空白，密钥不能写进 URL".into(),
        ));
    }
    if endpoint.model.is_empty() || endpoint.model.chars().any(char::is_whitespace) {
        return Err(Error::InvalidSpec("model 不能为空或含空白".into()));
    }
    if !is_ref_name(&endpoint.key_ref) || endpoint.key_ref.starts_with("sk-") {
        return Err(Error::InvalidSpec(
            "key_ref 只存引用名（字母数字和 _.:-），不存明文密钥，也不以 sk- 开头".into(),
        ));
    }
    Ok(())
}

fn validate_template(template: &ContextTemplate) -> Result<()> {
    let mut seen = BTreeSet::new();
    for slot in &template.slots {
        if !is_slot(slot) {
            return Err(Error::InvalidSpec(format!("非法槽名 {slot}")));
        }
        if !seen.insert(slot.as_str()) {
            return Err(Error::InvalidSpec(format!("重复槽 {slot}")));
        }
    }
    for slot in placeholders(&template.system)? {
        if !seen.contains(slot.as_str()) {
            return Err(Error::UnknownSlot(slot));
        }
    }
    Ok(())
}

fn validate_tools(tools: &[ToolDecl]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for tool in tools {
        if !is_ref_name(&tool.name) {
            return Err(Error::InvalidSpec(format!("非法工具名 {}", tool.name)));
        }
        if !seen.insert(tool.name.as_str()) {
            return Err(Error::InvalidSpec(format!("重复工具 {}", tool.name)));
        }
        if !tool.parameters.is_object() {
            return Err(Error::InvalidSpec(format!(
                "工具 {} 的 parameters 必须是 JSON 对象",
                tool.name
            )));
        }
    }
    Ok(())
}

fn validate_policy(policy: &Policy) -> Result<()> {
    if !is_ref_name(&policy.budget_ref.dimension) {
        return Err(Error::InvalidSpec(
            "budget_ref.dimension 不能为空，且只含字母数字和 _.:-".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    for signal in &policy.stop.completion_signals {
        if signal.is_empty() {
            return Err(Error::InvalidSpec("完成信号词不能是空串".into()));
        }
        if !seen.insert(signal.as_str()) {
            return Err(Error::InvalidSpec(format!("重复完成信号词 {signal}")));
        }
    }
    Ok(())
}

fn is_ref_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.chars().all(|ch| NAME_CHARS.contains(ch))
}

fn is_slot(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// 抽出 `{{ name }}` 的槽名。不成对的 `{{` 拒绝。值不在这里替换。
pub(crate) fn placeholders(system: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut rest = system;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(Error::InvalidSpec("模板有未闭合的 {{".into()));
        };
        out.push(after[..end].trim().to_string());
        rest = &after[end + 2..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_envelope_roundtrip() {
        let spec = decode_spec(crate::EXAMPLE_SPEC_JSON).unwrap();
        assert_eq!(spec.endpoint.protocol, Protocol::OpenaiChatCompletions);
        assert_eq!(spec.endpoint.key_ref, "kimi.prod");
        assert_eq!(spec.toolset.len(), 2);
        assert_eq!(spec.toolset[0].effect, EffectClass::Pure);
        assert_eq!(spec.toolset[1].effect, EffectClass::ExternalOnce);
        assert_eq!(spec.policy.budget_ref.unit, BudgetUnit::Cents);
        let encoded = encode_spec(&spec).unwrap();
        let again = decode_spec(&encoded).unwrap();
        assert_eq!(again, spec);
    }

    #[test]
    fn rejects_version_field_and_unknown_keys_before_body() {
        let missing = decode_spec(r#"{"body":{"endpoint":{}}}"#).unwrap_err();
        assert!(matches!(missing, Error::MissingVersion));

        let text = decode_spec(r#"{"v":"1","body":{"nope":true}}"#).unwrap_err();
        assert!(matches!(text, Error::UnreadableVersion { .. }));

        let future = decode_spec(r#"{"v":2,"body":{"not":"a spec"}}"#).unwrap_err();
        assert!(matches!(
            future,
            Error::UnknownVersion {
                found: 2,
                expected: 1
            }
        ));

        let extra = decode_spec(r#"{"v":1,"body":{},"extra":1}"#).unwrap_err();
        assert!(matches!(extra, Error::UnknownField(ref field) if field == "extra"));

        let bad_body =
            decode_spec(r#"{"v":1,"body":{"endpoint":{"protocol":"openai-chat-completions"}}}"#)
                .unwrap_err();
        assert!(matches!(bad_body, Error::Json(_)));
    }

    #[test]
    fn rejects_secret_shaped_key_ref_and_url_userinfo() {
        let mut spec = decode_spec(crate::EXAMPLE_SPEC_JSON).unwrap();
        spec.endpoint.key_ref = "sk-live-secret".into();
        assert!(spec.validate().is_err());
        spec = decode_spec(crate::EXAMPLE_SPEC_JSON).unwrap();
        spec.endpoint.base_url = "https://user:pass@api.example".into();
        assert!(spec.validate().is_err());
    }
}

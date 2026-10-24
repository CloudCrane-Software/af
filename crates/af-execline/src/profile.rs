//! 端点档案与密钥解析（af-bench 吸收 P1，原样搬入）。
//!
//! - `Profile`/`profile()` ← af-bench main.rs:156-327：key_cmd 字符串逐字保留
//!   （密钥纪律：明文只从解析器命令出来进请求头，不打印、不落文档——main.rs:10；
//!   零明文落码纪律随迁不变）；
//! - `resolve_key()` ← af-bench main.rs:329-350。
//!
//! af-harness 依赖说明：`profile()` 返回 af-harness 公开类型 `EndpointSpec`
//! （"原样搬"要求签名不动），故该依赖自 P4 提前至 P1——偏差登记于 RESULT-P1.md；
//! 依赖方向 af-execline ← af-harness，无环（方案 §1.2-3 断言仍成立）。
//! key_cmd/base_url/model/key_ref 字面量与 af-bench main.rs 的字节级一致由
//! tests/p1_comparison.rs 源文本比对钉死；resolve_key 行为由 tests/p1_suite.rs
//! 黄金值断言（printf 假命令，零真实凭证消耗）。

use af_harness::{AuthStyle, EndpointSpec, Protocol, ReasoningTier};
use std::process::Command;

// ---------- 端点档案（优惠通道，见 状态/优惠通道.md） ----------
// ← af-bench main.rs:156-161 原样。

pub struct Profile {
    pub endpoint: EndpointSpec,
    pub key_cmd: &'static str,
}

/// ← af-bench main.rs:163-327 原样（含各档案注释与未知档案报错文案）。
pub fn profile(name: &str) -> Result<Profile, String> {
    match name {
        "mavis-m31f" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::AnthropicMessages,
                base_url: "https://agent.minimaxi.com/mavis/api/v1/llm".into(),
                model: "MiniMax-M3.1-Flash-Preview".into(),
                key_ref: "mcode-oauth".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: Some(AuthStyle::Bearer),
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: "ssh anolis 'python3 ~/.local/bin/mcode-token.py get'",
        }),
        "mavis-m3" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::AnthropicMessages,
                base_url: "https://agent.minimaxi.com/mavis/api/v1/llm".into(),
                model: "MiniMax-M3".into(),
                key_ref: "mcode-oauth".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: Some(AuthStyle::Bearer),
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: "ssh anolis 'python3 ~/.local/bin/mcode-token.py get'",
        }),
        "glm-cp" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::AnthropicMessages,
                base_url: "https://open.bigmodel.cn/api/anthropic".into(),
                model: "GLM-5.3-Flash".into(),
                key_ref: "zhipu-coding-plan".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: Some(AuthStyle::Bearer),
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: // D-20 项 6：key_cmd 抗单点——首选 bao-agent（无 escrow 明文 root-token 依赖），
                // bao-agent 不可用（缺失/锁死/网络）时回退旧 escrow 路径，两条路都是"命令名+路径"，零明文落码。
                "K=$(bao-agent get company/LLM-upstream/Zhipu/zhipu-bigmodel-CodingPlan-1 --field api_key --reveal 2>/dev/null) && [ -n \"$K\" ] && printf '%s' \"$K\" || { RT=$(cat ~/.local/share/openbao-escrow/.root-token); curl -sS -m 15 -H \"X-Vault-Token: $RT\" http://100.64.0.6:8200/v1/kv/data/company/LLM-upstream/Zhipu/zhipu-bigmodel-CodingPlan-1 | jq -r .data.data.api_key; }",
        }),
        "glm-cp-max" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::AnthropicMessages,
                base_url: "https://open.bigmodel.cn/api/anthropic".into(),
                model: "GLM-5.3".into(),
                key_ref: "zhipu-coding-plan".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: Some(AuthStyle::Bearer),
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: // 同 glm-cp：bao-agent 首选 + escrow 回退（D-20 项 6）。
                "K=$(bao-agent get company/LLM-upstream/Zhipu/zhipu-bigmodel-CodingPlan-1 --field api_key --reveal 2>/dev/null) && [ -n \"$K\" ] && printf '%s' \"$K\" || { RT=$(cat ~/.local/share/openbao-escrow/.root-token); curl -sS -m 15 -H \"X-Vault-Token: $RT\" http://100.64.0.6:8200/v1/kv/data/company/LLM-upstream/Zhipu/zhipu-bigmodel-CodingPlan-1 | jq -r .data.data.api_key; }",
        }),
        "step-5" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::OpenaiChatCompletions,
                base_url: "https://api.stepfun.com/step_plan/v1".into(),
                model: "step-5-preview".into(),
                key_ref: "stepfun-plan".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: None,
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: "jq -r '.config.providerConfigRules.providerRules[] | select(.providerId==\"new-provider\") | .config.access.apiKey' ~/.zcode/v2/provider_config.json",
        }),
        "kimi-k3" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::OpenaiChatCompletions,
                base_url: "https://api.kimi.com/coding/v1".into(),
                model: "kimi-k3".into(),
                key_ref: "kimi-coding".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: None,
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: "jq -r '.config.providerConfigRules.providerRules[] | select(.providerId==\"moonshot-kimi\") | .config.access.apiKey' ~/.zcode/v2/provider_config.json",
        }),
        "qwen30b-local" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::OpenaiChatCompletions,
                // anolis ollama（2×V100S-32G）经 ssh 隧道暴露在本机 11435。
                // 读超时不在档案级配置：走 harness 全局（UreqClient 默认 600s，
                // AF_HARNESS_HTTP_TIMEOUT_SEC 可覆盖）——30B 全思考预算单响应可超分钟级，
                // 2026-10-20 d3 臂 run_failed 系旧二进制（30s）所致，见 commit 96d7d57。
                base_url: "http://127.0.0.1:11435/v1".into(),
                model: "qwen3-coder-30b".into(),
                key_ref: "ollama-local".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: None,
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: "printf '%s' ollama-local-noop-key",
        }),
        "qwen3-4b-local" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::OpenaiChatCompletions,
                base_url: "http://127.0.0.1:11434/v1".into(),
                model: "qwen3:4b".into(),
                key_ref: "ollama-local".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: None,
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: "printf '%s' ollama-local-noop-key",
        }),
        "qwen3-8b-local" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::OpenaiChatCompletions,
                base_url: "http://127.0.0.1:11434/v1".into(),
                model: "qwen3:8b".into(),
                key_ref: "ollama-local".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: None,
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: "printf '%s' ollama-local-noop-key",
        }),
        "qwen3-8b-anolis" => Ok(Profile {
            endpoint: EndpointSpec {
                protocol: Protocol::OpenaiChatCompletions,
                // anolis ollama（GPU1）经 ssh 隧道暴露在本机 11435；本机 3070 跑 8b 太慢已弃用。
                base_url: "http://127.0.0.1:11435/v1".into(),
                model: "qwen3:8b".into(),
                key_ref: "ollama-local".into(),
                reasoning_tier: ReasoningTier::Off,
                stream: false,
                auth: None,
                max_tokens: None,
                temperature_milli: None,
                top_p_milli: None,
            },
            key_cmd: "printf '%s' ollama-local-noop-key",
        }),
        other => Err(format!(
            "未知档案 {other}（可选 mavis-m31f / mavis-m3 / glm-cp / glm-cp-max / step-5 / kimi-k3 / qwen30b-local / qwen3-4b-local / qwen3-8b-local）"
        )),
    }
}

/// ← af-bench main.rs:329-350 原样。
pub fn resolve_key(cmd: &str) -> Result<String, String> {
    let output = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .output()
        .map_err(|err| format!("密钥解析器启动失败: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "密钥解析器退出 {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(160)
                .collect::<String>()
        ));
    }
    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if key.len() < 16 {
        return Err("密钥解析器输出过短，疑似失败".into());
    }
    Ok(key)
}

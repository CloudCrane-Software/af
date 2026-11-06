//! 接口选型面（strategies17 供给注册批 A · st-b-C126，设计稿 §五.6）：
//! `pkg.iface.mcp_client@1` 记录型包装器 + 接口服从度读数。
//!
//! **为什么是 M（记录型 builtin）**：`crates/af-execline/src/record.rs:234` 的
//! `usage` 只有 `unit_calls` 计数——「agent 是否真的走了被指派的接口」要的是
//! **点名级调用序列**（`{interface, tool_name, args_digest, ok}`），现成读数面
//! 无此形状。本模块补该面：每次调用落一行，服从度由
//! [`IfaceLog::adherence`] 按指派接口清单算出。
//!
//! **执行语义（不新增能力）**：包装器把一次工具调用经 `interface` 标签执行，
//! 底层分发走 [`crate::graph_unit::wired_tool_call`] → `pkg::sweng::
//! tool_impl_name` 已接线的四臂（env/locate/view/edit，`pkg/sweng.rs:99`
//! 同型分发表）——本模块只加「点名记账」这一层。表外工具（无已接线执行面）
//! = `ok=false` 记账 + 错误观察回给模型，**不静默跳过**（fail-closed 口径：
//! 记不上账的调用不得算作服从）。
//!
//! `pkg.iface.adherence@1`（unit 类，S 部分）的 impl_ref 按设计稿落已接线
//! 判定单元 `executor:builtin/pkg.sweng.test_unit@af-execline`（容器内跑判据
//! 命令，E9001 同型）；本文件的 [`IfaceLog::adherence`] 给出同一 outputs
//! 形状的进程内计算，供单测与读数面复用——两处口径必须一致。

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::hostbridge::HostBridge;
use crate::pkg::sweng;
use crate::suite::tb::TbEnv;

/// 组件引用串（与 `af/packages/components.json` 声明逐字一致；impl_ref 锚
/// `executor:builtin/pkg.iface.mcp_client@af-execline`）。
pub const REF_MCP_CLIENT: &str = "pkg.iface.mcp_client@1";

/// 组件名（去版本段；`tool_impl_name` / `node_tool_fn` 分发表键）。
pub const NAME_MCP_CLIENT: &str = "pkg.iface.mcp_client";

/// 参数摘要（sha256 十六进制小写）。规范化序列化 = [`canonical_json`]——
/// **显式递归排序对象键**，不得依赖 `serde_json::Map` 的键序：本 workspace
/// 经依赖统一（serde_with/cedar-policy → af-credential）开启
/// `serde_json` 的 `preserve_order` 特性，`Map` = `IndexMap`（插入序），
/// `to_string` 直接序列化会随书写键序变摘要——同参数必须同摘要。
pub fn args_digest(args: &Value) -> String {
    use sha2::{Digest, Sha256};
    let canonical = canonical_json(args);
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 递归键序规范化 JSON 序列化（对象键按字节序排序；数组保序）。
fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string()));
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap_or_else(|_| "null".to_string())),
    }
}

/// 点名级调用序列（每次调用一行）。行形状 = 设计稿 §五.6 的
/// `{interface, tool_name, args_digest, ok}`。
#[derive(Debug, Default)]
pub struct IfaceLog {
    calls: Mutex<Vec<Value>>,
}

impl IfaceLog {
    /// 空序列。
    pub fn new() -> Self {
        Self::default()
    }

    /// 落一条调用记录，返回该行的 `args_digest`（观察文本回显用）。
    pub fn record(&self, interface: &str, tool_name: &str, args: &Value, ok: bool) -> String {
        let digest = args_digest(args);
        let row = json!({
            "interface": interface,
            "tool_name": tool_name,
            "args_digest": digest,
            "ok": ok,
        });
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(row);
        digest
    }

    /// 序列快照（单测/读数面；顺序 = 调用顺序）。
    pub fn calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 调用条数。
    pub fn len(&self) -> usize {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// 恒 false（[`IfaceLog::len`] 为 0 即空序列）——clippy `len_without_is_empty`。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 接口服从度读数（`pkg.iface.adherence@1` 的 outputs 形状，逐字对齐
    /// 设计稿 §五.6 的三键）：`{adherence_rate_milli, off_interface_calls,
    /// failure_cost_class}`。调用条数等原始计数经 [`IfaceLog::calls`] 取
    /// （序列即审计面，不塞进聚合读数）。
    ///
    /// - `adherence_rate_milli` = 指派接口内调用占比 ×1000（零调用 = 0——
    ///   无证据不得记满分；分母为全量调用条数）。
    /// - `off_interface_calls` = 走了指派清单之外接口的条数。
    /// - `failure_cost_class` = 失败成本分档（v1 工作定义，按**全量调用**的
    ///   失败比例：0 → `none`；>0 且 <10% → `marginal`；≥10% 且 <50% →
    ///   `material`；≥50% → `dominant`）——分档口径登记在此，读数面可复核。
    pub fn adherence(&self, assigned: &[String]) -> Value {
        let calls = self.calls();
        let total = calls.len();
        let mut on_interface = 0usize;
        let mut failed = 0usize;
        for row in &calls {
            let iface = row.get("interface").and_then(Value::as_str).unwrap_or("");
            if assigned.iter().any(|a| a == iface) {
                on_interface += 1;
            }
            if row.get("ok").and_then(Value::as_bool) == Some(false) {
                failed += 1;
            }
        }
        let off_interface = total - on_interface;
        // clippy::manual_checked_ops 清偿（基线修批 2026-11-06）：语义恒等——total==0 时 checked_div=None→0
        let rate_milli = (on_interface * 1000).checked_div(total).unwrap_or(0) as u64;
        let fail_ratio_milli = (failed * 1000).checked_div(total).unwrap_or(0) as u64;
        let class = match fail_ratio_milli {
            0 => "none",
            1..=99 => "marginal",
            100..=499 => "material",
            _ => "dominant",
        };
        json!({
            "adherence_rate_milli": rate_milli,
            "off_interface_calls": off_interface,
            "failure_cost_class": class,
        })
    }
}

/// 记录型包装器的一次调用：对 `underlying`（已接线 builtin 工具引用，如
/// `pkg.repo.grep@1`）的调用经 `interface` 标签执行并落一行记录。
///
/// 观察文本 = 首行记账摘要（`interface`/`tool_name`/`args_digest`/`ok`，
/// 点名级可复核）+ 底层调用观察原文。底层失败（表外工具或执行失败）时
/// `ok=false` 记账，错误文案原样回给模型（可重试），不吞。
pub fn mcp_client(
    log: &IfaceLog,
    interface: &str,
    underlying: &str,
    bridge: &Arc<dyn HostBridge + Send + Sync>,
    env: &Arc<TbEnv>,
    args: &Value,
) -> Result<Value, String> {
    let tool_name = sweng::name_of(underlying).to_string();
    let outcome = crate::graph_unit::wired_tool_call(underlying, bridge, env, args);
    let ok = outcome.is_ok();
    let digest = log.record(interface, &tool_name, args, ok);
    let header =
        format!("[iface] interface={interface} tool={tool_name} args_digest={digest} ok={ok}");
    match outcome {
        Ok(Value::String(text)) => Ok(Value::String(format!("{header}\n{text}"))),
        Ok(other) => Ok(Value::String(format!(
            "{header}\n{}",
            serde_json::to_string(&other).unwrap_or_else(|_| other.to_string())
        ))),
        Err(err) => Ok(Value::String(format!("{header}\n{err}"))),
    }
}

//! S17 供给注册批 A · st-b-C126 接口选型面验收（设计稿 §五.6 的 M 部分）。
//!
//! 验收面：
//! - `pkg.iface.mcp_client@1` 是**记录型包装器**：每次调用落一行
//!   `{interface, tool_name, args_digest, ok}`（`record.rs:234` 的 usage 只有
//!   `unit_calls` 计数、无点名级序列——本面补该缺口）；
//! - 底层执行**不新增能力**：经 `graph_unit::wired_tool_call` 落到
//!   `tool_impl_name` 已接线臂（本文件以 locate 臂实证：真发容器命令）；
//! - 表外工具 = `ok=false` 记账（不静默跳过——记不上账的调用不得算服从）；
//! - 服从度读数与 `pkg.iface.adherence@1` 的 outputs 形状逐字一致；
//! - 模型面声明在场（`tool_decl` 表外即图节点装载失败）。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use af_execline::hostbridge::HostBridge;
use af_execline::pkg::iface;
use af_execline::pkg::sweng;
use af_execline::suite::tb::TbEnv;
use serde_json::{json, Value};

type BridgeResult = Result<(bool, String), String>;

// ---- FakeBridge（pkg_sweng.rs 同款；Mutex 化满足 Send+Sync 持有）----

#[derive(Default)]
struct FakeBridge {
    calls: Mutex<Vec<(String, String, u32)>>,
    scripted: Mutex<VecDeque<BridgeResult>>,
}

impl FakeBridge {
    fn new(scripted: Vec<BridgeResult>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            scripted: Mutex::new(scripted.into()),
        }
    }

    fn calls(&self) -> Vec<(String, String, u32)> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl HostBridge for FakeBridge {
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> BridgeResult {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push((
            host.to_string(),
            script.to_string(),
            timeout_sec,
        ));
        self.scripted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .unwrap_or_else(|| Ok((true, String::new())))
    }
}

fn fake_env() -> TbEnv {
    TbEnv {
        container: "afbench-swev-django-django-11099-glm-cp".into(),
        task: "django__django-11099".into(),
        host: "anolis".into(),
        workdir: "/testbed".into(),
    }
}

fn locate_hit() -> BridgeResult {
    Ok((
        true,
        "django/__init__.py:1:VERSION = (5, 2)\n__AF_RC=0".into(),
    ))
}

// ============================================================
// 记录型包装器：调用序列
// ============================================================

#[test]
fn mcp_client_records_call_with_digest_and_real_execution() {
    let fake = Arc::new(FakeBridge::new(vec![locate_hit()]));
    let bridge: Arc<dyn HostBridge + Send + Sync> = Arc::clone(&fake) as _;
    let env = Arc::new(fake_env());
    let log = iface::IfaceLog::new();
    let args = json!({"query": "VERSION"});

    let out = iface::mcp_client(&log, "mcp", "pkg.repo.grep@1", &bridge, &env, &args)
        .expect("已接线底层工具应成功");

    // 观察文本 = 首行记账摘要 + 底层观察原文（点名级可复核）。
    let text = out.as_str().expect("观察应为字符串");
    assert!(
        text.starts_with("[iface] interface=mcp tool=pkg.repo.grep args_digest="),
        "{text}"
    );
    assert!(text.contains("ok=true"), "{text}");
    assert!(
        text.contains("django/__init__.py:1:VERSION = (5, 2)"),
        "{text}"
    );
    assert!(text.contains("exit=0"), "{text}");

    // 底层真跑：容器命令经 locate 臂发出（grep -rnE …）。
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "包装器不新增能力，但底层调用必须真发");
    assert!(
        calls[0].1.contains("grep -rnE -I -- '\\''VERSION'\\''"),
        "{}",
        calls[0].1
    );

    // 序列一行，四字段齐（设计稿 §五.6 行形状）。
    let rows = log.calls();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["interface"], "mcp");
    assert_eq!(row["tool_name"], "pkg.repo.grep");
    assert_eq!(row["ok"], true);
    let digest = row["args_digest"].as_str().expect("args_digest 应为字符串");
    assert_eq!(digest.len(), 64, "sha256 十六进制小写 64 字符");
    assert!(
        digest
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "{digest}"
    );
}

#[test]
fn args_digest_is_canonical_and_content_sensitive() {
    let a = iface::args_digest(&json!({"query": "VERSION", "path": "/testbed"}));
    let b = iface::args_digest(&json!({"path": "/testbed", "query": "VERSION"}));
    assert_eq!(
        a, b,
        "同参数不同书写键序应同摘要（canonical_json 显式排序；preserve_order 开启时 to_string 不保证）"
    );
    let c = iface::args_digest(&json!({"query": "VERSION", "path": "/other"}));
    assert_ne!(a, c, "参数变则摘要变");
}

#[test]
fn mcp_client_records_failure_for_unwired_underlying_tool() {
    let fake = Arc::new(FakeBridge::new(vec![]));
    let bridge: Arc<dyn HostBridge + Send + Sync> = Arc::clone(&fake) as _;
    let env = Arc::new(fake_env());
    let log = iface::IfaceLog::new();

    let out = iface::mcp_client(
        &log,
        "mcp",
        "pkg.nope.tool@1",
        &bridge,
        &env,
        &json!({"anything": 1}),
    )
    .expect("失败不回 Err——失败要以观察文本回给模型（可重试）");

    let text = out.as_str().expect("观察应为字符串");
    assert!(text.contains("ok=false"), "{text}");
    assert!(text.contains("无已接线执行面"), "{text}");

    // 表外工具仍落一行、ok=false：记不上账的调用不得算服从。
    let rows = log.calls();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["ok"], false);
    assert_eq!(rows[0]["tool_name"], "pkg.nope.tool");
    // 无容器命令发出（没有可执行面，不得假跑）。
    assert!(fake.calls().is_empty());
}

#[test]
fn mcp_client_wraps_env_arm_tool() {
    // CLI/MCP 对照的真实形态：底层是 bash 臂（pkg.aci.shell 别名）。
    let fake = Arc::new(FakeBridge::new(vec![Ok((true, "hi\n__AF_RC=0".into()))]));
    let bridge: Arc<dyn HostBridge + Send + Sync> = Arc::clone(&fake) as _;
    let env = Arc::new(fake_env());
    let log = iface::IfaceLog::new();

    let out = iface::mcp_client(
        &log,
        "cli",
        "pkg.aci.shell@1",
        &bridge,
        &env,
        &json!({"command": "echo hi"}),
    )
    .expect("env 臂应可包装");

    assert!(out.as_str().is_some_and(|t| t.contains("ok=true")));
    let rows = log.calls();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["interface"], "cli");
    assert_eq!(rows[0]["tool_name"], "pkg.aci.shell");
    assert_eq!(fake.calls().len(), 1, "bash 臂命令应真发");
}

// ============================================================
// 服从度读数（pkg.iface.adherence@1 的 outputs 形状）
// ============================================================

fn log_with(rows: &[(&str, &str, bool)]) -> iface::IfaceLog {
    let log = iface::IfaceLog::new();
    for (interface, tool, ok) in rows {
        log.record(interface, tool, &json!({"probe": tool}), *ok);
    }
    log
}

#[test]
fn adherence_matches_declared_output_shape() {
    let log = log_with(&[
        ("mcp", "pkg.repo.grep", true),
        ("mcp", "pkg.repo.read", true),
        ("cli", "pkg.repo.grep", true),
        ("mcp", "pkg.nope.tool", false),
    ]);
    let out = log.adherence(&["mcp".to_string()]);
    // 三键逐字（设计稿 §五.6 的 emits 形状）。
    // 键集恰为设计稿三键（排序后比对——serde_json 的 preserve_order 经依赖
    // 统一开启，Map=IndexMap 插入序，不得按字面序断言）。
    let mut keys: Vec<&str> = out
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "adherence_rate_milli",
            "failure_cost_class",
            "off_interface_calls"
        ],
        "键集须恰为设计稿三键"
    );
    // 3/4 在指派接口内 → 750 milli；1 条走 cli → off_interface_calls=1。
    assert_eq!(out["adherence_rate_milli"], 750);
    assert_eq!(out["off_interface_calls"], 1);
    // 1/4 失败 = 250 milli → material 档（v1 口径见 iface.rs 注释：按全量调用
    // 的失败比例分档，0=none / <10%=marginal / <50%=material / ≥50%=dominant）。
    assert_eq!(out["failure_cost_class"], "material");
}

#[test]
fn adherence_zero_calls_is_zero_not_full_marks() {
    let log = iface::IfaceLog::new();
    let out = log.adherence(&["mcp".to_string()]);
    assert_eq!(out["adherence_rate_milli"], 0, "无证据不得记满分");
    assert_eq!(out["off_interface_calls"], 0);
    assert_eq!(out["failure_cost_class"], "none");
}

#[test]
fn adherence_failure_classes_follow_documented_thresholds() {
    // 0 失败 → none；1/10=100 milli → material；1/100=10 milli → marginal。
    let rows: Vec<(&str, &str, bool)> = (0..9).map(|_| ("mcp", "pkg.repo.grep", true)).collect();
    let clean = log_with(&rows);
    assert_eq!(
        clean.adherence(&["mcp".to_string()])["failure_cost_class"],
        "none"
    );

    let mut rows10: Vec<(&str, &str, bool)> =
        (0..9).map(|_| ("mcp", "pkg.repo.grep", true)).collect();
    rows10.push(("mcp", "pkg.repo.grep", false));
    let one_of_ten = log_with(&rows10);
    assert_eq!(
        one_of_ten.adherence(&["mcp".to_string()])["failure_cost_class"],
        "material",
        "10% 失败 = material（100 milli 档）"
    );

    let mut rows100: Vec<(&str, &str, bool)> =
        (0..99).map(|_| ("mcp", "pkg.repo.grep", true)).collect();
    rows100.push(("mcp", "pkg.repo.grep", false));
    let one_of_hundred = log_with(&rows100);
    assert_eq!(
        one_of_hundred.adherence(&["mcp".to_string()])["failure_cost_class"],
        "marginal",
        "1% 失败 = marginal（10 milli 档）"
    );
}

#[test]
fn empty_interface_label_is_not_assigned() {
    // 调用时 interface 为空串（驱动面专缝 fail-loud 之外的单测面）：不得算
    // 指派接口内——空标签 = 无归属。
    let log = log_with(&[("", "pkg.repo.grep", true)]);
    let out = log.adherence(&["mcp".to_string()]);
    assert_eq!(out["adherence_rate_milli"], 0);
    assert_eq!(out["off_interface_calls"], 1);
}

// ============================================================
// 声明面（分发表 + 模型面）
// ============================================================

#[test]
fn mcp_client_is_wired_in_dispatch_and_model_face() {
    // 恒等臂：本名即执行面键（tool_impl_name 不改写——底层工具名在入参）。
    assert_eq!(
        sweng::tool_impl_name("pkg.iface.mcp_client"),
        "pkg.iface.mcp_client"
    );
    assert_eq!(iface::NAME_MCP_CLIENT, "pkg.iface.mcp_client");
    assert_eq!(iface::REF_MCP_CLIENT, "pkg.iface.mcp_client@1");
    // impl_ref 锚（components.json 声明面同串）。
    assert_eq!(
        sweng::builtin_impl_ref(iface::NAME_MCP_CLIENT),
        "executor:builtin/pkg.iface.mcp_client@af-execline"
    );
    assert!(
        !sweng::is_builtin_impl(iface::REF_MCP_CLIENT),
        "REF 串是组件引用（无 builtin 前缀）；impl_ref 锚是上一行的 builtin 形态"
    );

    // 模型面在场（缺它 = 图节点装载期「无模型面声明」红）。
    let decl =
        sweng::tool_decl("pkg.iface.mcp_client@1", "/testbed").expect("记录型包装器应有模型面声明");
    assert_eq!(decl.name, "iface.mcp_client");
    let required = decl
        .parameters
        .get("required")
        .and_then(Value::as_array)
        .expect("required 数组");
    let required: Vec<&str> = required.iter().filter_map(Value::as_str).collect();
    assert_eq!(required, vec!["interface", "tool_name"]);
}

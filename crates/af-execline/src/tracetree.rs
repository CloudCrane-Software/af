//! G5 前置：订单 trace 的 GenAI span 树核对（语义投影，落地契约-v0 §4.2）。
//!
//! OTel 字面 span 投影（`span_tree` 字段）未实现（F-第三单交付 §trace 如实
//! 申报；trace.db 仅 declaration/gap_state）。本模块按契约 §4.2「events.jsonl
//! 是真相源」口径，把订单执行产物**重建**为四层语义树并断言逐层在场与父子
//! 关系（对照表 = 契约 §4.2 投影规则，实例对照 F-第三单 F3b）：
//!
//! | 层 | span 名 | 权威载体（本工具读） |
//! |---|---|---|
//! | L1 | `invoke_workflow` | `ledger.jsonl`（驱动面 op）+ `mes-events.jsonl`（记账行） |
//! | L2 | `invoke_agent` | `harness.db` `run_row`（终态）+ `critical.jsonl` `harness.stop` |
//! | L3 | `chat` | `normal.jsonl` `harness.model`（token 记账） |
//! | L4 | `execute_tool` | `normal.jsonl` `harness.tool` + `effects.db` `effect_intent` |
//!
//! 父子边断言：L2 经 `records.jsonl`（plan_id + evidence.run_dir）挂到 L1；
//! L3/L4 事件 run_id 必须等于 L2 run_row.run_id；L4 工具轮必须有同轮 L3 模型
//! 轮；L3 token 三和必须逐位等于 records.quota（F-第三单「逐位求和一致」口
//! 径）；L4 effect_intent 的 order_id 必须与 run_row 一致；L2 终态必须与
//! critical `harness.stop` 交叉一致。失败订单（close 未过、终态 max_turns）
//! 四层仍应完整——本工具核**树形状**，不核订单成败。
//!
//! run 目录三种布局（G5 正式批扩展 + 红队 W2-C32 graph 图执行器补全；实证
//! 来源 F-第五单交付差异#1 与 F4B-F4D 图真单 runs/<item>-a<n>/nodes/ 形态）：
//!
//! | 布局 | 形态 | L2 invoke_agent |
//! |---|---|---|
//! | swev 扁平 | `runs/<item>-a<n>/harness.db` 直挂 run 根 | 恰 1 个 |
//! | report 环 | `runs/<item>-a<n>/ring/<成员>/harness.db`（reader/writer-i1…） | 每成员 1 个（多 agent 兄弟） |
//! | graph 图执行 | `runs/<item>-a<n>/nodes/<节点>/harness.db`（planner/coder/…） | 每节点 1 个（多 agent 兄弟） |
//!
//! report 环口径（对照 F5c 实测）：plan_id 为注册表编译形态
//! `plan.<包>@<版本>+<摘要>`（非订单 id）；records.quota 是**全环成员 token
//! 之和**（reader+writer 逐位闭合）；环成员工具 `effect=pure` 不经效应网关
//! ——effect_intent 只与非纯（effect≠pure）工具事件对账。
//!
//! 半途中断的历史 attempt 目录（无 harness.db / 无终态）视为旁路 run，登记
//! 不断言。一切读取/解析错误都落 findings，不 panic。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::OpenFlags;

/// L2 invoke_agent 层的重建结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLayer {
    /// run 目录名（扁平布局 `<item>-a<attempt>`；report 环 `ring/<成员>`）。
    pub run_dir_name: String,
    /// harness.db `run_row.run_id`（L3/L4 事件的挂靠键）。
    pub run_id: String,
    /// records.jsonl 的 node_id（report 环成员 = 环成员名）。
    pub node_id: String,
    /// records.jsonl 的 attempt。
    pub attempt: u64,
    /// run_row 终态 kind（completed / max_turns / …）。
    pub terminal_kind: String,
    /// run_row 终态 signal（存在时）。
    pub terminal_signal: Option<String>,
    /// L3 `harness.model` 事件数。
    pub chat_events: usize,
    /// L4 `harness.tool` 事件数。
    pub tool_events: usize,
    /// L4 非纯（effect≠pure）工具事件数——与 effect_intent 对账的口径。
    pub impure_tool_events: usize,
    /// effects.db `effect_intent` state=executed 行数。
    pub effect_intents: usize,
    /// L3 token 三和（input, output, cache_read）。
    pub tokens: (u64, u64, u64),
}

/// 一份订单 trace 的核对报告：重建的树 + findings（空 = 四层树完整）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceTreeReport {
    pub order_id: String,
    pub stable_id: String,
    pub final_state: String,
    pub ledger_entries: usize,
    pub mes_kinds: Vec<String>,
    /// swev 扁平布局的唯一 invoke_agent（report 环布局为 None）。
    pub agent: Option<AgentLayer>,
    /// report 环布局的 invoke_agent 成员（`ring/<成员>` 各一；扁平布局空）。
    pub ring: Vec<AgentLayer>,
    /// graph 图执行布局的 invoke_agent 成员（`nodes/<节点>` 各一；红队
    /// W2-C32——此前 nodes/ 布局不在任何甄别分支，图单四层树不可工具核验）。
    pub graph: Vec<AgentLayer>,
    /// 旁路 run 目录名（半途 attempt，不计入树）。
    pub side_runs: Vec<String>,
    /// 非空 = 树有断点（每条点名层级与证据）。
    pub findings: Vec<String>,
}

impl TraceTreeReport {
    /// 四层树是否完整（findings 为空）。
    pub fn ok(&self) -> bool {
        self.findings.is_empty()
    }

    /// 人读树形输出（bin 与报告用；含四层 span 名、计数与记账交叉结果）。
    pub fn render(&self) -> String {
        let mut out = String::new();
        let sid = if self.stable_id.len() > 12 {
            format!("{}…", &self.stable_id[..12])
        } else {
            self.stable_id.clone()
        };
        out.push_str(&format!(
            "invoke_workflow {} [stable_id={sid} final={}] ledger={} 行 MES=[{}]\n",
            self.order_id,
            self.final_state,
            self.ledger_entries,
            self.mes_kinds.join(","),
        ));
        let agents: Vec<&AgentLayer> = if self.ring.is_empty() && self.graph.is_empty() {
            self.agent.iter().collect()
        } else {
            self.ring.iter().chain(self.graph.iter()).collect()
        };
        match agents.len() {
            0 => out.push_str("└─ invoke_agent <缺失>\n"),
            n => {
                for (i, a) in agents.iter().enumerate() {
                    let (branch, sub) = if i + 1 < n {
                        ("├─", "│  ")
                    } else {
                        ("└─", "   ")
                    };
                    let sig = match &a.terminal_signal {
                        Some(s) => format!("/{s}"),
                        None => String::new(),
                    };
                    out.push_str(&format!(
                        "{branch} invoke_agent {} a{} [run={} run_id={} terminal={}{}]\n",
                        a.node_id, a.attempt, a.run_dir_name, a.run_id, a.terminal_kind, sig
                    ));
                    out.push_str(&format!(
                        "{sub}├─ chat ×{} [tokens in/out/cache={}/{}/{}]\n",
                        a.chat_events, a.tokens.0, a.tokens.1, a.tokens.2
                    ));
                    out.push_str(&format!(
                        "{sub}└─ execute_tool ×{} [pure={} effect_intent executed ×{}]\n",
                        a.tool_events,
                        a.tool_events - a.impure_tool_events,
                        a.effect_intents
                    ));
                }
            }
        }
        if !self.side_runs.is_empty() {
            out.push_str(&format!(
                "旁路 run（不计入树）：{}\n",
                self.side_runs.join(", ")
            ));
        }
        if self.findings.is_empty() {
            out.push_str(
                "findings：0——四层树完整（invoke_workflow→invoke_agent→chat→execute_tool）\n",
            );
        } else {
            out.push_str(&format!("findings（{} 条）：\n", self.findings.len()));
            for f in &self.findings {
                out.push_str(&format!("  - {f}\n"));
            }
        }
        out
    }
}

/// 核对一份订单目录（af-state/orders/<order-id>/）。
pub fn verify_order_dir(dir: &Path) -> TraceTreeReport {
    let mut findings: Vec<String> = Vec::new();
    let order_id = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // ---- L1 invoke_workflow：ledger.jsonl --------------------------------
    let mut stable_id = String::new();
    let mut final_state = String::new();
    let mut ledger_entries = 0usize;
    let mut close_ok = false;
    let mut mes_kinds: Vec<String> = Vec::new();
    let mut quota: Option<(u64, u64, u64)> = None;
    let mut agent: Option<AgentLayer> = None;
    let mut ring: Vec<AgentLayer> = Vec::new();
    let mut graph: Vec<AgentLayer> = Vec::new();
    let mut side_runs: Vec<String> = Vec::new();
    let mut referenced_run: Option<String> = None;
    let mut records_node = String::new();
    let mut records_attempt = 0u64;
    let mut records_plan_id = String::new();

    match read_jsonl(&dir.join("ledger.jsonl")) {
        Err(e) => findings.push(format!("L1 invoke_workflow: 读取 ledger.jsonl 失败：{e}")),
        Ok(rows) => {
            ledger_entries = rows.len();
            let mut stable_ids: BTreeSet<String> = BTreeSet::new();
            for r in &rows {
                if let Some(s) = r.get("stable_id").and_then(|v| v.as_str()) {
                    stable_ids.insert(s.to_string());
                }
            }
            if stable_ids.len() > 1 {
                findings.push(format!(
                    "L1 invoke_workflow: stable_id 漂移（{} 个不同值）",
                    stable_ids.len()
                ));
            }
            if let Some(s) = stable_ids.iter().next() {
                stable_id = s.clone();
            }
            for op in [
                "create_order",
                "bind_line",
                "attach_voucher",
                "release",
                "start",
                "complete",
            ] {
                let ok = rows.iter().any(|r| {
                    r.get("op").and_then(|v| v.as_str()) == Some(op)
                        && r.get("ok").and_then(|v| v.as_bool()) == Some(true)
                });
                if !ok {
                    findings.push(format!(
                        "L1 invoke_workflow: 驱动面 op {op} 未 ok（缺席或失败）"
                    ));
                }
            }
            close_ok = rows.iter().any(|r| {
                r.get("op").and_then(|v| v.as_str()) == Some("close")
                    && r.get("ok").and_then(|v| v.as_bool()) == Some(true)
            });
            if let Some(last) = rows.last() {
                if let Some(s) = last.get("state_after").and_then(|v| v.as_str()) {
                    final_state = s.to_string();
                }
            }
            if !final_state.is_empty() && !matches!(final_state.as_str(), "Closed" | "Completed") {
                findings.push(format!(
                    "L1 invoke_workflow: 终态 {final_state} ∉ {{Closed, Completed}}"
                ));
            }
            if close_ok && final_state != "Closed" {
                findings.push(format!(
                    "L1 invoke_workflow: close 已 ok 但终态 {final_state} ≠ Closed"
                ));
            }
        }
    }

    // ---- L1 记账面：mes-events.jsonl --------------------------------------
    match read_jsonl(&dir.join("mes-events.jsonl")) {
        Err(e) => findings.push(format!(
            "L1 invoke_workflow: 读取 mes-events.jsonl 失败：{e}"
        )),
        Ok(rows) => {
            for r in &rows {
                let kind = r.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                mes_kinds.push(kind.to_string());
                let wo = r
                    .get("work_order_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !stable_id.is_empty() && wo != stable_id {
                    findings.push(format!(
                        "L1 invoke_workflow: MES 行 work_order_id {wo} ≠ ledger stable_id（父子断链）"
                    ));
                }
            }
            for k in ["release", "start", "complete"] {
                if !mes_kinds.iter().any(|m| m == k) {
                    findings.push(format!("L1 invoke_workflow: MES 缺 {k} 行"));
                }
            }
            let mes_close = mes_kinds.iter().any(|m| m == "close");
            if close_ok && !mes_close {
                findings.push("L1 invoke_workflow: ledger close 已 ok 但 MES 缺 close 行".into());
            }
            if !close_ok && mes_close {
                findings.push("L1 invoke_workflow: MES 有 close 行但 ledger close 未 ok".into());
            }
        }
    }

    // ---- L1→L2 父子边：records.jsonl --------------------------------------
    match read_jsonl(&dir.join("records.jsonl")) {
        Err(e) => findings.push(format!(
            "L1→L2: 读取 records.jsonl 失败（plan→run 父子边无法建立）：{e}"
        )),
        Ok(rows) => match rows.last() {
            None => findings.push("L1→L2: records.jsonl 为空（父子边断）".into()),
            Some(r) => {
                records_plan_id = r
                    .get("plan_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                records_node = r
                    .get("node_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                records_attempt = r.get("attempt").and_then(|v| v.as_u64()).unwrap_or(0);
                if let Some(q) = r.pointer("/usage/quota") {
                    quota = Some((
                        q.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
                        q.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
                        q.get("cache_read").and_then(|v| v.as_u64()).unwrap_or(0),
                    ));
                }
                let run_dir = r
                    .pointer("/evidence/run_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let name = Path::new(run_dir)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.is_empty() {
                    findings.push("L1→L2: records.jsonl 无 evidence.run_dir（父子边断）".into());
                } else if dir.join("runs").join(&name).is_dir() {
                    referenced_run = Some(name);
                } else {
                    findings.push(format!("L1→L2: evidence.run_dir 指向的 runs/{name} 不存在"));
                }
            }
        },
    }
    if !records_plan_id.is_empty()
        && !order_id.is_empty()
        && records_plan_id != order_id
        && !is_registry_plan_id(&records_plan_id)
    {
        findings.push(format!(
            "L1→L2: records plan_id {records_plan_id} ≠ 订单目录 {order_id}（也非 plan.<包>@<版本>+<摘要> 注册表形态）"
        ));
    }

    // ---- L2 invoke_agent + L3 chat + L4 execute_tool（引用 run 内） --------
    let mut token_sum = (0u64, 0u64, 0u64);
    let mut chat_total = 0usize;
    if let Some(run_name) = &referenced_run {
        let run_dir = dir.join("runs").join(run_name);
        // 布局甄别：swev 扁平（harness.db 直挂 run 根）优先；report 环
        // （ring/<成员>/harness.db）与 graph 图执行（nodes/<节点>/harness.db，
        // 红队 W2-C32）次之；三者皆无按扁平路径走——打开失败的 finding 形态
        // 与既有行为一致。
        let members = ring_members(&run_dir);
        let node_members = graph_node_members(&run_dir);
        if run_dir.join("harness.db").is_file() || (members.is_empty() && node_members.is_empty())
        {
            let a = verify_agent_unit(
                &run_dir,
                run_name.clone(),
                records_node.clone(),
                records_attempt,
                &mut findings,
            );
            chat_total = a.chat_events;
            token_sum = a.tokens;
            agent = Some(a);
        } else if !node_members.is_empty() && members.is_empty() {
            // graph 图执行：每节点一个 invoke_agent 兄弟节点（records.node_id
            // 是图单元整体的线位，节点名取自 nodes/<名> 目录名——F4B-F4D 实测
            // 形态；token 闭合口径与 report 环同：records.quota = 全节点和）。
            for n in node_members {
                let display = format!("nodes/{n}");
                let a = verify_agent_unit(
                    &run_dir.join("nodes").join(&n),
                    display.clone(),
                    n,
                    records_attempt,
                    &mut findings,
                );
                chat_total += a.chat_events;
                token_sum.0 += a.tokens.0;
                token_sum.1 += a.tokens.1;
                token_sum.2 += a.tokens.2;
                graph.push(a);
            }
            if graph.is_empty() {
                findings.push("L2 invoke_agent: nodes/ 下无含 harness.db 的成员（层缺失）".into());
            }
        } else {
            // report 环：每成员一个 invoke_agent 兄弟节点（records.node_id 是
            // 环整体的线位，成员名取自 ring/<成员> 目录名——F5c 实测形态）。
            for m in members {
                let display = format!("ring/{m}");
                let a = verify_agent_unit(
                    &run_dir.join("ring").join(&m),
                    display.clone(),
                    m,
                    records_attempt,
                    &mut findings,
                );
                chat_total += a.chat_events;
                token_sum.0 += a.tokens.0;
                token_sum.1 += a.tokens.1;
                token_sum.2 += a.tokens.2;
                ring.push(a);
            }
            if ring.is_empty() {
                findings.push("L2 invoke_agent: ring/ 下无含 harness.db 的成员（层缺失）".into());
            }
        }
        if chat_total == 0 && agent.is_none() && ring.is_empty() && graph.is_empty() {
            // 单元一个都没建成（如 run 根与 ring/ 均无 harness.db）——单元级
            // findings 已点名具体缺失；此处兜底整层缺席。
            findings.push("L3 chat: 零 harness.model 事件（层缺失）".into());
        }
        if let Some(q) = quota {
            if chat_total > 0 && token_sum != q {
                findings.push(format!(
                    "L3 chat: token 三和 {}/{}/{} ≠ records.quota {}/{}/{}（记账断链）",
                    token_sum.0, token_sum.1, token_sum.2, q.0, q.1, q.2
                ));
            }
        }
    } else {
        findings.push("L2 invoke_agent: 引用 run 未建立（见 L1→L2 findings）".into());
    }

    // ---- 旁路 run（半途 attempt）------------------------------------------
    if let Ok(rd) = std::fs::read_dir(dir.join("runs")) {
        let mut names: Vec<String> = rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| Some(n) != referenced_run.as_ref())
            .collect();
        names.sort();
        side_runs = names;
    }

    TraceTreeReport {
        order_id,
        stable_id,
        final_state,
        ledger_entries,
        mes_kinds,
        agent,
        ring,
        graph,
        side_runs,
        findings,
    }
}

/// 注册表编译形态 plan_id：`plan.<包名>@<版本>+<摘要>`（report/graph 单元经
/// af-compiler-plan 编译，plan_id 不等于订单 id——父子边由 evidence.run_dir
/// 与 token 闭合承担；swev 直发形态 plan_id==订单 id 走相等的旧路径）。
fn is_registry_plan_id(plan_id: &str) -> bool {
    let Some(rest) = plan_id.strip_prefix("plan.") else {
        return false;
    };
    match rest.split_once('@') {
        Some((pkg, ver)) => !pkg.is_empty() && ver.contains('+'),
        None => false,
    }
}

/// graph 图执行节点：`run_dir/nodes/` 下含 harness.db 的子目录名（排序稳定）。
/// nodes/ 不存在或无合格成员 → 空 vec（调用方回退其他布局；红队 W2-C32——
/// graph_unit 逐节点 harness.db 落 nodes/<node_id>/，此前不在任何甄别分支）。
fn graph_node_members(run_dir: &Path) -> Vec<String> {
    let nodes = run_dir.join("nodes");
    if !nodes.is_dir() {
        return Vec::new();
    }
    let mut names: Vec<String> = std::fs::read_dir(&nodes)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && e.path().join("harness.db").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// report 环成员：`run_dir/ring/` 下含 harness.db 的子目录名（排序稳定）。
/// ring/ 不存在或无合格成员 → 空 vec（调用方回退扁平路径）。
fn ring_members(run_dir: &Path) -> Vec<String> {
    let ring = run_dir.join("ring");
    if !ring.is_dir() {
        return Vec::new();
    }
    let mut names: Vec<String> = std::fs::read_dir(&ring)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && e.path().join("harness.db").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 校验一个 invoke_agent 单元（扁平 run 根或 ring/<成员>）：L2 终态交叉 +
/// L3 chat + L4 execute_tool 的全部单元内断言。跨单元断言（token 闭合、
/// 零 chat）由调用方做。
fn verify_agent_unit(
    agent_dir: &Path,
    display_name: String,
    node_id: String,
    attempt: u64,
    findings: &mut Vec<String>,
) -> AgentLayer {
    let (run_id, terminal) = read_run_row(agent_dir, findings);
    let rid = run_id.as_deref().unwrap_or("");
    if let Some((kind, signal)) = &terminal {
        check_stop_status(agent_dir, rid, kind, signal.as_deref(), findings);
    }
    let stats = read_normal_events(agent_dir, rid, findings);
    let eff_n = read_effects(agent_dir, rid, findings);
    if stats.chat == 0 {
        findings.push(format!(
            "L3 chat: {display_name} 零 harness.model 事件（层缺失）"
        ));
    }
    if stats.tool == 0 {
        findings.push(format!(
            "L4 execute_tool: {display_name} 零 harness.tool 事件（层缺失）"
        ));
    }
    if stats.bad_rid > 0 {
        findings.push(format!(
            "L3/L4: {display_name} 有 {} 条事件 run_id ≠ invoke_agent run_id（脱链）",
            stats.bad_rid
        ));
    }
    if stats.impure_tool != eff_n {
        findings.push(format!(
            "L4 execute_tool: {display_name} effect_intent executed {eff_n} 行 ≠ 非纯（effect≠pure）工具事件 {} 条",
            stats.impure_tool
        ));
    }
    AgentLayer {
        run_dir_name: display_name,
        run_id: run_id.unwrap_or_default(),
        node_id,
        attempt,
        terminal_kind: terminal.as_ref().map(|t| t.0.clone()).unwrap_or_default(),
        terminal_signal: terminal.as_ref().and_then(|t| t.1.clone()),
        chat_events: stats.chat,
        tool_events: stats.tool,
        impure_tool_events: stats.impure_tool,
        effect_intents: eff_n,
        tokens: stats.tokens,
    }
}

fn read_jsonl(path: &PathBuf) -> Result<Vec<serde_json::Value>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| format!("{} 第 {} 行不是 JSON：{e}", path.display(), i + 1))?;
        out.push(v);
    }
    Ok(out)
}

/// 读 harness.db run_row（L2 权威载体）。返回 (run_id, (kind, signal))。
fn read_run_row(
    run_dir: &Path,
    findings: &mut Vec<String>,
) -> (Option<String>, Option<(String, Option<String>)>) {
    let conn = match rusqlite::Connection::open_with_flags(
        run_dir.join("harness.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(e) => {
            findings.push(format!("L2 invoke_agent: 打开 harness.db 失败：{e}"));
            return (None, None);
        }
    };
    let rows: Vec<(Option<String>, Option<String>)> =
        match conn.prepare("SELECT run_id, terminal_json FROM run_row") {
            Ok(mut stmt) => stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())
                .and_then(|it| {
                    it.collect::<rusqlite::Result<Vec<_>>>()
                        .map_err(|e| e.to_string())
                }),
            Err(e) => Err(e.to_string()),
        }
        .unwrap_or_else(|e| {
            findings.push(format!("L2 invoke_agent: 读 run_row 失败：{e}"));
            Vec::new()
        });
    match rows.len() {
        1 => {}
        0 => findings.push("L2 invoke_agent: run_row 零行（层缺失）".into()),
        n => findings.push(format!("L2 invoke_agent: run_row {n} 行（应恰 1）")),
    }
    let (rid, term) = rows.into_iter().next().unwrap_or((None, None));
    let rid = match rid {
        Some(r) if !r.is_empty() => Some(r),
        _ => {
            findings.push("L2 invoke_agent: run_row.run_id 缺失".into());
            None
        }
    };
    let terminal = match term {
        Some(t) if !t.is_empty() => {
            let v: serde_json::Value = match serde_json::from_str(&t) {
                Ok(v) => v,
                Err(e) => {
                    findings.push(format!("L2 invoke_agent: terminal_json 非法：{e}"));
                    return (rid, None);
                }
            };
            let kind = v
                .get("kind")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if kind.is_empty() {
                findings.push("L2 invoke_agent: terminal 缺 kind".into());
            }
            let signal = v.get("signal").and_then(|x| x.as_str()).map(String::from);
            Some((kind, signal))
        }
        _ => {
            findings.push("L2 invoke_agent: run_row terminal 缺失（invoke_agent 无终态）".into());
            None
        }
    };
    (rid, terminal)
}

/// L2 交叉核对：critical.jsonl harness.stop 的 status 与 run_row 终态一致。
fn check_stop_status(
    run_dir: &Path,
    run_id: &str,
    kind: &str,
    signal: Option<&str>,
    findings: &mut Vec<String>,
) {
    match read_jsonl(&run_dir.join("critical.jsonl")) {
        Err(e) => findings.push(format!(
            "L2 invoke_agent: 读取 critical.jsonl 失败（harness.stop 交叉核对缺源）：{e}"
        )),
        Ok(rows) => {
            let last_stop = rows
                .iter()
                .rev()
                .find(|r| r.get("kind").and_then(|v| v.as_str()) == Some("harness.stop"))
                .cloned();
            match last_stop {
                None => findings.push(
                    "L2 invoke_agent: critical.jsonl 无 harness.stop（终态无交叉证据）".into(),
                ),
                Some(s) => {
                    let sk = s
                        .pointer("/payload/status/kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if sk != kind {
                        findings.push(format!(
                            "L2 invoke_agent: critical harness.stop kind {sk} ≠ run_row 终态 {kind}"
                        ));
                    }
                    let ss = s.pointer("/payload/status/signal").and_then(|v| v.as_str());
                    if let (Some(a), Some(b)) = (ss, signal) {
                        if a != b {
                            findings.push(format!(
                                "L2 invoke_agent: harness.stop signal {a} ≠ run_row 终态 {b}"
                            ));
                        }
                    }
                    let srid = s.pointer("/payload/run_id").and_then(|v| v.as_str());
                    if !run_id.is_empty() && srid != Some(run_id) {
                        findings.push(format!(
                            "L2 invoke_agent: harness.stop run_id {srid:?} ≠ run_row {run_id}"
                        ));
                    }
                }
            }
        }
    }
}

/// 一个 agent 单元内 normal.jsonl 的 L3/L4 计数。
struct NormalStats {
    chat: usize,
    tool: usize,
    /// effect≠pure 的工具事件数（effect_intent 对账口径；report 环工具
    /// `effect=pure` 不经效应网关，不产 intent 行）。
    impure_tool: usize,
    tokens: (u64, u64, u64),
    bad_rid: usize,
}

/// 读 normal.jsonl：逐事件累计 L3 chat / L4 execute_tool 计数与 token 三和。
/// 工具轮→chat 轮的父子核对在遍历内逐条完成（事件为追加序，模型轮先于其工具轮）。
fn read_normal_events(run_dir: &Path, run_id: &str, findings: &mut Vec<String>) -> NormalStats {
    let mut s = NormalStats {
        chat: 0,
        tool: 0,
        impure_tool: 0,
        tokens: (0, 0, 0),
        bad_rid: 0,
    };
    let rows = match read_jsonl(&run_dir.join("normal.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            findings.push(format!("L3/L4: 读取 normal.jsonl 失败：{e}"));
            return s;
        }
    };
    let mut model_turns: BTreeSet<u64> = BTreeSet::new();
    for r in &rows {
        let kind = r.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let rid = r
            .pointer("/payload/run_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if (kind == "harness.model" || kind == "harness.tool")
            && !run_id.is_empty()
            && rid != run_id
        {
            s.bad_rid += 1;
        }
        match kind {
            "harness.model" => {
                s.chat += 1;
                if let Some(t) = r.pointer("/payload/turn").and_then(|v| v.as_u64()) {
                    model_turns.insert(t);
                }
                s.tokens.0 += r
                    .pointer("/payload/usage/input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                s.tokens.1 += r
                    .pointer("/payload/usage/output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                s.tokens.2 += r
                    .pointer("/payload/usage/cache_read_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }
            "harness.tool" => {
                s.tool += 1;
                if r.pointer("/payload/effect").and_then(|v| v.as_str()) != Some("pure") {
                    s.impure_tool += 1;
                }
                if let Some(t) = r.pointer("/payload/turn").and_then(|v| v.as_u64()) {
                    if !model_turns.contains(&t) {
                        findings.push(format!(
                            "L4 execute_tool: turn {t} 的工具事件无对应 chat 轮（脱离 chat 父链）"
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    s
}

/// 读 effects.db：executed 行数 + order_id 与 run_row 一致性。
fn read_effects(run_dir: &Path, run_id: &str, findings: &mut Vec<String>) -> usize {
    let conn = match rusqlite::Connection::open_with_flags(
        run_dir.join("effects.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(e) => {
            findings.push(format!("L4 execute_tool: 打开 effects.db 失败：{e}"));
            return 0;
        }
    };
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT state, order_id FROM effect_intent")
        .and_then(|mut stmt| {
            let it = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let mut v = Vec::new();
            for x in it {
                v.push(x?);
            }
            Ok(v)
        })
        .unwrap_or_else(|e| {
            findings.push(format!("L4 execute_tool: 读 effect_intent 失败：{e}"));
            Vec::new()
        });
    let executed = rows.iter().filter(|(s, _)| s == "executed").count();
    if let Some((_, oid)) = rows
        .iter()
        .find(|(_, oid)| !run_id.is_empty() && oid != run_id)
    {
        findings.push(format!(
            "L4 execute_tool: effect_intent order_id {oid} ≠ run_row run_id {run_id}"
        ));
    }
    executed
}

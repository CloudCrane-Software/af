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
//! 半途中断的历史 attempt 目录（无 harness.db / 无终态）视为旁路 run，登记
//! 不断言。一切读取/解析错误都落 findings，不 panic。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::OpenFlags;

/// L2 invoke_agent 层的重建结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLayer {
    /// run 目录名（`<item>-a<attempt>` 形态）。
    pub run_dir_name: String,
    /// harness.db `run_row.run_id`（L3/L4 事件的挂靠键）。
    pub run_id: String,
    /// records.jsonl 的 node_id。
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
    pub agent: Option<AgentLayer>,
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
        match &self.agent {
            Some(a) => {
                let sig = match &a.terminal_signal {
                    Some(s) => format!("/{s}"),
                    None => String::new(),
                };
                out.push_str(&format!(
                    "└─ invoke_agent {} a{} [run={} run_id={} terminal={}{}]\n",
                    a.node_id, a.attempt, a.run_dir_name, a.run_id, a.terminal_kind, sig
                ));
                out.push_str(&format!(
                    "   ├─ chat ×{} [tokens in/out/cache={}/{}/{}]\n",
                    a.chat_events, a.tokens.0, a.tokens.1, a.tokens.2
                ));
                out.push_str(&format!(
                    "   └─ execute_tool ×{} [effect_intent executed ×{}]\n",
                    a.tool_events, a.effect_intents
                ));
            }
            None => out.push_str("└─ invoke_agent <缺失>\n"),
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
    if !records_plan_id.is_empty() && !order_id.is_empty() && records_plan_id != order_id {
        findings.push(format!(
            "L1→L2: records plan_id {records_plan_id} ≠ 订单目录 {order_id}"
        ));
    }

    // ---- L2 invoke_agent + L3 chat + L4 execute_tool（引用 run 内） --------
    if let Some(run_name) = &referenced_run {
        let run_dir = dir.join("runs").join(run_name);
        let (run_id, terminal) = read_run_row(&run_dir, &mut findings);
        let rid = run_id.as_deref().unwrap_or("");
        if let Some((kind, signal)) = &terminal {
            check_stop_status(&run_dir, rid, kind, signal.as_deref(), &mut findings);
        }
        let (chat_n, tool_n, tokens, bad_rid_n) = read_normal_events(&run_dir, rid, &mut findings);
        let eff_n = read_effects(&run_dir, rid, &mut findings);
        if chat_n == 0 {
            findings.push("L3 chat: 零 harness.model 事件（层缺失）".into());
        }
        if tool_n == 0 {
            findings.push("L4 execute_tool: 零 harness.tool 事件（层缺失）".into());
        }
        if bad_rid_n > 0 {
            findings.push(format!(
                "L3/L4: {bad_rid_n} 条事件 run_id ≠ invoke_agent run_id（脱链）"
            ));
        }
        if let Some(q) = quota {
            if chat_n > 0 && tokens != q {
                findings.push(format!(
                    "L3 chat: token 三和 {}/{}/{} ≠ records.quota {}/{}/{}（记账断链）",
                    tokens.0, tokens.1, tokens.2, q.0, q.1, q.2
                ));
            }
        }
        if tool_n != eff_n {
            findings.push(format!(
                "L4 execute_tool: effect_intent executed {eff_n} 行 ≠ harness.tool 事件 {tool_n} 条"
            ));
        }
        agent = Some(AgentLayer {
            run_dir_name: run_name.clone(),
            run_id: run_id.unwrap_or_default(),
            node_id: records_node,
            attempt: records_attempt,
            terminal_kind: terminal.as_ref().map(|t| t.0.clone()).unwrap_or_default(),
            terminal_signal: terminal.as_ref().and_then(|t| t.1.clone()),
            chat_events: chat_n,
            tool_events: tool_n,
            effect_intents: eff_n,
            tokens,
        });
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
        side_runs,
        findings,
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

/// 读 normal.jsonl：返回 (chat 数, tool 数, token 三和, run_id 脱链数)。
/// 工具轮→chat 轮的父子核对在遍历内逐条完成（事件为追加序，模型轮先于其工具轮）。
fn read_normal_events(
    run_dir: &Path,
    run_id: &str,
    findings: &mut Vec<String>,
) -> (usize, usize, (u64, u64, u64), usize) {
    let rows = match read_jsonl(&run_dir.join("normal.jsonl")) {
        Ok(r) => r,
        Err(e) => {
            findings.push(format!("L3/L4: 读取 normal.jsonl 失败：{e}"));
            return (0, 0, (0, 0, 0), 0);
        }
    };
    let mut chat = 0usize;
    let mut tool = 0usize;
    let mut tokens = (0u64, 0u64, 0u64);
    let mut model_turns: BTreeSet<u64> = BTreeSet::new();
    let mut bad_rid = 0usize;
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
            bad_rid += 1;
        }
        match kind {
            "harness.model" => {
                chat += 1;
                if let Some(t) = r.pointer("/payload/turn").and_then(|v| v.as_u64()) {
                    model_turns.insert(t);
                }
                tokens.0 += r
                    .pointer("/payload/usage/input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                tokens.1 += r
                    .pointer("/payload/usage/output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                tokens.2 += r
                    .pointer("/payload/usage/cache_read_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }
            "harness.tool" => {
                tool += 1;
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
    (chat, tool, tokens, bad_rid)
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

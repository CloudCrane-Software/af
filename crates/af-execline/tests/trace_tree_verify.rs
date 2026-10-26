//! G5 前置：OTel GenAI span 树核对工具（[`af_execline::tracetree`]）测试。
//!
//! 数据源口径 = 落地契约-v0 §4.2「events.jsonl 是真相源，OTel 是派生非关键
//! 车道」——OTel 字面 span 投影未实现（F-第三单交付 §trace 如实申报），四层
//! 语义树的权威载体为：L1 invoke_workflow=ledger+MES、L2 invoke_agent=
//! harness.db run_row+critical harness.stop、L3 chat=normal `harness.model`、
//! L4 execute_tool=normal `harness.tool`+effects.db effect_intent。夹具形状
//! 逐字段对照 af-state/orders/F1-F3b 真实订单实读（2026-10-25）。

use std::path::{Path, PathBuf};

/// 夹具参数：默认 = 全绿最小单（1 模型轮 + 1 工具 + 完整 close 链）。
struct Fixture {
    order_id: String,
    /// (op, ok, state_after)
    ledger: Vec<(&'static str, bool, &'static str)>,
    /// MES kind 序列（work_order_id 一律 = stable_id）
    mes_kinds: Vec<&'static str>,
    /// 记录行 quota（None = 不写 records.jsonl）
    quota: Option<(u64, u64, u64)>,
    /// (turn, in, out, cache) 模型轮
    models: Vec<(u64, u64, u64, u64)>,
    /// 工具调用的 turn 序列
    tool_turns: Vec<u64>,
    /// run_row 终态（None = terminal_json NULL）
    terminal: Option<&'static str>,
    /// critical harness.stop 的 status JSON（None = 不写 critical.jsonl）
    stop_status: Option<String>,
    /// effects.db executed 行数（默认 = 工具数）
    effects: Option<usize>,
    /// 额外的旁路 run 目录名
    side_run: Option<&'static str>,
}

impl Default for Fixture {
    fn default() -> Self {
        Fixture {
            order_id: "T1-order".to_string(),
            ledger: vec![
                ("create_order", true, "Created"),
                ("bind_line", true, "Created"),
                ("attach_voucher", true, "Created"),
                ("release", true, "Released"),
                ("start", true, "InProgress"),
                ("attach_artifact", true, "InProgress"),
                ("complete", true, "Completed"),
                ("close", true, "Closed"),
            ],
            mes_kinds: vec!["release", "start", "complete", "close"],
            quota: Some((100, 20, 40)),
            models: vec![(0, 60, 12, 24), (1, 40, 8, 16)],
            tool_turns: vec![0, 1],
            terminal: Some(r#"{"kind":"completed","signal":"TASK_COMPLETE"}"#),
            stop_status: Some(r#"{"kind":"completed","signal":"TASK_COMPLETE"}"#.into()),
            effects: None,
            side_run: None,
        }
    }
}

fn sid(f: &Fixture) -> String {
    format!("sid-{}", f.order_id)
}

fn run_name(f: &Fixture) -> String {
    format!("{}-a1", f.order_id)
}

fn run_id(f: &Fixture) -> String {
    format!("{}-run", f.order_id)
}

fn write_order(f: &Fixture) -> PathBuf {
    // 父目录带 pid 防并发撞名；订单目录名恰 = order_id（真实 af-state 形态：
    // verify_order_dir 以目录名为订单 id 与 records.plan_id 对账）。
    let parent = std::env::temp_dir().join(format!("af-tracetree-test-{}", std::process::id()));
    std::fs::create_dir_all(&parent).unwrap();
    let dir = parent.join(&f.order_id);
    let _ = std::fs::remove_dir_all(&dir);
    let run_dir = dir.join("runs").join(run_name(f));
    std::fs::create_dir_all(&run_dir).unwrap();

    // ledger.jsonl
    let stable = sid(f);
    let ledger: Vec<String> = f
        .ledger
        .iter()
        .enumerate()
        .map(|(i, (op, ok, state))| {
            format!(
                concat!(
                    r#"{{"seq":{seq},"ts_ms":1,"order_id":"{oid}","stable_id":"{sid}","op":"{op}","#,
                    r#""idem_key":"{sid}/{op}","ok":{ok},"state_after":"{state}","order_after":{{}}}}"#
                ),
                seq = i + 1,
                oid = f.order_id,
                sid = stable,
                op = op,
                ok = ok,
                state = state,
            )
        })
        .collect();
    std::fs::write(dir.join("ledger.jsonl"), ledger.join("\n") + "\n").unwrap();

    // mes-events.jsonl
    let mes: Vec<String> = f
        .mes_kinds
        .iter()
        .enumerate()
        .map(|(i, k)| {
            format!(
                concat!(
                    r#"{{"work_order_id":"{sid}","occurred_at":{ts},"event_id":"{sid}:{k}:{i}","#,
                    r#""line_id":"line-A","station_id":"af-order-cli","kind":"{k}","quantity":1}}"#
                ),
                sid = stable,
                ts = i + 1,
                k = k,
                i = i,
            )
        })
        .collect();
    std::fs::write(dir.join("mes-events.jsonl"), mes.join("\n") + "\n").unwrap();

    // records.jsonl（单行；evidence.run_dir = 真实订单同款相对路径形态）
    if let Some((qi, qo, qc)) = f.quota {
        let rec = serde_json::json!({
            "plan_id": f.order_id,
            "node_id": "line-A",
            "item_id": "i",
            "attempt": 1,
            "host_alias": "h",
            "unit_id": "u",
            "unit_version": "0.1.0",
            "status": "completed",
            "verdict": true,
            "evidence": {
                "run_dir": format!("af-state/orders/{}/runs/{}", f.order_id, run_name(f)),
                "evidence_digest": "d",
            },
            "gate": "ok",
            "usage": {
                "turns": 1,
                "unit_calls": 1,
                "http_retries": 0,
                "quota": {"input": qi, "output": qo, "cache_read": qc},
            },
        });
        std::fs::write(dir.join("records.jsonl"), format!("{rec}\n")).unwrap();
    }

    // normal.jsonl：model 轮 + tool 轮（run_id 统一）
    let mut normal: Vec<String> = Vec::new();
    for (idx, (t, i, o, c)) in f.models.iter().enumerate() {
        normal.push(format!(
            concat!(
                r#"{{"seq":{seq},"level":"normal","kind":"harness.model","payload":{{"run_id":"{rid}","#,
                r#""turn":{t},"model":"m","finish":"tool_use","usage":{{"input_tokens":{i},"output_tokens":{o},"cache_read_tokens":{c}}}}},"ts_ms":{ts}}}"#
            ),
            seq = idx,
            ts = idx + 1,
            rid = run_id(f),
            t = t,
            i = i,
            o = o,
            c = c,
        ));
    }
    let n_models = f.models.len();
    for (idx, t) in f.tool_turns.iter().enumerate() {
        normal.push(format!(
            concat!(
                r#"{{"seq":{seq},"level":"normal","kind":"harness.tool","payload":{{"run_id":"{rid}","#,
                r#""turn":{t},"tool":"bash","effect":"external_once","is_error":false,"error_code":""}},"ts_ms":{ts}}}"#
            ),
            seq = n_models + idx,
            ts = n_models + idx + 1,
            rid = run_id(f),
            t = t,
        ));
    }
    std::fs::write(run_dir.join("normal.jsonl"), normal.join("\n") + "\n").unwrap();

    // critical.jsonl：harness.stop（status 与 run_row 终态交叉一致）
    if let Some(status) = &f.stop_status {
        let crit = format!(
            concat!(
                r#"{{"seq":0,"level":"critical","kind":"harness.stop","payload":{{"run_id":"{rid}","#,
                r#""turn":9,"status":{status}}},"ts_ms":99}}"#
            ),
            rid = run_id(f),
            status = status,
        );
        std::fs::write(run_dir.join("critical.jsonl"), crit + "\n").unwrap();
    }

    // harness.db：run_row 单行（工具只查 run_id/terminal_json/order_id 三列，
    // 真实 schema 其余列不参与断言）
    let hdb = rusqlite::Connection::open(run_dir.join("harness.db")).unwrap();
    hdb.execute(
        "CREATE TABLE run_row (run_id TEXT, terminal_json TEXT, order_id TEXT)",
        [],
    )
    .unwrap();
    hdb.execute(
        "INSERT INTO run_row VALUES (?1, ?2, ?3)",
        rusqlite::params![run_id(f), f.terminal, format!("{}-run", f.order_id)],
    )
    .unwrap();

    // effects.db：effect_intent（工具只查 state/order_id 两列）
    let edb = rusqlite::Connection::open(run_dir.join("effects.db")).unwrap();
    edb.execute(
        "CREATE TABLE effect_intent (workorder_id TEXT, seq INTEGER, payload_json TEXT, \
         payload_sha256 TEXT, state TEXT, order_id TEXT)",
        [],
    )
    .unwrap();
    let n_eff = f.effects.unwrap_or(f.tool_turns.len());
    for i in 0..n_eff {
        edb.execute(
            "INSERT INTO effect_intent VALUES (?1, ?2, '{}', 'sha', 'executed', ?3)",
            rusqlite::params![format!("wo-{i}"), i as i64, run_id(f)],
        )
        .unwrap();
    }

    // 旁路 run 目录（有 normal.jsonl 无 harness.db —— 半途中断形态）
    if let Some(side) = f.side_run {
        let sdir = dir.join("runs").join(side);
        std::fs::create_dir_all(&sdir).unwrap();
        std::fs::write(
            sdir.join("normal.jsonl"),
            format!(
                concat!(
                    r#"{{"seq":0,"level":"normal","kind":"harness.model","payload":{{"run_id":"{rid}-x","#,
                    r#""turn":0,"model":"m","finish":"x","usage":{{"input_tokens":1,"output_tokens":1,"cache_read_tokens":1}}}},"ts_ms":1}}"#
                ),
                rid = run_id(f),
            ),
        )
        .unwrap();
    }
    dir
}

fn findings(dir: &Path) -> Vec<String> {
    af_execline::tracetree::verify_order_dir(dir).findings
}

#[test]
fn happy_path_full_tree_ok() {
    let dir = write_order(&Fixture::default());
    let r = af_execline::tracetree::verify_order_dir(&dir);
    assert!(
        r.findings.is_empty(),
        "全绿单不应有 findings：{:#?}",
        r.findings
    );
    assert_eq!(r.order_id, "T1-order");
    assert_eq!(r.final_state, "Closed");
    assert_eq!(r.ledger_entries, 8);
    assert_eq!(r.mes_kinds, vec!["release", "start", "complete", "close"]);
    assert_eq!(r.agent.as_ref().unwrap().chat_events, 2);
    assert_eq!(r.agent.as_ref().unwrap().tool_events, 2);
    assert_eq!(r.agent.as_ref().unwrap().effect_intents, 2);
    assert_eq!(r.agent.as_ref().unwrap().tokens, (100, 20, 40));
    assert_eq!(r.agent.as_ref().unwrap().terminal_kind, "completed");
}

/// F3a2 形态（真实失败订单）：close ok=false、终态 Completed、MES 无 close、
/// 终态 max_turns——四层树仍完整，应零 findings。
#[test]
fn failed_order_shape_still_full_tree() {
    let f = Fixture {
        order_id: "T2-failed".to_string(),
        ledger: vec![
            ("create_order", true, "Created"),
            ("bind_line", true, "Created"),
            ("attach_voucher", true, "Created"),
            ("release", true, "Released"),
            ("start", true, "InProgress"),
            ("attach_artifact", true, "InProgress"),
            ("complete", true, "Completed"),
            ("close", false, "Completed"),
        ],
        mes_kinds: vec!["release", "start", "complete"],
        quota: Some((60, 12, 24)),
        models: vec![(0, 60, 12, 24)],
        tool_turns: vec![0],
        terminal: Some(r#"{"kind":"max_turns"}"#),
        stop_status: Some(r#"{"kind":"max_turns"}"#.into()),
        effects: None,
        side_run: None,
    };
    let dir = write_order(&f);
    let r = af_execline::tracetree::verify_order_dir(&dir);
    assert!(
        r.findings.is_empty(),
        "失败订单四层仍应全过：{:#?}",
        r.findings
    );
    assert_eq!(r.final_state, "Completed");
    assert_eq!(r.agent.as_ref().unwrap().terminal_kind, "max_turns");
}

/// L1 拦截：ledger 有 ok=true 的 close，但 MES 缺 close 行。
#[test]
fn l1_close_ok_but_mes_close_missing() {
    let f = Fixture {
        order_id: "T3-mes".to_string(),
        mes_kinds: vec!["release", "start", "complete"],
        ..Fixture::default()
    };
    let fs = findings(&write_order(&f));
    assert!(
        fs.iter().any(|x| x.contains("close") && x.contains("MES")),
        "应点名 MES 缺 close：{fs:#?}"
    );
}

/// L1 拦截：start op 失败（ok=false）——驱动面不完整。
#[test]
fn l1_start_op_not_ok() {
    let f = Fixture {
        order_id: "T4-start".to_string(),
        ledger: vec![
            ("create_order", true, "Created"),
            ("bind_line", true, "Created"),
            ("attach_voucher", true, "Created"),
            ("release", true, "Released"),
            ("start", false, "Released"),
        ],
        mes_kinds: vec!["release"],
        quota: None,
        models: vec![],
        tool_turns: vec![],
        terminal: None,
        stop_status: None,
        effects: Some(0),
        side_run: None,
    };
    let fs = findings(&write_order(&f));
    assert!(
        fs.iter().any(|x| x.contains("start") && x.contains("ok")),
        "应点名 start 未 ok：{fs:#?}"
    );
}

/// L1 拦截：MES 行的 work_order_id 与 ledger stable_id 不一致（父子断链）。
#[test]
fn l1_mes_work_order_id_mismatch() {
    let f = Fixture {
        order_id: "TD-mes".to_string(),
        mes_kinds: vec!["release", "start", "complete", "close"],
        ..Fixture::default()
    };
    let dir = write_order(&f);
    let mes = std::fs::read_to_string(dir.join("mes-events.jsonl")).unwrap();
    std::fs::write(
        dir.join("mes-events.jsonl"),
        mes.replace("sid-TD-mes", "sid-someone-else"),
    )
    .unwrap();
    let fs = findings(&dir);
    assert!(
        fs.iter().any(|x| x.contains("work_order_id")),
        "应点名 MES work_order_id 断链：{fs:#?}"
    );
}

/// L2 拦截：run_row 终态与 critical harness.stop 的 status 不一致。
#[test]
fn l2_terminal_stop_mismatch() {
    let f = Fixture {
        order_id: "T5-stop".to_string(),
        stop_status: Some(r#"{"kind":"max_turns"}"#.into()),
        ..Fixture::default()
    };
    let fs = findings(&write_order(&f));
    assert!(
        fs.iter().any(|x| x.contains("harness.stop")),
        "应点名 stop 与终态不一致：{fs:#?}"
    );
}

/// L2 拦截：run_row terminal_json 为 NULL（run 无终态）。
#[test]
fn l2_terminal_missing() {
    let f = Fixture {
        order_id: "T6-term".to_string(),
        terminal: None,
        stop_status: None,
        ..Fixture::default()
    };
    let fs = findings(&write_order(&f));
    assert!(
        fs.iter().any(|x| x.contains("terminal")),
        "应点名终态缺失：{fs:#?}"
    );
}

/// L3 拦截：事件流里出现不属于 run_row.run_id 的事件（chat 脱离 invoke_agent）。
#[test]
fn l3_event_run_id_mismatch() {
    let f = Fixture {
        order_id: "TC-rid".to_string(),
        ..Fixture::default()
    };
    let dir = write_order(&f);
    let normal_path = dir.join("runs").join("TC-rid-a1").join("normal.jsonl");
    let normal = std::fs::read_to_string(&normal_path).unwrap();
    std::fs::write(&normal_path, normal.replace("TC-rid-run", "other-run")).unwrap();
    let fs = findings(&dir);
    assert!(
        fs.iter().any(|x| x.contains("run_id")),
        "应点名事件 run_id 脱链：{fs:#?}"
    );
}

/// L3 拦截：chat token 三和 ≠ records.quota（记账断链，F-第三单口径的逐位核对）。
#[test]
fn l3_token_sum_mismatch() {
    let f = Fixture {
        order_id: "T7-token".to_string(),
        quota: Some((101, 20, 40)), // 实际模型和 = 100
        ..Fixture::default()
    };
    let fs = findings(&write_order(&f));
    assert!(
        fs.iter().any(|x| x.contains("quota")),
        "应点名 token 和与 quota 不一致：{fs:#?}"
    );
}

/// L4 拦截：effects.db executed 行数少于 harness.tool 事件数。
#[test]
fn l4_effect_count_mismatch() {
    let f = Fixture {
        order_id: "T8-eff".to_string(),
        effects: Some(1), // 工具事件 2 条
        ..Fixture::default()
    };
    let fs = findings(&write_order(&f));
    assert!(
        fs.iter().any(|x| x.contains("effect")),
        "应点名 effect_intent 数不齐：{fs:#?}"
    );
}

/// L4 拦截：工具事件的 turn 没有对应 chat 轮（execute_tool 脱离 chat）。
#[test]
fn l4_tool_turn_without_chat() {
    let f = Fixture {
        order_id: "T9-turn".to_string(),
        tool_turns: vec![0, 7], // turn 7 无模型轮
        ..Fixture::default()
    };
    let fs = findings(&write_order(&f));
    assert!(
        fs.iter().any(|x| x.contains("turn")),
        "应点名工具轮脱离 chat：{fs:#?}"
    );
}

/// 旁路 run（半途中断的历史 attempt）不计入树、不产生 findings。
#[test]
fn side_run_ignored_without_findings() {
    let f = Fixture {
        order_id: "TA-side".to_string(),
        side_run: Some("TA-side-a0"),
        ..Fixture::default()
    };
    let dir = write_order(&f);
    let r = af_execline::tracetree::verify_order_dir(&dir);
    assert!(
        r.findings.is_empty(),
        "旁路 run 不应产生 findings：{:#?}",
        r.findings
    );
    assert_eq!(r.side_runs, vec!["TA-side-a0".to_string()]);
}

/// 缺 records.jsonl：L1→L2 父子边无法建立，应点名。
#[test]
fn missing_records_breaks_l1_l2_edge() {
    let f = Fixture {
        order_id: "TB-rec".to_string(),
        quota: None,
        ..Fixture::default()
    };
    let fs = findings(&write_order(&f));
    assert!(
        fs.iter().any(|x| x.contains("records")),
        "应点名 records.jsonl 缺失：{fs:#?}"
    );
}

/// 渲染包含四层 span 名与计数（供人读的树形输出）。
#[test]
fn render_contains_span_tree() {
    let f = Fixture {
        order_id: "TE-render".to_string(),
        ..Fixture::default()
    };
    let dir = write_order(&f);
    let r = af_execline::tracetree::verify_order_dir(&dir);
    let out = r.render();
    for layer in ["invoke_workflow", "invoke_agent", "chat", "execute_tool"] {
        assert!(out.contains(layer), "渲染应含 {layer}：\n{out}");
    }
    assert!(out.contains("chat ×2"), "渲染应含 chat 计数：\n{out}");
    assert!(out.contains("execute_tool ×2"), "渲染应含工具计数：\n{out}");
}

// ===== report 环布局（runs/<item>-a<n>/ring/<成员>/，F5c 实测形态） ============

/// 环成员参数：（成员名, run_id, 模型轮 (turn,in,out,cache), 工具 effect）。
type RingMember<'a> = (&'a str, &'a str, (u64, u64, u64, u64), &'a str);

/// report 环订单夹具：两个环成员（reader/writer-i1），工具 effect=pure 不产
/// effect_intent 行；records.quota = 全环 token 之和；plan_id 为注册表编译形态。
struct RingFixture {
    order_id: String,
    /// 成员（名, run_id, 模型轮, 工具 effect）
    members: Vec<RingMember<'static>>,
    /// records.quota（None = 用全环真和）
    quota: Option<(u64, u64, u64)>,
    plan_id: String,
}

impl Default for RingFixture {
    fn default() -> Self {
        // F5c 实测值：reader 1893/4835 + writer-i1 1205/10002 → 和 3098/14837
        RingFixture {
            order_id: "TR-ring".to_string(),
            members: vec![
                ("reader", "report-reader", (0, 1893, 4835, 0), "pure"),
                ("writer-i1", "report-writer-i1", (0, 1205, 10002, 0), "pure"),
            ],
            quota: None,
            plan_id: "plan.paper_report@1.0+13a1799adc04".to_string(),
        }
    }
}

fn write_ring_order(f: &RingFixture) -> PathBuf {
    let parent =
        std::env::temp_dir().join(format!("af-tracetree-ring-test-{}", std::process::id()));
    std::fs::create_dir_all(&parent).unwrap();
    let dir = parent.join(&f.order_id);
    let _ = std::fs::remove_dir_all(&dir);
    let run_dir = dir.join("runs").join("swev-item-glm-cp-a1");
    for (name, ..) in &f.members {
        std::fs::create_dir_all(run_dir.join("ring").join(name)).unwrap();
    }
    let stable = format!("sid-{}", f.order_id);

    // ledger.jsonl（8 op 全 ok）
    let ledger: Vec<String> = [
        ("create_order", "Created"),
        ("bind_line", "Created"),
        ("attach_voucher", "Created"),
        ("release", "Released"),
        ("start", "InProgress"),
        ("attach_artifact", "InProgress"),
        ("complete", "Completed"),
        ("close", "Closed"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (op, st))| {
        format!(
            r#"{{"seq":{seq},"ts_ms":{ts},"order_id":"{oid}","stable_id":"{sid}","op":"{op}","idem_key":"k","ok":true,"state_after":"{st}","order_after":{{}}}}"#,
            seq = i + 1,
            ts = i + 1,
            oid = f.order_id,
            sid = stable,
            op = op,
            st = st,
        )
    })
    .collect();
    std::fs::write(dir.join("ledger.jsonl"), ledger.join("\n") + "\n").unwrap();

    // mes-events.jsonl（四类跃迁）
    let mes: Vec<String> = ["release", "start", "complete", "close"]
        .iter()
        .enumerate()
        .map(|(i, k)| {
            format!(
                r#"{{"work_order_id":"{sid}","occurred_at":{ts},"event_id":"{sid}:{k}:{i}","line_id":"line-A","station_id":"af-order-cli","kind":"{k}","quantity":1}}"#,
                sid = stable,
                ts = i + 1,
                k = k,
                i = i,
            )
        })
        .collect();
    std::fs::write(dir.join("mes-events.jsonl"), mes.join("\n") + "\n").unwrap();

    // 环成员产物：harness.db / normal.jsonl / critical.jsonl / effects.db
    let mut sum = (0u64, 0u64, 0u64);
    for (name, rid, (t, ti, to, tc), effect) in &f.members {
        let mdir = run_dir.join("ring").join(name);
        let model_line = format!(
            concat!(
                r#"{{"seq":0,"level":"normal","kind":"harness.model","payload":{{"run_id":"{rid}","#,
                r#""turn":{t},"model":"m","finish":"tool_use","usage":{{"input_tokens":{ti},"output_tokens":{to},"cache_read_tokens":{tc}}}}},"ts_ms":1}}"#
            ),
            rid = rid,
            t = t,
            ti = ti,
            to = to,
            tc = tc,
        );
        let tool_line = format!(
            concat!(
                r#"{{"seq":1,"level":"normal","kind":"harness.tool","payload":{{"run_id":"{rid}","#,
                r#""turn":{t},"tool":"submit_report","effect":"{effect}","is_error":false,"error_code":""}},"ts_ms":2}}"#
            ),
            rid = rid,
            t = t,
            effect = effect,
        );
        std::fs::write(
            mdir.join("normal.jsonl"),
            format!("{model_line}\n{tool_line}\n"),
        )
        .unwrap();
        std::fs::write(
            mdir.join("critical.jsonl"),
            format!(
                r#"{{"seq":0,"level":"critical","kind":"harness.stop","payload":{{"run_id":"{rid}","turn":1,"status":{{"kind":"completed","signal":"REPORT_DONE"}}}},"ts_ms":9}}"#
, rid = rid
            ),
        )
        .unwrap();
        let hdb = rusqlite::Connection::open(mdir.join("harness.db")).unwrap();
        hdb.execute(
            "CREATE TABLE run_row (run_id TEXT, terminal_json TEXT, order_id TEXT)",
            [],
        )
        .unwrap();
        hdb.execute(
            "INSERT INTO run_row VALUES (?1, ?2, ?3)",
            rusqlite::params![
                rid,
                r#"{"kind":"completed","signal":"REPORT_DONE"}"#,
                format!("{}-run", f.order_id)
            ],
        )
        .unwrap();
        let edb = rusqlite::Connection::open(mdir.join("effects.db")).unwrap();
        edb.execute(
            "CREATE TABLE effect_intent (workorder_id TEXT, seq INTEGER, payload_json TEXT, \
             payload_sha256 TEXT, state TEXT, order_id TEXT)",
            [],
        )
        .unwrap();
        sum.0 += ti;
        sum.1 += to;
        sum.2 += tc;
    }

    // records.jsonl：plan_id 注册表形态 + 全环 quota 和
    let (qi, qo, qc) = f.quota.unwrap_or(sum);
    let rec = serde_json::json!({
        "plan_id": f.plan_id,
        "node_id": "line-A",
        "item_id": "item",
        "attempt": 1,
        "host_alias": "h",
        "unit_id": "report-unit",
        "unit_version": "0.1.0",
        "status": "completed",
        "verdict": true,
        "evidence": {
            "run_dir": format!("af-state/orders/{}/runs/swev-item-glm-cp-a1", f.order_id),
            "evidence_digest": null,
        },
        "gate": "ok",
        "usage": {"turns": 2, "unit_calls": 2, "http_retries": 0,
                  "quota": {"input": qi, "output": qo, "cache_read": qc}},
    });
    std::fs::write(dir.join("records.jsonl"), format!("{rec}\n")).unwrap();
    dir
}

/// F5c 形态全绿：两环成员各 1 chat + 1 pure 工具、token 全环闭合、
/// plan_id 注册表形态放行、effect=pure 不产 intent 不算脱账。
#[test]
fn report_ring_layout_full_tree_ok() {
    let dir = write_ring_order(&RingFixture::default());
    let r = af_execline::tracetree::verify_order_dir(&dir);
    assert!(
        r.findings.is_empty(),
        "report 环全绿单不应有 findings：{:#?}",
        r.findings
    );
    assert!(r.agent.is_none(), "环布局不应建扁平 agent");
    assert_eq!(r.ring.len(), 2);
    assert_eq!(r.ring[0].node_id, "reader");
    assert_eq!(r.ring[0].run_dir_name, "ring/reader");
    assert_eq!(r.ring[0].run_id, "report-reader");
    assert_eq!(r.ring[0].tokens, (1893, 4835, 0));
    assert_eq!(r.ring[0].impure_tool_events, 0);
    assert_eq!(r.ring[0].effect_intents, 0);
    assert_eq!(r.ring[1].tokens, (1205, 10002, 0));
    let out = r.render();
    assert!(out.contains("ring/reader"), "渲染应含环成员：\n{out}");
    assert!(out.contains("ring/writer-i1"), "渲染应含环成员：\n{out}");
    assert!(out.contains("pure=1"), "渲染应含 pure 工具计数：\n{out}");
}

/// 环布局 token 闭合是跨成员求和：quota 少 1 也必须点名记账断链。
#[test]
fn report_ring_token_sum_must_close_across_members() {
    let f = RingFixture {
        order_id: "TR-tok".to_string(),
        quota: Some((3098, 14837, 1)), // 真和 cache=0，此处错报 1
        ..RingFixture::default()
    };
    let fs = {
        let dir = write_ring_order(&f);
        af_execline::tracetree::verify_order_dir(&dir).findings
    };
    assert!(
        fs.iter().any(|x| x.contains("quota")),
        "应点名跨成员 token 和与 quota 不一致：{fs:#?}"
    );
}

/// 非纯工具（effect=external_once）必须与 effect_intent 对账（环内同口径）。
#[test]
fn report_ring_impure_tool_needs_intent_row() {
    let f = RingFixture {
        order_id: "TR-imp".to_string(),
        members: vec![
            ("reader", "report-reader", (0, 100, 200, 0), "pure"),
            ("planner", "report-planner", (0, 10, 20, 0), "external_once"),
        ],
        quota: None,
        ..RingFixture::default()
    };
    let fs = {
        let dir = write_ring_order(&f);
        af_execline::tracetree::verify_order_dir(&dir).findings
    };
    assert!(
        fs.iter()
            .any(|x| x.contains("effect") && x.contains("ring/planner")),
        "应点名 planner 非纯工具无 intent 行：{fs:#?}"
    );
}

/// plan_id 既非订单 id 也非注册表形态仍要拦（父子边检查有牙）。
#[test]
fn report_ring_foreign_plan_id_still_flagged() {
    let f = RingFixture {
        order_id: "TR-pid".to_string(),
        plan_id: "someone-elses-order".to_string(),
        ..RingFixture::default()
    };
    let fs = {
        let dir = write_ring_order(&f);
        af_execline::tracetree::verify_order_dir(&dir).findings
    };
    assert!(
        fs.iter().any(|x| x.contains("plan_id")),
        "应点名 plan_id 断链：{fs:#?}"
    );
}

/// 环成员 chat 层缺失（零模型轮）必须逐成员点名。
#[test]
fn report_ring_member_without_chat_flagged() {
    let f = RingFixture {
        order_id: "TR-chat".to_string(),
        members: vec![
            ("reader", "report-reader", (0, 100, 200, 0), "pure"),
            ("writer-i1", "report-writer-i1", (0, 0, 0, 0), "pure"), // 零 token 伪零轮
        ],
        quota: None,
        ..RingFixture::default()
    };
    // 直接删 writer-i1 的模型轮事件，保留工具轮 → chat 零 + 工具脱链双 findings
    let dir = write_ring_order(&f);
    let nj = dir.join("runs/swev-item-glm-cp-a1/ring/writer-i1/normal.jsonl");
    let text = std::fs::read_to_string(&nj).unwrap();
    let tool_line = text.lines().last().unwrap().to_string();
    std::fs::write(&nj, format!("{tool_line}\n")).unwrap();
    let fs = af_execline::tracetree::verify_order_dir(&dir).findings;
    assert!(
        fs.iter()
            .any(|x| x.contains("ring/writer-i1") && x.contains("harness.model")),
        "应点名 writer-i1 零模型轮：{fs:#?}"
    );
}

// ============================================================
// 红队 W2-C32：graph 图执行布局（runs/<item>-a<n>/nodes/<节点>/harness.db）
// ——此前 nodes/ 不在任何甄别分支，图单四层 trace 不可工具核验（F4B-F4D
// 实测形态：每节点 harness.db/normal.jsonl/critical.jsonl/effects.db，
// run 根无 harness.db）。
// ============================================================

type GraphNode<'a> = (&'a str, &'a str, (u64, u64, u64, u64), &'a str);

/// graph 图执行订单夹具：两节点（planner/coder），各 1 模型轮 + 1 工具轮；
/// records.quota = 全节点 token 之和；plan_id 注册表编译形态（F4D 同款）。
struct GraphFixture {
    order_id: String,
    /// 节点（名, run_id, 模型轮, 工具 effect）
    nodes: Vec<GraphNode<'static>>,
    /// records.quota（None = 用全节点真和）
    quota: Option<(u64, u64, u64)>,
    plan_id: String,
}

impl Default for GraphFixture {
    fn default() -> Self {
        GraphFixture {
            order_id: "TR-graph".to_string(),
            nodes: vec![
                ("planner", "graph-planner", (0, 100, 20, 40), "external_once"),
                ("coder", "graph-coder", (1, 200, 40, 80), "external_once"),
            ],
            quota: None,
            plan_id: "plan.sweng_sop_full@1.1+348a0142d883".to_string(),
        }
    }
}

fn write_graph_order(f: &GraphFixture) -> PathBuf {
    let parent =
        std::env::temp_dir().join(format!("af-tracetree-graph-test-{}", std::process::id()));
    std::fs::create_dir_all(&parent).unwrap();
    let dir = parent.join(&f.order_id);
    let _ = std::fs::remove_dir_all(&dir);
    let run_dir = dir.join("runs").join("graph-item-glm-cp-a1");
    for (name, ..) in &f.nodes {
        std::fs::create_dir_all(run_dir.join("nodes").join(name)).unwrap();
    }
    let stable = format!("sid-{}", f.order_id);

    // ledger.jsonl（8 op 全 ok）
    let ledger: Vec<String> = [
        ("create_order", "Created"),
        ("bind_line", "Created"),
        ("attach_voucher", "Created"),
        ("release", "Released"),
        ("start", "InProgress"),
        ("attach_artifact", "InProgress"),
        ("complete", "Completed"),
        ("close", "Closed"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (op, st))| {
        format!(
            r#"{{"seq":{seq},"ts_ms":{ts},"order_id":"{oid}","stable_id":"{sid}","op":"{op}","idem_key":"k","ok":true,"state_after":"{st}","order_after":{{}}}}"#,
            seq = i + 1,
            ts = i + 1,
            oid = f.order_id,
            sid = stable,
            op = op,
            st = st,
        )
    })
    .collect();
    std::fs::write(dir.join("ledger.jsonl"), ledger.join("\n") + "\n").unwrap();

    // mes-events.jsonl（四类跃迁）
    let mes: Vec<String> = ["release", "start", "complete", "close"]
        .iter()
        .enumerate()
        .map(|(i, k)| {
            format!(
                r#"{{"work_order_id":"{sid}","occurred_at":{ts},"event_id":"{sid}:{k}:{i}","line_id":"line-A","station_id":"af-order-cli","kind":"{k}","quantity":1}}"#,
                sid = stable,
                ts = i + 1,
                k = k,
                i = i,
            )
        })
        .collect();
    std::fs::write(dir.join("mes-events.jsonl"), mes.join("\n") + "\n").unwrap();

    // 节点产物：harness.db / normal.jsonl / critical.jsonl / effects.db
    let mut sum = (0u64, 0u64, 0u64);
    for (name, rid, (t, ti, to, tc), effect) in &f.nodes {
        let ndir = run_dir.join("nodes").join(name);
        let model_line = format!(
            concat!(
                r#"{{"seq":0,"level":"normal","kind":"harness.model","payload":{{"run_id":"{rid}","#,
                r#""turn":{t},"model":"m","finish":"tool_use","usage":{{"input_tokens":{ti},"output_tokens":{to},"cache_read_tokens":{tc}}}}},"ts_ms":1}}"#
            ),
            rid = rid,
            t = t,
            ti = ti,
            to = to,
            tc = tc,
        );
        let tool_line = format!(
            concat!(
                r#"{{"seq":1,"level":"normal","kind":"harness.tool","payload":{{"run_id":"{rid}","#,
                r#""turn":{t},"tool":"bash","effect":"{effect}","is_error":false,"error_code":""}},"ts_ms":2}}"#
            ),
            rid = rid,
            t = t,
            effect = effect,
        );
        std::fs::write(ndir.join("normal.jsonl"), format!("{model_line}\n{tool_line}\n"))
            .unwrap();
        std::fs::write(
            ndir.join("critical.jsonl"),
            format!(
                r#"{{"seq":0,"level":"critical","kind":"harness.stop","payload":{{"run_id":"{rid}","turn":1,"status":{{"kind":"completed","signal":"TASK_COMPLETE"}}}},"ts_ms":9}}"#,
                rid = rid
            ),
        )
        .unwrap();
        let hdb = rusqlite::Connection::open(ndir.join("harness.db")).unwrap();
        hdb.execute(
            "CREATE TABLE run_row (run_id TEXT, terminal_json TEXT, order_id TEXT)",
            [],
        )
        .unwrap();
        hdb.execute(
            "INSERT INTO run_row VALUES (?1, ?2, ?3)",
            rusqlite::params![
                rid,
                r#"{"kind":"completed","signal":"TASK_COMPLETE"}"#,
                format!("{}-run", f.order_id)
            ],
        )
        .unwrap();
        let edb = rusqlite::Connection::open(ndir.join("effects.db")).unwrap();
        edb.execute(
            "CREATE TABLE effect_intent (workorder_id TEXT, seq INTEGER, payload_json TEXT, \
             payload_sha256 TEXT, state TEXT, order_id TEXT)",
            [],
        )
        .unwrap();
        edb.execute(
            "INSERT INTO effect_intent VALUES (?1, ?2, '{}', 'sha', 'executed', ?3)",
            rusqlite::params![format!("wo-{name}"), 0, rid],
        )
        .unwrap();
        sum.0 += ti;
        sum.1 += to;
        sum.2 += tc;
    }

    // records.jsonl：plan_id 注册表形态 + 全节点 quota 和
    let (qi, qo, qc) = f.quota.unwrap_or(sum);
    let rec = serde_json::json!({
        "plan_id": f.plan_id,
        "node_id": "line-A",
        "item_id": "item",
        "attempt": 1,
        "host_alias": "h",
        "unit_id": "graph-unit",
        "unit_version": "0.1.0",
        "status": "completed",
        "verdict": true,
        "evidence": {
            "run_dir": format!("af-state/orders/{}/runs/graph-item-glm-cp-a1", f.order_id),
            "evidence_digest": null,
        },
        "gate": "ok",
        "usage": {"turns": 2, "unit_calls": 2, "http_retries": 0,
                  "quota": {"input": qi, "output": qo, "cache_read": qc}},
    });
    std::fs::write(dir.join("records.jsonl"), format!("{rec}\n")).unwrap();
    dir
}

/// graph 布局全绿：每节点一个 invoke_agent（nodes/planner、nodes/coder），
/// L2 终态交叉 + L3/L4 单元断言逐节点过，token 全节点闭合，plan_id 注册表
/// 形态放行——图单四层 trace 可工具核验（W2-C32 收口）。
#[test]
fn graph_layout_full_tree_ok() {
    let dir = write_graph_order(&GraphFixture::default());
    let r = af_execline::tracetree::verify_order_dir(&dir);
    assert!(
        r.findings.is_empty(),
        "graph 布局全绿单不应有 findings：{:#?}",
        r.findings
    );
    assert!(r.agent.is_none(), "graph 布局无扁平 invoke_agent");
    assert!(r.ring.is_empty());
    assert_eq!(r.graph.len(), 2, "每节点一个 invoke_agent");
    // 成员序 = 目录名字典序（与 ring 同口径）：coder < planner。
    assert_eq!(r.graph[0].run_dir_name, "nodes/coder");
    assert_eq!(r.graph[0].node_id, "coder");
    assert_eq!(r.graph[0].run_id, "graph-coder");
    assert_eq!(r.graph[0].terminal_kind, "completed");
    assert_eq!(r.graph[0].chat_events, 1);
    assert_eq!(r.graph[0].tool_events, 1);
    assert_eq!(r.graph[0].effect_intents, 1, "非纯工具恰一条 executed intent");
    assert_eq!(r.graph[0].tokens, (200, 40, 80));
    assert_eq!(r.graph[1].run_dir_name, "nodes/planner");
    assert_eq!(r.graph[1].node_id, "planner");
    assert_eq!(r.graph[1].run_id, "graph-planner");
    assert_eq!(r.graph[1].tokens, (100, 20, 40));
}

/// graph 布局 token 闭合：records.quota ≠ 全节点和 → L3 记账断链 finding
/// （口径与 report 环同：quota 是全节点之和）。
#[test]
fn graph_layout_token_sum_must_close_across_nodes() {
    let dir = write_graph_order(&GraphFixture {
        order_id: "TR-graph-tok".into(),
        quota: Some((1, 2, 3)),
        ..GraphFixture::default()
    });
    let r = af_execline::tracetree::verify_order_dir(&dir);
    assert!(
        r.findings
            .iter()
            .any(|f| f.contains("L3 chat") && f.contains("token 三和")),
        "跨节点 token 闭合断链应点名：{:#?}",
        r.findings
    );
}

/// graph 布局甄别优先级：run 根无 harness.db 且 nodes/ 在场时不再按扁平
/// 路径误报「打开 harness.db 失败」（W2-C32 探针的原始红形态）。
#[test]
fn graph_layout_not_misread_as_flat_run_root() {
    let dir = write_graph_order(&GraphFixture {
        order_id: "TR-graph-flat".into(),
        ..GraphFixture::default()
    });
    let r = af_execline::tracetree::verify_order_dir(&dir);
    assert!(
        !r.findings
            .iter()
            .any(|f| f.contains("打开 harness.db 失败")),
        "不得按 run 根扁平路径误报：{:#?}",
        r.findings
    );
}

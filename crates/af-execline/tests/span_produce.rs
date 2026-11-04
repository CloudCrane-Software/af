//! RT-E138 语义层 span 生产者：投影与状态面落盘。
//!
//! 夹具形状对照 `trace_tree_verify` 全绿单（ledger 真 order_id + 四层树）。
//! 生产者不判绿；本文件只核「订单级键行 > 0」与「无真源不造行」。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use af_execline::span_produce::{
    count_order_keyed_rows, load_state, order_id_from_ledger, project_order, publish_from_env,
    publish_order, ENV_STATE_PATH,
};
use af_execline::{DriverConfig, EngineClient, FixedClock, OrderLifecycleDriver, StubExecuteUnit};
use af_workorder::{BudgetVoucher, LineBinding, Priority, Sla};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn parent() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "af-span-produce-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_ledger(dir: &Path, order_id: &str, ops: &[(&str, bool, &str)]) {
    fs::create_dir_all(dir).unwrap();
    let sid = format!("sid-{order_id}");
    let mut lines = Vec::new();
    for (i, (op, ok, state)) in ops.iter().enumerate() {
        lines.push(format!(
            concat!(
                r#"{{"seq":{seq},"ts_ms":1,"order_id":"{oid}","stable_id":"{sid}","op":"{op}","#,
                r#""idem_key":"{sid}/{op}","ok":{ok},"state_after":"{state}","order_after":{{}}}}"#
            ),
            seq = i + 1,
            oid = order_id,
            sid = sid,
            op = op,
            ok = ok,
            state = state,
        ));
    }
    fs::write(dir.join("ledger.jsonl"), lines.join("\n") + "\n").unwrap();
}

fn default_ops() -> Vec<(&'static str, bool, &'static str)> {
    vec![
        ("create_order", true, "Created"),
        ("bind_line", true, "Created"),
        ("attach_voucher", true, "Created"),
        ("release", true, "Released"),
        ("start", true, "InProgress"),
        ("attach_artifact", true, "InProgress"),
        ("complete", true, "Completed"),
        ("close", true, "Closed"),
    ]
}

fn write_full_tree(order_id: &str) -> PathBuf {
    let dir = parent().join(order_id);
    let run_name = format!("{order_id}-a1");
    let run_id = format!("{order_id}-run");
    let run_dir = dir.join("runs").join(&run_name);
    fs::create_dir_all(&run_dir).unwrap();
    write_ledger(&dir, order_id, &default_ops());
    let sid = format!("sid-{order_id}");
    let mes: Vec<String> = ["release", "start", "complete", "close"]
        .iter()
        .enumerate()
        .map(|(i, k)| {
            format!(
                concat!(
                    r#"{{"work_order_id":"{sid}","occurred_at":{ts},"event_id":"{sid}:{k}:{i}","#,
                    r#""line_id":"line-A","station_id":"af-order-cli","kind":"{k}","quantity":1}}"#
                ),
                sid = sid,
                ts = i + 1,
                k = k,
                i = i,
            )
        })
        .collect();
    fs::write(dir.join("mes-events.jsonl"), mes.join("\n") + "\n").unwrap();
    let rec = serde_json::json!({
        "plan_id": order_id,
        "node_id": "line-A",
        "item_id": "i",
        "attempt": 1,
        "host_alias": "h",
        "unit_id": "u",
        "unit_version": "0.1.0",
        "status": "completed",
        "verdict": true,
        "evidence": {
            "run_dir": format!("af-state/orders/{order_id}/runs/{run_name}"),
            "evidence_digest": "d",
        },
        "gate": "ok",
        "usage": {
            "turns": 1,
            "unit_calls": 1,
            "http_retries": 0,
            "quota": {"input": 100, "output": 20, "cache_read": 40},
        },
    });
    fs::write(dir.join("records.jsonl"), format!("{rec}\n")).unwrap();
    let normal = [
        format!(
            concat!(
                r#"{{"seq":0,"level":"normal","kind":"harness.model","payload":{{"run_id":"{rid}","#,
                r#""turn":0,"model":"m","finish":"tool_use","usage":{{"input_tokens":60,"output_tokens":12,"cache_read_tokens":24}}}},"ts_ms":1}}"#
            ),
            rid = run_id
        ),
        format!(
            concat!(
                r#"{{"seq":1,"level":"normal","kind":"harness.model","payload":{{"run_id":"{rid}","#,
                r#""turn":1,"model":"m","finish":"tool_use","usage":{{"input_tokens":40,"output_tokens":8,"cache_read_tokens":16}}}},"ts_ms":2}}"#
            ),
            rid = run_id
        ),
        format!(
            concat!(
                r#"{{"seq":2,"level":"normal","kind":"harness.tool","payload":{{"run_id":"{rid}","#,
                r#""turn":0,"tool":"bash","effect":"external_once","is_error":false,"error_code":""}},"ts_ms":3}}"#
            ),
            rid = run_id
        ),
        format!(
            concat!(
                r#"{{"seq":3,"level":"normal","kind":"harness.tool","payload":{{"run_id":"{rid}","#,
                r#""turn":1,"tool":"bash","effect":"external_once","is_error":false,"error_code":""}},"ts_ms":4}}"#
            ),
            rid = run_id
        ),
    ];
    fs::write(run_dir.join("normal.jsonl"), normal.join("\n") + "\n").unwrap();
    let crit = format!(
        concat!(
            r#"{{"seq":0,"level":"critical","kind":"harness.stop","payload":{{"run_id":"{rid}","#,
            r#""turn":9,"status":{{"kind":"completed","signal":"TASK_COMPLETE"}}}},"ts_ms":99}}"#
        ),
        rid = run_id
    );
    fs::write(run_dir.join("critical.jsonl"), crit + "\n").unwrap();
    let hdb = rusqlite::Connection::open(run_dir.join("harness.db")).unwrap();
    hdb.execute(
        "CREATE TABLE run_row (run_id TEXT, terminal_json TEXT, order_id TEXT)",
        [],
    )
    .unwrap();
    hdb.execute(
        "INSERT INTO run_row VALUES (?1, ?2, ?3)",
        rusqlite::params![
            run_id,
            r#"{"kind":"completed","signal":"TASK_COMPLETE"}"#,
            &run_id
        ],
    )
    .unwrap();
    let edb = rusqlite::Connection::open(run_dir.join("effects.db")).unwrap();
    edb.execute(
        "CREATE TABLE effect_intent (workorder_id TEXT, seq INTEGER, payload_json TEXT, \
         payload_sha256 TEXT, state TEXT, order_id TEXT)",
        [],
    )
    .unwrap();
    for i in 0..2 {
        edb.execute(
            "INSERT INTO effect_intent VALUES (?1, ?2, '{}', 'sha', 'executed', ?3)",
            rusqlite::params![format!("wo-{i}"), i as i64, &run_id],
        )
        .unwrap();
    }
    dir
}

#[test]
fn empty_dir_emits_zero_rows() {
    let dir = parent().join("empty");
    fs::create_dir_all(&dir).unwrap();
    assert!(project_order(&dir).is_empty());
    assert_eq!(order_id_from_ledger(&dir), None);
}

#[test]
fn ledger_without_order_id_emits_zero_rows() {
    let dir = parent().join("no-key");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("ledger.jsonl"),
        r#"{"seq":1,"ts_ms":1,"order_id":"","stable_id":"x","op":"create_order","idem_key":"x/create_order","ok":true,"state_after":"Created"}"#
            .to_string()
            + "\n",
    )
    .unwrap();
    assert!(project_order(&dir).is_empty());
}

#[test]
fn ledger_only_emits_workflow_span_with_true_order_id() {
    let dir = parent().join("ledger-only");
    write_ledger(&dir, "wo-ledger-1", &default_ops());
    fs::write(dir.join("mes-events.jsonl"), "").unwrap();
    let spans = project_order(&dir);
    assert!(!spans.is_empty(), "真 ledger 必须产出 span");
    assert!(spans.iter().all(|s| s.order_id == "wo-ledger-1"));
    assert_eq!(spans[0].span_name, "invoke_workflow");
    assert_eq!(order_id_from_ledger(&dir).as_deref(), Some("wo-ledger-1"));
}

#[test]
fn full_tree_emits_four_layers_all_keyed() {
    let dir = write_full_tree("T1-order");
    let spans = project_order(&dir);
    let names: Vec<&str> = spans.iter().map(|s| s.span_name.as_str()).collect();
    assert_eq!(
        names,
        ["invoke_workflow", "invoke_agent", "chat", "execute_tool"]
    );
    assert!(spans.iter().all(|s| s.order_id == "T1-order"));
    let chat = spans.iter().find(|s| s.span_name == "chat").unwrap();
    assert_eq!(chat.attrs["count"], 2);
    let tool = spans
        .iter()
        .find(|s| s.span_name == "execute_tool")
        .unwrap();
    assert_eq!(tool.attrs["count"], 2);
    let _guard = ENV_LOCK.lock().unwrap();
    if let Ok(path) = std::env::var(ENV_STATE_PATH) {
        if !path.is_empty() {
            let n = publish_order(&dir, Path::new(&path)).unwrap();
            eprintln!("full_tree published keyed_rows={n} order_id=T1-order state={path}");
        }
    }
}

#[test]
fn publish_upserts_by_order_id_and_counts_keyed_rows() {
    let a = write_full_tree("ord-A");
    let b = write_full_tree("ord-B");
    let state = parent().join("otel-production.json");
    let n1 = publish_order(&a, &state).unwrap();
    assert!(n1 > 0);
    let n2 = publish_order(&b, &state).unwrap();
    assert!(n2 > n1, "第二单应累加键行");
    let n3 = publish_order(&a, &state).unwrap();
    assert_eq!(n3, n2, "同单重发覆盖不加倍");
    let loaded = load_state(&state).unwrap();
    assert_eq!(count_order_keyed_rows(&loaded), n3);
    let ids: std::collections::BTreeSet<&str> =
        loaded.spans.iter().map(|s| s.order_id.as_str()).collect();
    assert_eq!(
        ids,
        ["ord-A", "ord-B"]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace 根")
}

fn target_dir() -> PathBuf {
    match std::env::var("CARGO_TARGET_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => workspace_root().join("target"),
    }
}

#[test]
fn drive_publishes_true_order_keyed_spans_when_env_set() {
    let _guard = ENV_LOCK.lock().unwrap();
    let scratch = parent();
    let out = scratch.join("order-out");
    // 端到端脚本可预先 export AF_OTEL_PRODUCTION_STATE 指向状态面；
    // 未设置时落到本测试 scratch，避免污染仓库。
    let previous = std::env::var(ENV_STATE_PATH).ok().filter(|p| !p.is_empty());
    let state = match &previous {
        Some(p) => PathBuf::from(p),
        None => scratch.join("otel-production.json"),
    };
    fs::create_dir_all(&out).unwrap();
    if let Some(parent) = state.parent() {
        let _ = fs::create_dir_all(parent);
    }
    std::env::set_var(ENV_STATE_PATH, &state);
    let order_id = format!("wo-span-{}", std::process::id());
    let cfg = DriverConfig {
        task_sha256: None,
        plan_sha256: None,
        order_id: order_id.clone(),
        graph_version: "g-poc-1".to_string(),
        sla: Sla::new(None, Priority::Normal),
        line: LineBinding {
            line_id: "line-poc".into(),
            line_version: "lv-1".into(),
        },
        voucher: BudgetVoucher {
            voucher_id: format!("bv-{order_id}"),
            amount: 500,
            signed: true,
        },
        out_dir: out.clone(),
        station_id: "st-poc".to_string(),
        host_alias: "anolis".into(),
        data_root: PathBuf::new(),
        task_id: String::new(),
        trial: false,
        profile: "stub".into(),
        attempt_layout: af_execline::AttemptLayout::SwevIncrement,
    };
    let client = EngineClient::locate(&workspace_root(), &target_dir())
        .expect("af-engine 二进制定位/构建失败");
    let mut driver =
        OrderLifecycleDriver::open(cfg, client, Box::new(FixedClock::new(1_767_225_600_000)))
            .expect("打开驱动");
    driver
        .drive(&StubExecuteUnit::qualified_final(format!("fa-{order_id}")))
        .expect("全生命周期应走通");
    if let Some(p) = previous {
        std::env::set_var(ENV_STATE_PATH, p);
    } else {
        std::env::remove_var(ENV_STATE_PATH);
    }
    let loaded = load_state(&state).expect("状态面应由 drive 挂钩写出");
    let n = count_order_keyed_rows(&loaded);
    assert!(n > 0, "真库订单级键行必须 > 0，实得 {n}");
    assert!(
        loaded.spans.iter().any(|s| s.order_id == order_id),
        "span 必须带本单 order_id={order_id}，实得 {:?}",
        loaded.spans.iter().map(|s| &s.order_id).collect::<Vec<_>>()
    );
    eprintln!(
        "drive_publishes: order_id={order_id} keyed_rows={n} state={}",
        state.display()
    );
}

#[test]
fn publish_from_env_skips_when_unset() {
    let _guard = ENV_LOCK.lock().unwrap();
    let previous = std::env::var(ENV_STATE_PATH).ok();
    std::env::remove_var(ENV_STATE_PATH);
    let dir = write_full_tree("skip-env");
    publish_from_env(&dir);
    if let Some(p) = previous {
        std::env::set_var(ENV_STATE_PATH, p);
    }
}

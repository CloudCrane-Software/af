//! RT-E138 语义层 span 生产者：投影与状态面落盘。
//!
//! 夹具形状对照 `trace_tree_verify` 全绿单（ledger 真 order_id + 四层树）。
//! 生产者不判绿；本文件只核「订单级键行 > 0」与「无真源不造行」。
//!
//! P3-A2（单根 trace）新增一节：投影树**恰一个根 span** 且父链可解析
//! （断言器 S1/S2 的生产者面），以及 OTLP 发射挂钩的**幂等闸**
//! （同订单同内容不重发——重发会在同一 trace 里再插一份全树、根翻倍）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use af_execline::span_produce::{
    count_fully_keyed_rows, count_key_drift_warns, count_order_keyed_rows, load_state,
    order_id_from_ledger, probe_key_drift, project_order, publish_from_env, publish_order,
    root_span_count, unresolved_parent_names, ENV_EMIT_MARKER, ENV_STATE_PATH,
};
use af_execline::tracetree::verify_order_dir;
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

/// A1：语义层名册正本（`otel/semantic-roster.json`）的契约钉桩。
/// 生产代码从正本取名（单源）；本测试把「正本必须是什么」钉死——
/// 改正本（改名/删层/动订单域键）⇒ 本测试红（三面夹具的 crate 面）。
#[test]
fn roster_contract_pins_layers_and_order_domain_keys() {
    let Some(r) = af_execline::span_produce::roster() else {
        panic!("名册正本必须可解析（include_str otel/semantic-roster.json）");
    };
    assert_eq!(r.schema, "af.otel.semantic-roster.v1");
    assert_eq!(r.order_domain_keys, ["af.order_id", "af.work_order_id"]);
    let pinned = [
        ("L1", "invoke_workflow", None),
        ("L2", "invoke_agent", Some("invoke_workflow")),
        ("L3", "chat", Some("invoke_agent")),
        ("L4", "execute_tool", Some("invoke_agent")),
    ];
    for (layer, want_name, want_parent) in pinned {
        assert_eq!(
            r.layer_name(layer),
            Some(want_name),
            "层 {layer} 的 span 名必须钉为 {want_name}（正本被篡改？）"
        );
        let entry = &r.layers[want_name];
        assert_eq!(entry.layer, layer);
        assert_eq!(entry.parent.as_deref(), want_parent, "{want_name} 父链");
    }
    assert_eq!(
        r.layers["invoke_workflow"].span_order_keys,
        ["af.order_id", "af.work_order_id", "af.line"]
    );
    // 投影行只出正本名（含被钉名与投影输出的一致）。
    let dir = write_full_tree("roster-pin");
    let spans = project_order(&dir);
    assert!(spans
        .iter()
        .all(|s| r.layers.contains_key(s.span_name.as_str())));
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

// ------------------------------------------------------------------------ //
// 键推广（A3 v2）：每行携带完整订单级键集（order_id + work_order_id）。
// work_order_id = Rust 侧 tracetree BTreeSet 最小值；与投影器首行取键独立，
// 容许漂移。既有测试断言零改动；本节全部为新增。
// ------------------------------------------------------------------------ //

fn write_ledger_sids(dir: &Path, order_id: &str, sids: &[&str], ops: &[(&str, bool, &str)]) {
    fs::create_dir_all(dir).unwrap();
    let mut lines = Vec::new();
    for (i, (op, ok, state)) in ops.iter().enumerate() {
        let sid = sids[i.min(sids.len().saturating_sub(1))];
        let sid_field = if sid.is_empty() {
            String::new()
        } else {
            format!(r#""stable_id":"{sid}","#)
        };
        lines.push(format!(
            concat!(
                r#"{{"seq":{seq},"ts_ms":1,"order_id":"{oid}",{sid_field}"op":"{op}","#,
                r#""idem_key":"{oid}/{op}","ok":{ok},"state_after":"{state}","order_after":{{}}}}"#
            ),
            seq = i + 1,
            oid = order_id,
            sid_field = sid_field,
            op = op,
            ok = ok,
            state = state,
        ));
    }
    fs::write(dir.join("ledger.jsonl"), lines.join("\n") + "\n").unwrap();
}

#[test]
fn full_tree_rows_carry_full_order_key_set() {
    let dir = write_full_tree("T2-keyset");
    let spans = project_order(&dir);
    assert_eq!(spans.len(), 4, "四层树应投影 4 行");
    assert!(
        spans
            .iter()
            .all(|s| !s.order_id.is_empty() && !s.work_order_id.is_empty()),
        "每行必须同时携带 order_id 与 work_order_id"
    );
    assert!(
        spans
            .iter()
            .all(|s| s.work_order_id == format!("sid-{}", s.order_id)),
        "本夹具每行 stable_id 同值：work_order_id 等于 Rust 侧集合最小值"
    );
}

#[test]
fn publish_counts_fully_keyed_rows_and_upserts() {
    let a = write_full_tree("ord-K1");
    let b = write_full_tree("ord-K2");
    let state = parent().join("otel-production-keyset.json");
    let _ = publish_order(&a, &state).unwrap();
    let n = publish_order(&b, &state).unwrap();
    let loaded = load_state(&state).unwrap();
    assert_eq!(count_order_keyed_rows(&loaded), n);
    assert_eq!(
        count_fully_keyed_rows(&loaded),
        n,
        "真源齐备订单：双键行数必须等于键行数"
    );
    assert_eq!(count_key_drift_warns(&loaded), 0, "齐备同值账本无漂移 WARN");
}

#[test]
fn legacy_state_without_work_order_id_field_loads_with_default() {
    let state = parent().join("otel-production-legacy.json");
    let legacy = serde_json::json!({
        "schema": "af.otel.production.v1",
        "updated_at_unix": 0,
        "producer": "af-execline/span_produce",
        "spans": [{
            "order_id": "ord-legacy",
            "stable_id": "sid-ord-legacy",
            "span_name": "invoke_workflow",
            "span_id": "ord-legacy:invoke_workflow",
            "source": "ledger.jsonl+mes-events.jsonl",
            "attrs": {}
        }]
    });
    fs::write(&state, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
    let loaded = load_state(&state).expect("旧 schema 行（无 work_order_id 字段）必须可加载");
    assert_eq!(loaded.spans.len(), 1);
    assert_eq!(loaded.spans[0].order_id, "ord-legacy");
    assert_eq!(loaded.spans[0].work_order_id, "", "缺省字段按空串兼容");
    assert_eq!(count_order_keyed_rows(&loaded), 1);
    assert_eq!(
        count_fully_keyed_rows(&loaded),
        0,
        "旧行半键：不进双键计数（如实）"
    );
}

#[test]
fn missing_stable_id_emits_zero_rows_fail_closed() {
    let dir = parent().join("ord-missing-sid");
    write_ledger_sids(&dir, "ord-missing-sid", &[""], &default_ops());
    fs::write(dir.join("mes-events.jsonl"), "").unwrap();
    let report = verify_order_dir(&dir);
    assert!(
        report.stable_id.is_empty(),
        "缺 stable_id 字段时 tracetree 集合为空"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.contains("stable_id") && (f.contains("缺") || f.contains("空"))),
        "tracetree 对字段缺席不报 finding，实得 {:?}",
        report.findings
    );
    let spans = project_order(&dir);
    assert!(
        spans.is_empty(),
        "缺 stable_id fail-closed 不产出半键行，实得 {} 行",
        spans.len()
    );
}

#[test]
fn key_drift_probe_warns_when_first_row_differs_from_set_min() {
    let dir = parent().join("ord-drift");
    // 首行 sid-zzz（投影器取此），其余 sid-aaa（BTreeSet 最小=sid-aaa）。
    let mut sids: Vec<&str> = vec!["sid-zzz"];
    sids.extend(std::iter::repeat_n("sid-aaa", 7));
    write_ledger_sids(&dir, "ord-drift", &sids, &default_ops());
    fs::write(dir.join("mes-events.jsonl"), "").unwrap();
    let probe = probe_key_drift(&dir);
    assert_eq!(probe.projector_first_row, "sid-zzz");
    assert_eq!(probe.rust_set_min, "sid-aaa");
    assert!(probe.drifted, "首行≠集合最小必须 WARN");
    let spans = project_order(&dir);
    assert!(
        !spans.is_empty(),
        "漂移仍产出行（容许漂移，以 Rust 取值为准写本面）"
    );
    assert!(
        spans.iter().all(|s| s.work_order_id == "sid-aaa"),
        "本面 work_order_id 走 Rust 集合最小"
    );
    let l1 = spans
        .iter()
        .find(|s| s.span_name == "invoke_workflow")
        .unwrap();
    assert_eq!(l1.attrs["ledger_key_drift"], serde_json::json!(true));
    let state = parent().join("otel-production-drift.json");
    let _ = publish_order(&dir, &state).unwrap();
    let loaded = load_state(&state).unwrap();
    assert_eq!(count_key_drift_warns(&loaded), 1);
}

#[test]
fn key_drift_probe_silent_when_aligned() {
    let dir = write_full_tree("ord-aligned");
    let probe = probe_key_drift(&dir);
    assert!(!probe.drifted, "齐备同值账本不应 WARN");
    assert_eq!(probe.projector_first_row, probe.rust_set_min);
    let spans = project_order(&dir);
    assert!(spans
        .iter()
        .all(|s| s.attrs.get("ledger_key_drift").is_none()));
}

// ------------------------------------------------------------------------ //
// P3-A2 单根 trace：投影树结构不变量 + OTLP 发射幂等闸。
//
// 背景（2026-11-05 亲读）：真库 trace `45c71671dd8d6efc…` 的 70 行只有 7 个
// 不同 span_id，每个插了 10 遍——根 span（invoke_workflow）计 10 次，
// 断言器 S1「每 trace 恰一个根 span」红。投影器派生 ID 只由 work_order_id
// 决定 ⇒ 重发即同 trace 再生一份全树。本节把两件事钉在生产面：
//   ① 投影本身恰一根、父链可解析（S1/S2 等价，改 materialize 立即红）；
//   ② 发射挂钩对同订单同内容只呼一次脚本（幂等闸，防 CH 同 trace 多根）。
// ------------------------------------------------------------------------ //

/// 写一个假发射脚本：被呼一次就往计数器文件追加一行（python3 -I 直跑）。
fn write_fake_emit_script(path: &Path, counter: &Path) {
    let body = format!(
        "import sys\nwith open({counter:?}, 'a', encoding='utf-8') as fh:\n    fh.write('emit\\n')\n",
    );
    fs::write(path, body).unwrap();
}

fn count_lines(path: &Path) -> usize {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

#[test]
fn projected_tree_has_single_root_and_resolvable_parents() {
    // 四层树夹具：恰一个根（invoke_workflow），其余行的 parent 均可在本投影内解析。
    let dir = write_full_tree("T1-order");
    let spans = project_order(&dir);
    assert_eq!(root_span_count(&spans), 1, "一订单投影应恰一个根 span");
    let root = spans
        .iter()
        .find(|s| s.parent_span_name.as_deref().unwrap_or("").is_empty())
        .expect("必须有根行");
    assert_eq!(root.span_name, "invoke_workflow", "根必须是 L1");
    assert!(
        unresolved_parent_names(&spans).is_empty(),
        "非根行 parent 必须在本投影内可解析（防断链）"
    );
    // 无 harness 事件的 ledger-only 单：同样单根（L1 独占）。
    let dir = parent().join("root-pin-ledger-only");
    write_ledger(&dir, "wo-root-pin", &default_ops());
    fs::write(dir.join("mes-events.jsonl"), "").unwrap();
    let spans = project_order(&dir);
    assert_eq!(spans.len(), 1);
    assert_eq!(root_span_count(&spans), 1);
    assert!(unresolved_parent_names(&spans).is_empty());
}

#[test]
fn emit_hook_skips_reemit_when_projection_unchanged() {
    let _guard = ENV_LOCK.lock().unwrap();
    let scratch = parent();
    let counter = scratch.join("emit-counter.txt");
    let script = scratch.join("fake-emit.py");
    let marker = scratch.join("emit-marker.json");
    write_fake_emit_script(&script, &counter);

    let prev_cmd = std::env::var("AF_OTEL_EMIT_CMD").ok();
    let prev_marker = std::env::var(ENV_EMIT_MARKER).ok();
    let prev_state = std::env::var(ENV_STATE_PATH).ok();
    let prev_ep = std::env::var("AF_OTEL_ENDPOINT").ok();
    std::env::remove_var(ENV_STATE_PATH);
    std::env::set_var("AF_OTEL_ENDPOINT", "http://127.0.0.1:1");
    std::env::set_var("AF_OTEL_EMIT_CMD", &script);
    std::env::set_var(ENV_EMIT_MARKER, &marker);

    let dir = write_full_tree(&format!("wo-emit-idem-{}", std::process::id()));
    publish_from_env(&dir);
    publish_from_env(&dir);
    publish_from_env(&dir);
    assert_eq!(
        count_lines(&counter),
        1,
        "同订单同投影重复 drive 只能呼一次发射脚本（幂等闸；重发=同 trace 第二份根）"
    );

    // 还原环境。
    match prev_cmd {
        Some(v) => std::env::set_var("AF_OTEL_EMIT_CMD", v),
        None => std::env::remove_var("AF_OTEL_EMIT_CMD"),
    }
    match prev_marker {
        Some(v) => std::env::set_var(ENV_EMIT_MARKER, v),
        None => std::env::remove_var(ENV_EMIT_MARKER),
    }
    match prev_state {
        Some(v) => std::env::set_var(ENV_STATE_PATH, v),
        None => std::env::remove_var(ENV_STATE_PATH),
    }
    match prev_ep {
        Some(v) => std::env::set_var("AF_OTEL_ENDPOINT", v),
        None => std::env::remove_var("AF_OTEL_ENDPOINT"),
    }
}

#[test]
fn emit_hook_reemits_when_projection_changes() {
    let _guard = ENV_LOCK.lock().unwrap();
    let scratch = parent();
    let counter = scratch.join("emit-counter-change.txt");
    let script = scratch.join("fake-emit-change.py");
    let marker = scratch.join("emit-marker-change.json");
    write_fake_emit_script(&script, &counter);

    let prev_cmd = std::env::var("AF_OTEL_EMIT_CMD").ok();
    let prev_marker = std::env::var(ENV_EMIT_MARKER).ok();
    let prev_state = std::env::var(ENV_STATE_PATH).ok();
    let prev_ep = std::env::var("AF_OTEL_ENDPOINT").ok();
    std::env::remove_var(ENV_STATE_PATH);
    std::env::set_var("AF_OTEL_ENDPOINT", "http://127.0.0.1:1");
    std::env::set_var("AF_OTEL_EMIT_CMD", &script);
    std::env::set_var(ENV_EMIT_MARKER, &marker);

    let order_id = format!("wo-emit-change-{}", std::process::id());
    let dir = write_full_tree(&order_id);
    publish_from_env(&dir);
    assert_eq!(count_lines(&counter), 1, "首轮必须发射");

    // 追加一次 harness.model 事件 ⇒ chat 计数变 ⇒ 投影内容摘要变 ⇒ 重新发射。
    let normal = dir
        .join("runs")
        .join(format!("{order_id}-a1"))
        .join("normal.jsonl");
    let rid = format!("{order_id}-run");
    let extra = format!(
        concat!(
            r#"{{"seq":4,"level":"normal","kind":"harness.model","payload":{{"run_id":"{rid}","#,
            r#""turn":2,"model":"m","finish":"stop","usage":{{"input_tokens":10,"output_tokens":2,"cache_read_tokens":1}}}},"ts_ms":5}}"#
        ),
        rid = rid
    );
    let mut text = fs::read_to_string(&normal).unwrap();
    text.push_str(&extra);
    text.push('\n');
    fs::write(&normal, text).unwrap();

    publish_from_env(&dir);
    assert_eq!(
        count_lines(&counter),
        2,
        "投影内容变化后必须重新发射（闸不挡真增量）"
    );

    // 还原环境。
    match prev_cmd {
        Some(v) => std::env::set_var("AF_OTEL_EMIT_CMD", v),
        None => std::env::remove_var("AF_OTEL_EMIT_CMD"),
    }
    match prev_marker {
        Some(v) => std::env::set_var(ENV_EMIT_MARKER, v),
        None => std::env::remove_var(ENV_EMIT_MARKER),
    }
    match prev_state {
        Some(v) => std::env::set_var(ENV_STATE_PATH, v),
        None => std::env::remove_var(ENV_STATE_PATH),
    }
    match prev_ep {
        Some(v) => std::env::set_var("AF_OTEL_ENDPOINT", v),
        None => std::env::remove_var("AF_OTEL_ENDPOINT"),
    }
}

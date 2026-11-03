//! release-gate —— 投放闸的可执行面（PoC-B：投放闸接线）。
//!
//! 复用 crate 内既有 [`DurableGate`]（gate_meta/wip_item/canary 三表 + BEGIN IMMEDIATE
//! + f64 位型核对），不新增第 27 件机制。子命令：
//!
//! - `serve`    常驻：首启声明闸门（--upper/--lower/--night），机制5 自举
//!   （接线即 begin_canary），周期对账孤儿在制单（进程已死未 settle 的
//!   读 results.jsonl 质量位补 settle），刷事件到 `<db>.events.jsonl`。
//! - `admit`    提交前问闸。记 wip_item（order_id + 单调 item_id + admitted 分钟 +
//!   权重）。`--shadow` 只记不裁：AtCapacity 也照实入账（shadow_record 路径），
//!   输出里带 shadow_forced 标注，供影子对照。
//! - `complete` 进程退出后释容。接 results.jsonl 质量位（reward=false → defects=1）。
//!   幂等 settle：重复/未知单不重复释容（exit 0 + settled:false）。
//!   `--cancelled` 走取消账（秒败单），不喂渐放谓词。
//! - `status`   闸读数（refill_needed / wip_minutes / 守恒对账 / 渐放状态）。
//! - `events`   尾读 `<db>.events.jsonl`。
//!
//! 权重首版全 1（计件模式 = CONWIP 退化特例，upper=件数上限）；真实 review_minutes
//! 分布未得，不启用权重。CONWIP 单位 = WorkOrder：admit 起占量，Closed/Cancelled
//! 才释量；工单状态机仍归 af-order，本面只做在制记账与水位判定。
//!
//! 退出码：0 成功（含幂等 no-op）；2 用法错；3 AtCapacity（本轮跳过）；
//! 4 重复单；5 账本/闸门错误；6 账本未初始化（先 serve）；7 渐放未晋升（探针封顶）。

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use af_release_gate::{
    AdmitError, CanaryPredicate, Conservation, DurableGate, EvalReport, GateConfig, GateEvent,
    Item, MinuteOfDay, NightWindow, OpenOutcome, PersistError,
};

// ---------- 通用 ----------

fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// 闸内分钟 = UTC 当天第几分钟（账本一致性只要求单调有序，不要求本地时区）。
fn minute_of_day() -> MinuteOfDay {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ((secs % 86_400) / 60) as u32
}

fn events_path(db: &Path) -> PathBuf {
    let mut s = db.as_os_str().to_os_string();
    s.push(".events.jsonl");
    PathBuf::from(s)
}

/// 追加一行 JSON 事件到 `<db>.events.jsonl`。单次 write <4KB，O_APPEND 原子。
fn append_event(db: &Path, mut record: serde_json::Value) {
    if let Some(obj) = record.as_object_mut() {
        obj.insert("ts".into(), serde_json::json!(wall_ms()));
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(events_path(db));
    if let Ok(file) = file.as_mut() {
        let _ = writeln!(file, "{record}");
    }
}

fn out_json(value: &serde_json::Value) {
    println!("{value}");
}

fn die(code: i32, message: &str) -> ! {
    eprintln!("release-gate: {message}");
    exit(code);
}

/// 按库内声明重开（声明漂移直接失败）。库未初始化 → exit 6。
/// 库里有渐放行时按行内 config 字符串（`key@min_completed/max_defects`）重建谓词
/// 重入——谓词是函数不落库，参数编进 config 串是重开时的唯一真相。
fn open_declared(db: &Path) -> (DurableGate, GateConfig) {
    let Some(declaration) = DurableGate::declaration_on_disk(db).unwrap_or_else(|err| {
        die(5, &format!("读账本声明失败: {err}"));
    }) else {
        die(6, &format!("账本 {db:?} 未初始化：先跑 serve 声明"));
    };
    let resume = canary_config_on_disk(db).map(|config| {
        let (_, min_completed, max_defects) = parse_canary_config(&config);
        canary_predicate(min_completed, max_defects)
    });
    match DurableGate::open(db, &declaration, resume) {
        Ok((gate, OpenOutcome::AlreadyDeclared)) => (gate, declaration),
        Ok(_) => die(5, "已存在的账本重开却报 Created"),
        Err(err) => die(5, &format!("开账本失败: {err}")),
    }
}

/// 库里渐放行的 config（无行/无库 → None）。
fn canary_config_on_disk(db: &Path) -> Option<String> {
    let conn = rusqlite::Connection::open(db).ok()?;
    conn.query_row("SELECT config FROM canary WHERE id = 1", [], |row| {
        row.get(0)
    })
    .ok()
}

fn conservation_json(c: &Conservation) -> serde_json::Value {
    serde_json::json!({
        "admitted_count": c.admitted_count,
        "admitted_weight": c.admitted_weight,
        "completed_count": c.completed_count,
        "completed_weight": c.completed_weight,
        "cancelled_count": c.cancelled_count,
        "cancelled_weight": c.cancelled_weight,
        "wip_count": c.wip_count,
        "wip_minutes": c.wip_minutes,
        "count_balanced": c.count_balanced(),
        "weight_balanced": c.weight_balanced(),
    })
}

fn canary_json(gate: &DurableGate) -> serde_json::Value {
    match (gate.canary_status(), gate.canary_cursor()) {
        (Some(status), Some(cursor)) => serde_json::json!({
            "config": status.config,
            "open": status.open,
            "released": status.released,
            "max_probes": status.max_probes,
            "completed": cursor.completed,
            "defects": cursor.defects,
        }),
        _ => serde_json::Value::Null,
    }
}

// ---------- admit ----------

fn cmd_admit(db: &Path, args: &[String]) {
    let mut order_id = String::new();
    let mut weight: f64 = 1.0; // 计件模式：权重首版全 1。
    let mut config = std::env::var("RELEASE_GATE_CONFIG").unwrap_or_else(|_| "v1".to_string());
    let mut shadow = false;
    let mut note = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--order-id" => order_id = next_value(args, &mut i, "--order-id"),
            "--weight" => {
                let raw = next_value(args, &mut i, "--weight");
                weight = raw.parse().unwrap_or_else(|_| die(2, "--weight 不是数字"));
            }
            "--config" => config = next_value(args, &mut i, "--config"),
            "--shadow" => shadow = true,
            "--note" => note = next_value(args, &mut i, "--note"),
            other => die(2, &format!("admit 未知参数 {other:?}")),
        }
        i += 1;
    }
    if order_id.is_empty() {
        die(2, "admit 必须带 --order-id（WIP 单位=WorkOrder）");
    }
    if !(weight.is_finite() && weight > 0.0) {
        die(2, &format!("--weight 非法: {weight}"));
    }
    let (mut gate, _) = open_declared(db);
    let now = minute_of_day();
    let item = Item {
        id: gate.next_item_id(),
        order_id: order_id.clone(),
        config: config.clone(),
        review_minutes: weight,
    };

    // 决策先走正式路径（全策略）；影子模式遇 AtCapacity/RampNotOpen 再照实补记。
    let decision = gate.try_admit(&item, now);
    let record = match &decision {
        Ok(ticket) => serde_json::json!({
            "decision": "admit",
            "item_id": ticket.item_id,
            "order_id": ticket.order_id,
            "weight": ticket.weight,
            "admitted_at": ticket.admitted_at,
            "wip_minutes": gate.wip_minutes(),
            "wip_upper": gate.config().wip_upper_minutes,
            "shadow": shadow,
            "note": note,
        }),
        Err(PersistError::Admit(AdmitError::WipCap {
            wip,
            weight: got,
            cap,
        })) => {
            let forced = if shadow {
                Some(shadow_force_record(&mut gate, &item, now))
            } else {
                None
            };
            let mut record = serde_json::json!({
                "decision": "at_capacity",
                "order_id": order_id,
                "wip_minutes": wip,
                "weight": got,
                "wip_upper": cap,
                "shadow": shadow,
                "note": note,
            });
            if let Some(ticket) = forced {
                record["shadow_forced"] = serde_json::json!(true);
                record["item_id"] = serde_json::json!(ticket.item_id);
                record["wip_minutes_after_record"] = serde_json::json!(gate.wip_minutes());
            }
            record
        }
        Err(PersistError::Admit(AdmitError::CanaryProbeCap { config, cap })) => {
            let forced = if shadow {
                Some(shadow_force_record(&mut gate, &item, now))
            } else {
                None
            };
            let mut record = serde_json::json!({
                "decision": "ramp_not_open",
                "order_id": order_id,
                "canary_config": config,
                "probe_cap": cap,
                "shadow": shadow,
                "note": note,
            });
            if let Some(ticket) = forced {
                record["shadow_forced"] = serde_json::json!(true);
                record["item_id"] = serde_json::json!(ticket.item_id);
            }
            record
        }
        Err(PersistError::Admit(AdmitError::DuplicateOrder {
            order_id: dup,
            item_id,
        })) => serde_json::json!({
            "decision": "duplicate_order",
            "order_id": dup,
            "held_by_item": item_id,
            "shadow": shadow,
            "note": note,
        }),
        Err(PersistError::Admit(AdmitError::NightThrottled { ratio })) => {
            // 本部署不配置夜间窗口；影子模式下出现说明部署走样，fail-loud。
            die(
                5,
                &format!("夜间窗口限流 (ratio={ratio})：影子记账不支持夜间窗口，请检查部署"),
            )
        }
        Err(other) => die(5, &format!("admit 失败: {other}")),
    };
    append_event(
        db,
        serde_json::json!({
            "kind": "admit",
            "db_wip": gate.wip_minutes(),
            "result": record,
        }),
    );
    let code = match &decision {
        Ok(_) => 0,
        Err(PersistError::Admit(AdmitError::WipCap { .. }))
        | Err(PersistError::Admit(AdmitError::CanaryProbeCap { .. }))
            if shadow =>
        {
            0
        }
        Err(PersistError::Admit(AdmitError::WipCap { .. })) => 3,
        Err(PersistError::Admit(AdmitError::CanaryProbeCap { .. })) => 7,
        Err(PersistError::Admit(AdmitError::DuplicateOrder { .. })) => 4,
        Err(_) => unreachable!("其他错误已在上面 die"),
    };
    out_json(&record);
    if code != 0 {
        exit(code);
    }
}

/// 影子补记：现实里的单已经提交，闸只负责把事实入账（跳 WIP 上限与探针封顶）。
fn shadow_force_record(
    gate: &mut DurableGate,
    item: &Item,
    now: MinuteOfDay,
) -> af_release_gate::Ticket {
    match gate.try_admit_shadow_record(item, now) {
        Ok(ticket) => ticket,
        Err(PersistError::Admit(AdmitError::DuplicateOrder { order_id, item_id })) => die(
            4,
            &format!("影子补记撞重复单 {order_id}（在制 item {item_id}）"),
        ),
        Err(other) => die(5, &format!("影子补记失败: {other}")),
    }
}

fn next_value(args: &[String], i: &mut usize, flag: &str) -> String {
    *i += 1;
    args.get(*i)
        .cloned()
        .unwrap_or_else(|| die(2, &format!("{flag} 缺值")))
}

// ---------- complete / cancel ----------

fn cmd_complete(db: &Path, args: &[String]) {
    let mut order_id = String::new();
    let mut item_id: Option<u64> = None;
    let mut defects: u64 = 0;
    let mut cancelled = false;
    let mut note = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--order-id" => order_id = next_value(args, &mut i, "--order-id"),
            "--item-id" => {
                let raw = next_value(args, &mut i, "--item-id");
                item_id = Some(raw.parse().unwrap_or_else(|_| die(2, "--item-id 不是数字")));
            }
            "--defects" => {
                let raw = next_value(args, &mut i, "--defects");
                defects = raw.parse().unwrap_or_else(|_| die(2, "--defects 不是数字"));
            }
            "--cancelled" => cancelled = true,
            "--note" => note = next_value(args, &mut i, "--note"),
            other => die(2, &format!("complete 未知参数 {other:?}")),
        }
        i += 1;
    }
    if order_id.is_empty() && item_id.is_none() {
        die(2, "complete 需要 --order-id 或 --item-id");
    }
    let (mut gate, _) = open_declared(db);
    let now = minute_of_day();

    // 按 order_id 解析 item_id（账本里在制单的对账键）。
    let resolved = if let Some(id) = item_id {
        Some((id, String::new()))
    } else {
        gate.in_process()
            .into_iter()
            .find(|item| item.order_id == order_id)
            .map(|item| (item.id, item.order_id))
    };
    let Some((id, held_order)) = resolved else {
        // 幂等：已 settle 过的单/从未准入的单都不释容、不报错（调用方可安全重试）。
        let record = serde_json::json!({
            "settled": false,
            "reason": "not_in_wip",
            "order_id": order_id,
            "item_id": item_id,
            "cancelled": cancelled,
            "defects": defects,
            "note": note,
        });
        append_event(
            db,
            serde_json::json!({"kind": "settle_noop", "result": record}),
        );
        out_json(&record);
        return;
    };

    let result = if cancelled {
        gate.cancel(id, now)
    } else {
        gate.complete(id, now, defects)
    };
    let record = match result {
        Ok(done) => serde_json::json!({
            "settled": true,
            "cancelled": cancelled,
            "item_id": done.ticket.item_id,
            "order_id": if held_order.is_empty() { done.ticket.order_id.clone() } else { held_order },
            "released_weight": done.ticket.weight,
            "defects": defects,
            "wip_minutes": gate.wip_minutes(),
            "note": note,
        }),
        Err(PersistError::Gate(err)) => die(5, &format!("settle 失败: {err}")),
        Err(other) => die(5, &format!("settle 失败: {other}")),
    };
    append_event(
        db,
        serde_json::json!({"kind": if cancelled { "cancel" } else { "complete" }, "result": record}),
    );
    out_json(&record);
}

// ---------- status ----------

fn cmd_status(db: &Path, args: &[String]) {
    let mut line = false;
    for arg in args {
        match arg.as_str() {
            "--line" => line = true,
            other => die(2, &format!("status 未知参数 {other:?}")),
        }
    }
    let (gate, _) = open_declared(db);
    let c = gate.conservation();
    if line {
        println!(
            "gate wip={:.1}/{} items={} refill_needed={} admitted={} closed={}+{} next_item_id={} canary={}",
            gate.wip_minutes(),
            gate.config().wip_upper_minutes,
            gate.in_process_count(),
            gate.refill_needed(),
            c.admitted_count,
            c.completed_count,
            c.cancelled_count,
            gate.next_item_id(),
            gate
                .canary_status()
                .map(|s| if s.open { format!("open({})", s.config) } else { format!("gated({}/{})", s.released, s.max_probes) })
                .unwrap_or_else(|| "none".to_string()),
        );
        return;
    }
    out_json(&serde_json::json!({
        "wip_minutes": gate.wip_minutes(),
        "in_process_count": gate.in_process_count(),
        "wip_upper_minutes": gate.config().wip_upper_minutes,
        "wip_lower_minutes": gate.config().wip_lower_minutes,
        "refill_needed": gate.refill_needed(),
        "next_item_id": gate.next_item_id(),
        "conservation": conservation_json(&c),
        "canary": canary_json(&gate),
        "in_process": gate.in_process().iter().map(|i| serde_json::json!({
            "item_id": i.id, "order_id": i.order_id, "config": i.config,
            "weight": i.weight, "admitted_at": i.admitted_at,
        })).collect::<Vec<_>>(),
    }));
}

// ---------- events ----------

fn cmd_events(db: &Path, args: &[String]) {
    let mut tail: usize = 40;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tail" => {
                let raw = next_value(args, &mut i, "--tail");
                tail = raw.parse().unwrap_or_else(|_| die(2, "--tail 不是数字"));
            }
            "--all" => tail = usize::MAX,
            other => die(2, &format!("events 未知参数 {other:?}")),
        }
        i += 1;
    }
    let path = events_path(db);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| die(5, &format!("读 {path:?} 失败: {err}")));
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(tail);
    for line in &lines[start..] {
        println!("{line}");
    }
}

// ---------- serve ----------

/// 渐放谓词以 config 字符串携带参数：`<key>@<min_completed>/<max_defects>`。
/// 重开账本时按同一字符串重建同一谓词（谓词本体不落库，靠参数确定性重建）。
fn parse_canary_config(config: &str) -> (String, usize, u64) {
    let (key, rule) = match config.split_once('@') {
        Some(pair) => pair,
        None => return (config.to_string(), 1, 0),
    };
    let (min_completed, max_defects) = match rule.split_once('/') {
        Some(pair) => pair,
        None => return (config.to_string(), 1, 0),
    };
    let parsed = (
        key.to_string(),
        min_completed.parse().unwrap_or(1),
        max_defects.parse().unwrap_or(0),
    );
    // 规则不完整时按最保守取值（min_completed 至少 1，defects 容忍 0）。
    if parsed.1 == 0 {
        (parsed.0, 1, parsed.2)
    } else {
        parsed
    }
}

fn canary_predicate(min_completed: usize, max_defects: u64) -> CanaryPredicate {
    Box::new(move |report: &EvalReport| {
        report.probes_completed >= min_completed && report.defects <= max_defects
    })
}

/// results.jsonl 质量位：末行 reward=false → 1 缺陷；无文件/无 reward 键 → None
/// （调用方决定按 cancelled 处理）。
fn defects_from_results(orders_dir: &Path, order_id: &str) -> Option<u64> {
    let path = orders_dir.join(order_id).join("results.jsonl");
    let content = std::fs::read_to_string(path).ok()?;
    let last = content
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())?;
    let value: serde_json::Value = serde_json::from_str(last).ok()?;
    match value.get("reward") {
        Some(serde_json::Value::Bool(true)) => Some(0),
        Some(serde_json::Value::Bool(false)) => Some(1),
        _ => None,
    }
}

/// 对账一笔在制单：其 af-order 进程是否还在。匹配 spec 文件名（burn-runner 提交
/// 时 argv 携带 `--spec spec-<前缀>.json`，order_id 前缀即单号）。
fn order_alive(order_id: &str) -> bool {
    let prefix = order_id.split('-').next().unwrap_or(order_id);
    let marker = format!("spec-{prefix}.json");
    let output = std::process::Command::new("pgrep")
        .arg("-f")
        .arg("--")
        .arg(format!("af-order.*{marker}"))
        .output();
    match output {
        Ok(out) => !out.stdout.is_empty(),
        Err(_) => true, // pgrep 不可用按"活着"处理（宁可少 settle 不误 settle）
    }
}

fn cmd_serve(db: &Path, args: &[String]) {
    let mut upper: f64 = env_u64("RELEASE_GATE_UPPER", 8) as f64;
    let mut lower: f64 = env_u64("RELEASE_GATE_LOWER", 2) as f64;
    let mut night: Option<NightWindow> = None;
    let mut interval = env_u64("RELEASE_GATE_INTERVAL", 30);
    let mut once = false;
    let mut orders_dir = PathBuf::from(
        std::env::var("RELEASE_GATE_ORDERS_DIR")
            .unwrap_or_else(|_| "/data/bench/af-p3-state/orders".to_string()),
    );
    let mut canary_key = std::env::var("RELEASE_GATE_CANARY_KEY")
        .unwrap_or_else(|_| "burn-runner-admit".to_string());
    let mut canary_min_completed = env_u64("RELEASE_GATE_CANARY_MIN_COMPLETED", 3);
    let mut canary_max_defects = env_u64("RELEASE_GATE_CANARY_MAX_DEFECTS", 0);
    let mut canary_probes = env_u64("RELEASE_GATE_CANARY_PROBES", 5);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--upper" => {
                let raw = next_value(args, &mut i, "--upper");
                upper = raw.parse().unwrap_or_else(|_| die(2, "--upper 不是数字"));
            }
            "--lower" => {
                let raw = next_value(args, &mut i, "--lower");
                lower = raw.parse().unwrap_or_else(|_| die(2, "--lower 不是数字"));
            }
            "--night" => {
                let raw = next_value(args, &mut i, "--night");
                night = Some(parse_night(&raw));
            }
            "--interval" => {
                let raw = next_value(args, &mut i, "--interval");
                interval = raw
                    .parse()
                    .unwrap_or_else(|_| die(2, "--interval 不是数字"));
            }
            "--orders-dir" => orders_dir = PathBuf::from(next_value(args, &mut i, "--orders-dir")),
            "--canary-key" => canary_key = next_value(args, &mut i, "--canary-key"),
            "--canary-probes" => {
                let raw = next_value(args, &mut i, "--canary-probes");
                canary_probes = raw
                    .parse()
                    .unwrap_or_else(|_| die(2, "--canary-probes 不是数字"));
            }
            "--canary-min-completed" => {
                let raw = next_value(args, &mut i, "--canary-min-completed");
                canary_min_completed = raw
                    .parse()
                    .unwrap_or_else(|_| die(2, "--canary-min-completed 不是数字"));
            }
            "--canary-max-defects" => {
                let raw = next_value(args, &mut i, "--canary-max-defects");
                canary_max_defects = raw
                    .parse()
                    .unwrap_or_else(|_| die(2, "--canary-max-defects 不是数字"));
            }
            "--once" => once = true,
            other => die(2, &format!("serve 未知参数 {other:?}")),
        }
        i += 1;
    }
    // 谓词参数编进 config 字符串（重开重建同一谓词的唯一真相）。
    // key 已带 @（形如 key@min/max）则原样使用——serve 与调用方共用同一串，
    // 避免二次拼接（首部署踩过：key 传 burn-runner-admit@3/0 被拼成 ...@3/0@3/0，
    // 与条目 config 不匹配，渐放变哑）。
    let canary_config = if canary_key.contains('@') {
        canary_key.clone()
    } else {
        format!("{canary_key}@{canary_min_completed}/{canary_max_defects}")
    };
    let resume = canary_predicate(canary_min_completed as usize, canary_max_defects);

    // 首启声明或按库内声明重开；库里有渐放行则必须带同一谓词参数重入。
    let declaration = DurableGate::declaration_on_disk(db)
        .unwrap_or_else(|err| die(5, &format!("读账本声明失败: {err}")));
    let (declaration, resume_predicate) = match declaration {
        Some(existing) => {
            // 已有渐放行（config 可能是旧参数编的）→ 按行内参数重建谓词。
            match DurableGate::open(db, &existing, None) {
                Ok((probe, _)) => match probe.canary_status() {
                    Some(status) => {
                        drop(probe);
                        let (_, min_completed, max_defects) = parse_canary_config(&status.config);
                        (existing, Some(canary_predicate(min_completed, max_defects)))
                    }
                    None => (existing, None),
                },
                Err(PersistError::CanaryPredicateRequired) => {
                    // 上面没带谓词重开被拒——用行内参数重建后重试。
                    let config = canary_config_on_disk(db).unwrap_or_else(|| die(5, "渐放行缺失"));
                    let (_, min_completed, max_defects) = parse_canary_config(&config);
                    (existing, Some(canary_predicate(min_completed, max_defects)))
                }
                Err(other) => die(5, &format!("开账本失败: {other}")),
            }
        }
        None => (
            GateConfig {
                wip_upper_minutes: upper,
                wip_lower_minutes: lower,
                night_window: night,
            },
            None,
        ),
    };
    let (mut gate, outcome) = DurableGate::open(db, &declaration, resume_predicate)
        .unwrap_or_else(|err| die(5, &format!("开账本失败: {err}")));
    append_event(
        db,
        serde_json::json!({"kind": "serve_open", "outcome": format!("{outcome:?}"),
                           "upper": declaration.wip_upper_minutes, "lower": declaration.wip_lower_minutes}),
    );

    // 机制5 自举：接线即 begin_canary（已有渐放行且未晋升则保留，不重置游标）。
    if gate.canary_status().is_none() {
        gate.begin_canary(&canary_config, canary_probes as usize, resume)
            .unwrap_or_else(|err| die(5, &format!("begin_canary 失败: {err}")));
        append_event(
            db,
            serde_json::json!({"kind": "canary_begin", "config": canary_config,
                               "max_probes": canary_probes,
                               "predicate": format!("completed>={canary_min_completed} && defects<={canary_max_defects}")}),
        );
    }

    loop {
        // ① 孤儿对账：在制单进程已死 → 补 settle（幂等，与调用方竞争安全）。
        for item in gate.in_process() {
            if item.order_id.is_empty() {
                continue; // 结转注入等无进程对应的单不参与对账
            }
            if order_alive(&item.order_id) {
                continue;
            }
            match defects_from_results(&orders_dir, &item.order_id) {
                Some(defects) => {
                    match gate.complete(item.id, minute_of_day(), defects) {
                        Ok(_) => append_event(
                            db,
                            serde_json::json!({
                            "kind": "reconcile_complete", "order_id": item.order_id,
                            "item_id": item.id, "defects": defects}),
                        ),
                        Err(PersistError::Gate(af_release_gate::GateError::UnknownItem(_))) => {} // 调用方先 settle 了，幂等让位
                        Err(other) => append_event(
                            db,
                            serde_json::json!({
                            "kind": "reconcile_error", "order_id": item.order_id, "error": other.to_string()}),
                        ),
                    }
                }
                None => match gate.cancel(item.id, minute_of_day()) {
                    Ok(_) => append_event(
                        db,
                        serde_json::json!({
                            "kind": "reconcile_cancel", "order_id": item.order_id, "item_id": item.id}),
                    ),
                    Err(PersistError::Gate(af_release_gate::GateError::UnknownItem(_))) => {}
                    Err(other) => append_event(
                        db,
                        serde_json::json!({
                            "kind": "reconcile_error", "order_id": item.order_id, "error": other.to_string()}),
                    ),
                },
            }
        }

        // ② 事件刷出（NeedRefill/RefillSatisfied/渐放晋升…）。
        for event in gate.drain_events() {
            append_event(
                db,
                serde_json::json!({"kind": "gate_event", "event": event_json(&event)}),
            );
        }

        // ③ 心跳一行（stdout 供 nohup 日志）。
        let c = gate.conservation();
        println!(
            "[{}] serve: wip={:.1}/{} items={} refill_needed={} admitted={} closed={}+{} canary={} conservation={}/{}",
            std::process::id(),
            gate.wip_minutes(),
            declaration.wip_upper_minutes,
            gate.in_process_count(),
            gate.refill_needed(),
            c.admitted_count,
            c.completed_count,
            c.cancelled_count,
            gate.canary_status()
                .map(|s| if s.open { "open".to_string() } else { format!("gated {}/{}", s.released, s.max_probes) })
                .unwrap_or_else(|| "none".to_string()),
            c.count_balanced(),
            c.weight_balanced(),
        );
        let _ = std::io::stdout().flush();

        if once {
            return;
        }
        std::thread::sleep(Duration::from_secs(interval.max(1)));
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(default)
}

fn parse_night(raw: &str) -> NightWindow {
    // start:end:ratio（分钟，一天内；ratio ∈ (0,1]）
    let parts: Vec<&str> = raw.split(':').collect();
    if parts.len() != 3 {
        die(2, "--night 需要 start:end:ratio");
    }
    let start: u32 = parts[0]
        .parse()
        .unwrap_or_else(|_| die(2, "--night start 非法"));
    let end: u32 = parts[1]
        .parse()
        .unwrap_or_else(|_| die(2, "--night end 非法"));
    let ratio: f64 = parts[2]
        .parse()
        .unwrap_or_else(|_| die(2, "--night ratio 非法"));
    NightWindow { start, end, ratio }
}

fn event_json(event: &GateEvent) -> serde_json::Value {
    match event {
        GateEvent::NeedRefill { at, wip, lower } => serde_json::json!({
            "type": "NeedRefill", "at": at, "wip": wip, "lower": lower}),
        GateEvent::RefillSatisfied { at, wip } => serde_json::json!({
            "type": "RefillSatisfied", "at": at, "wip": wip}),
        GateEvent::NightWindowEnter { at } => serde_json::json!({
            "type": "NightWindowEnter", "at": at}),
        GateEvent::NightWindowExit { at } => serde_json::json!({
            "type": "NightWindowExit", "at": at}),
        GateEvent::CanaryPromoted {
            config,
            probes_released,
            at,
        } => serde_json::json!({
            "type": "CanaryPromoted", "config": config, "probes_released": probes_released, "at": at}),
    }
}

// ---------- main ----------

fn usage() -> ! {
    die(
        2,
        "用法: release-gate --db PATH serve [--upper N --lower N --night s:e:r --interval S --once]\n\
         \x20   release-gate --db PATH admit --order-id ID [--weight 1.0] [--config K] [--shadow] [--note T]\n\
         \x20   release-gate --db PATH complete --order-id ID | --item-id N [--defects N] [--cancelled] [--note T]\n\
         \x20   release-gate --db PATH status [--line]\n\
         \x20   release-gate --db PATH events [--tail N | --all]",
    )
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut db: Option<PathBuf> = None;
    let mut sub: Option<&str> = None;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--db" => {
                let raw = argv
                    .get(i + 1)
                    .cloned()
                    .unwrap_or_else(|| die(2, "--db 缺值"));
                db = Some(PathBuf::from(raw));
                i += 1;
            }
            "serve" | "admit" | "complete" | "status" | "events" if sub.is_none() => {
                sub = Some(&argv[i]);
            }
            other if sub.is_some() => rest.push(other.to_string()),
            _ => usage(),
        }
        i += 1;
    }
    let db = db
        .or_else(|| std::env::var("RELEASE_GATE_DB").ok().map(PathBuf::from))
        .unwrap_or_else(|| usage());
    let Some(sub) = sub else { usage() };
    match sub {
        "serve" => cmd_serve(&db, &rest),
        "admit" => cmd_admit(&db, &rest),
        "complete" => cmd_complete(&db, &rest),
        "status" => cmd_status(&db, &rest),
        "events" => cmd_events(&db, &rest),
        _ => unreachable!(),
    }
}

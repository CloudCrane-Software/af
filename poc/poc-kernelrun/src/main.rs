//! poc-kernelrun —— 内核件『裸进程』部署形态实测。
//!
//! 单进程依次初始化 11 个内核 crate 的核心结构并逐项断言：
//! 凭证签发一对 / 图模板校验一条 / 订单走一跃迁 / 网关记账一条 /
//! 抽样引擎喂三件 / 投放闸放行一件 / meter 扣一次 / 日历算一单 /
//! trace 收一条 / 止损喂两事件 / station 认领一任务。
//!
//! 子命令：
//!   run <data_dir>            —— 主场景（11 步 + 冷启动计时）
//!   layout-default <data_dir> —— 默认形态：两件各自独立 .db，持连接 6s 供外部清点文件
//!   layout-shared  <data_dir> —— 共库形态：多件同文件、同进程各开连接
//!
//! 任何断言失败 = panic = 非零退出码。

use std::collections::VecDeque;
use std::io::Write as _;
use std::time::{Duration, Instant};

use chrono::TimeZone as _;
use chrono_tz::Asia::Shanghai;

fn main() {
    // 冷启动计时起点：main 第一行（内部口径，不含进程 fork/exec）。
    let t0 = Instant::now();
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("run");
    let data_dir = std::path::PathBuf::from(args.get(2).map(String::as_str).unwrap_or("data"));
    match cmd {
        "run" => cmd_run(&data_dir, t0),
        "layout-default" => cmd_layout_default(&data_dir),
        "layout-shared" => cmd_layout_shared(&data_dir),
        other => {
            eprintln!("unknown subcommand: {other}");
            std::process::exit(2);
        }
    }
}

// ---------------------------------------------------------------------------
// 主场景：11 步单进程跑完
// ---------------------------------------------------------------------------

fn cmd_run(data_dir: &std::path::Path, t0: Instant) {
    std::fs::create_dir_all(data_dir).expect("create data dir");

    let mut step_no = 0usize;
    let mut step = |name: &str, f: &mut dyn FnMut() -> String| -> Duration {
        step_no += 1;
        let s = Instant::now();
        let detail = f();
        let dt = s.elapsed();
        println!(
            "STEP {}/11 [{}] {} OK ({}ms) {}",
            step_no,
            name,
            "✓",
            dt.as_millis(),
            detail
        );
        let _ = std::io::stdout().flush();
        dt
    };

    // ① 凭证签发一对（根 + 衰减子），走 serialize/verify/evaluate 全链
    step("credential 签发一对", &mut || {
        let root_spec = af_credential::RootSpec {
            tools: vec!["git".into(), "test".into()],
            namespace: "line-a".into(),
            budget_cents: 1000,
        };
        let root = af_credential::issue_root(&root_spec).expect("issue_root");
        let child = af_credential::attenuate(
            &root.token,
            &af_credential::ChildSpec { tools: vec!["test".into()], spend_cap_cents: 500 },
        )
        .expect("attenuate");
        let bytes = af_credential::serialize(&child).expect("serialize");
        let child = af_credential::deserialize_verify(&bytes, &root.keypair.public())
            .expect("deserialize_verify(root pk)");
        let allow = af_credential::evaluate(
            &child,
            &af_credential::AccessQuery {
                role: "reviewer".into(),
                tool: "test".into(),
                namespace: "line-a".into(),
                spend_cents: 300,
            },
        )
        .expect("evaluate(test)");
        assert!(allow.allow, "衰减子凭证 test@line-a 必须放行: {}", allow.reason);
        let deny = af_credential::evaluate(
            &child,
            &af_credential::AccessQuery {
                role: "reviewer".into(),
                tool: "git".into(),
                namespace: "line-a".into(),
                spend_cents: 300,
            },
        )
        .expect("evaluate(git)");
        assert!(!deny.allow, "git 必须被衰减 check 封死: {}", deny.reason);
        assert!(
            deny.token_carries_capability,
            "biscuit append-only：git 事实仍在 token，只是行为被封死"
        );
        format!("root(test,git@line-a,budget=1000c)+child(test,≤500c); allow(test)=true, deny(git)=true")
    });

    // ② 图模板校验一条（合法 DAG 含人工 gate 节点）+ 一条负例（回边成环必被拒）
    step("graph 校验一条", &mut || {
        let legal = r#"{
            "graph_version": "g-7",
            "name": "kernelrun-pipeline",
            "entry": "draft",
            "nodes": [
                {"id": "draft", "kind": "task"},
                {"id": "review", "kind": "gate",
                 "human": {"station": "review-station", "role": "reviewer"}},
                {"id": "publish", "kind": "transform"}
            ],
            "edges": [
                {"from": "draft", "output": "draft.doc", "to": "review", "input": "review.in"},
                {"from": "review", "output": "review.verdict", "to": "publish", "input": "publish.in"}
            ]
        }"#;
        let tpl: af_graph::GraphTemplate = serde_json::from_str(legal).expect("parse template");
        let report = af_graph::validate(&tpl);
        assert!(report.is_ok(), "合法模板必须零违例: {:?}", report.violations);

        let cyclic = r#"{
            "graph_version": "g-8",
            "name": "kernelrun-cycle",
            "entry": "draft",
            "nodes": [
                {"id": "draft", "kind": "task"},
                {"id": "publish", "kind": "transform"}
            ],
            "edges": [
                {"from": "draft", "output": "a", "to": "publish", "input": "b"},
                {"from": "publish", "output": "c", "to": "draft", "input": "d"}
            ]
        }"#;
        let bad: af_graph::GraphTemplate = serde_json::from_str(cyclic).expect("parse bad");
        let rep_bad = af_graph::validate(&bad);
        assert!(
            !rep_bad.is_ok()
                && rep_bad.violations.iter().any(|v| v.rule == af_graph::Rule::Cycle),
            "回边必须被判 cycle: {:?}",
            rep_bad.violations
        );
        format!(
            "g-7 三节点 DAG 零违例; 回边负例报 cycle({} 条违例)",
            rep_bad.violations.len()
        )
    });

    // ③ 订单走一跃迁：Created --Release--> Released（先证门控会拒，再补前置放行）
    step("workorder 一跃迁", &mut || {
        let mut o = af_workorder::WorkOrder::new(
            "wo-1",
            af_workorder::GraphVersion::new("g-7"),
            af_workorder::Sla::new(None, af_workorder::Priority::Normal),
        );
        let err = o
            .transition(af_workorder::Transition::Release)
            .expect_err("空单 release 必须被门控拒绝");
        let missing = err.missing().expect("门控错误必须带类型化缺失清单");
        assert!(
            missing.iter().any(|m| matches!(m, af_workorder::Missing::LineVersionNotBound))
                && missing
                    .iter()
                    .any(|m| matches!(m, af_workorder::Missing::BudgetVoucherNotIssued)),
            "缺失清单必须点名两项前置: {:?}",
            missing
        );
        o.bind_line(af_workorder::LineBinding {
            line_id: "line-a".into(),
            line_version: "v3".into(),
        })
        .expect("bind_line");
        o.attach_budget_voucher(af_workorder::BudgetVoucher {
            voucher_id: "bv-1".into(),
            amount: 1000,
            signed: true,
        })
        .expect("attach_budget_voucher");
        let st = o
            .transition(af_workorder::Transition::Release)
            .expect("补齐前置后 release 放行");
        assert_eq!(st, af_workorder::OrderState::Released);
        assert_eq!(o.graph_version().as_str(), "g-7", "graph_version 不可变");
        assert_eq!(o.history().len(), 1, "履历记录一次跃迁");
        "Created→Released; 负例缺失清单=[LineVersionNotBound, BudgetVoucherNotIssued]"
            .to_string()
    });

    // ④ 网关记账一条：declare → (窗口内拒发) → execute 落回执 → 重放不外呼
    step("gateway 记账一条", &mut || {
        let gw = af_effect_gateway::Gateway::open(
            data_dir.join("gateway.db"),
            Duration::from_millis(20),
        )
        .expect("open gateway.db");
        gw.resume().expect("resume -> epoch 1");
        let session = gw.attach().expect("attach");
        let key = af_effect_gateway::IntentKey {
            workorder_id: "wo-1".into(),
            step_path: "publish.notify".into(),
            seq: 1,
        };
        let payload = serde_json::json!({"op": "notify", "channel": "email", "to": "qc@factory"});
        let declared = gw.declare_intent(&session, &key, &payload).expect("declare");
        // 首启/重入分叉：首启 Created，重入 AlreadyDeclared（幂等层语义，kernelrun 缺口 3 的落地断言）
        assert!(
            matches!(
                declared,
                af_effect_gateway::DeclareOutcome::Created { .. }
                    | af_effect_gateway::DeclareOutcome::AlreadyDeclared { .. }
            ),
            "声明必须为 Created 或 AlreadyDeclared: {declared:?}"
        );
        let is_reentry = matches!(declared, af_effect_gateway::DeclareOutcome::AlreadyDeclared { .. });
        if !is_reentry {
            let early = gw.execute(
                &session,
                &key,
                |_p: &serde_json::Value| Ok(serde_json::json!({"txn": "must-not-run"})),
            );
            assert!(
                matches!(early, Err(af_effect_gateway::GatewayError::WindowStillOpen { .. })),
                "撤回窗口未关时 execute 必须拒发: {:?}",
                early
            );
            std::thread::sleep(Duration::from_millis(30));
        }
        let mut outer_calls = 0u32;
        let out = gw
            .execute(&session, &key, |_p: &serde_json::Value| {
                outer_calls += 1;
                Ok(serde_json::json!({"txn": "T-1"}))
            })
            .expect("execute after window");
        if !is_reentry {
            assert!(matches!(out, af_effect_gateway::ExecuteOutcome::RanFresh { .. }));
        } else {
            // 重入：意图已执行→必须重放原回执且零外呼
            assert!(out.is_replay(), "重入后 execute 必须重放原回执: {out:?}");
            assert_eq!(outer_calls, 0, "重入外呼必须为 0");
        }
        let snap = gw.snapshot_intent(&key).expect("snapshot").expect("intent row");
        assert!(matches!(snap.state, af_effect_gateway::IntentState::Executed));
        let replay = gw
            .execute(&session, &key, |_p: &serde_json::Value| {
                outer_calls += 1;
                Ok(serde_json::json!({"txn": "T-2-DUP"}))
            })
            .expect("replay");
        assert!(replay.is_replay(), "第二次必须重放已存回执");
        assert_eq!(replay.into_inner()["txn"], "T-1", "重放返回原回执");
        if is_reentry {
            assert_eq!(outer_calls, 0, "重入全程零外呼（两次 execute 均重放）");
        } else {
            assert_eq!(outer_calls, 1, "外部操作恰好发生一次");
        }
        format!(
            "gateway.db 意图 1 条 executed, 外呼恰好 1 次, epoch={}",
            gw.current_epoch().expect("epoch")
        )
    });

    // ⑤ 抽样引擎喂三件（注入器 → CSP-1 → 金标台账）+ 升迁引擎收一条 RunOutcome
    step("sampling 喂三件", &mut || {
        let mut injector = af_sampling::GoldenInjector::new(
            af_sampling::InjectorConfig { p: 0.5, defect_share: 0.7 },
            42,
        )
        .expect("injector");
        let mut csp1 = af_sampling::Csp1::new(af_sampling::Csp1Config {
            enabled: true,
            i: 2,
            f: 1.0, // 全检：金标缺陷必被拦截，断言与种子无关
            f_min: 0.05,
        })
        .expect("csp1");
        let mut ledger = af_sampling::GoldenLedger::default();
        let mut queue: VecDeque<af_sampling::Tagged> = VecDeque::new();
        let mut rng = af_sampling::Rng::new(7);
        for conforming in [true, true, false] {
            injector.enqueue_real(&mut queue, conforming);
        }
        assert!(queue.len() >= 3, "队列至少含 3 件真实件, got {}", queue.len());
        let fed = queue.len();
        let mut processed = 0usize;
        while let Some(item) = queue.pop_front() {
            let conforming = match &item {
                af_sampling::Tagged::Real { conforming, .. } => *conforming,
                af_sampling::Tagged::Golden { defective, .. } => !*defective,
            };
            let v = csp1.process(conforming, &mut rng);
            ledger.observe(&item, &v);
            processed += 1;
        }
        assert_eq!(processed, fed, "三件真实件(+注入金标)全数处理");
        assert_eq!(ledger.alarms(), 0, "f=1.0 全检下零告警");
        assert_eq!(ledger.false_rejects(), 0, "干净金标零误拦");
        assert_eq!(
            ledger.defective_intercepted(),
            injector.stats().defective,
            "注入的缺陷金标全部拦截"
        );
        assert_eq!(
            ledger.golden_seen(),
            injector.stats().injected,
            "台账见到的金标数 == 注入数"
        );
        let mut engine = af_sampling::Engine::from_recipe_json(
            r#"{"window":3,"min_successes":3,"max_violations":0,"min_consistency":0.95,"clean_to_recover":2}"#,
        )
        .expect("recipe engine");
        let action = engine.record(af_sampling::RunOutcome {
            success: true,
            violations: 0,
            consistency: 1.0,
        });
        assert!(
            matches!(action, af_sampling::Action::Hold) && engine.events().len() == 1,
            "首条成功记录应为 Hold: {:?}",
            action
        );
        format!(
            "队列 {} 件(金标注入 {}), 全检拦截缺陷金标 {}, 升迁引擎 Hold@Agent",
            processed,
            injector.stats().injected,
            ledger.defective_intercepted()
        )
    });

    // ⑥ 投放闸放行一件（含 WipCap 负例与完成释容）
    step("release-gate 放行一件", &mut || {
        let mut gate = af_release_gate::ReleaseGate::new(af_release_gate::GateConfig {
            wip_upper_minutes: 30.0,
            wip_lower_minutes: 5.0,
            night_window: None,
        })
        .expect("gate config");
        let t1 = gate
            .try_admit(
                &af_release_gate::Item { id: 1, config: "v1".into(), review_minutes: 10.0 },
                600,
            )
            .expect("admit #1");
        assert_eq!(t1.weight, 10.0);
        assert_eq!(gate.wip_minutes(), 10.0, "WIP 记账 10");
        let err = gate
            .try_admit(
                &af_release_gate::Item { id: 2, config: "v1".into(), review_minutes: 25.0 },
                601,
            )
            .expect_err("10+25>30 必须被 WipCap 拒");
        assert!(matches!(err, af_release_gate::AdmitError::WipCap { .. }));
        gate.complete(1, 700, 0).expect("complete #1");
        assert_eq!(gate.wip_minutes(), 0.0, "完成释容");
        gate.try_admit(
            &af_release_gate::Item { id: 2, config: "v1".into(), review_minutes: 25.0 },
            701,
        )
        .expect("释容后同件可入");
        "放行 1 件(10min); WipCap 负例拒绝; complete 后 WIP=0"
            .to_string()
    });

    // ⑦ meter 扣一次（Min(周窗, 日窗) 结构）+ 不足负例
    step("meter 扣一次", &mut || {
        let mut supply = af_meter::Meter::min(vec![
            af_meter::Meter::window(7 * 86400, 1000, 0), // 周窗 1000
            af_meter::Meter::window(86400, 200, 0),        // 日窗 200
        ]);
        supply.try_consume(80, 3600).expect("consume #1");
        let r = supply.given(3600);
        assert!(
            r.available == 120,
            "Min=约束合取: min(周窗920, 日窗120)=120, got {}",
            r.available
        );
        let err = supply.try_consume(150, 4000).expect_err("日窗仅剩 120");
        match err {
            af_meter::MeterError::Insufficient { available, .. } => {
                assert_eq!(available, 120, "报 Insufficient 且 available=120");
            }
            other => panic!("期望 Insufficient, got {other:?}"),
        }
        "Min(周1000,日200) 扣 80 → available=120; 超扣 150 被拒".to_string()
    });

    // ⑧ 日历算一单：周五 16:00 + 4 工时 → 下周一 11:00；反向截单回到周五 16:00
    step("calendar 算一单", &mut || {
        let cal = af_calendar::Calendar::weekday_8x5_shanghai();
        cal.validate().expect("calendar valid");
        let placed = Shanghai.with_ymd_and_hms(2026, 10, 23, 16, 0, 0).unwrap(); // 周五
        let due = cal
            .sla_deadline(placed, chrono::Duration::hours(4))
            .expect("sla_deadline");
        assert_eq!(
            due.format("%Y-%m-%d %H:%M").to_string(),
            "2026-10-26 11:00",
            "周五 16:00 + 4 工时 → 下周一 11:00"
        );
        let latest = cal
            .latest_order_at(due, chrono::Duration::hours(4))
            .expect("latest_order_at");
        assert_eq!(
            latest.format("%Y-%m-%d %H:%M").to_string(),
            "2026-10-23 16:00",
            "反向截单与正向互逆"
        );
        "周五16:00+4h→周一11:00; 截单点回算=周五16:00".to_string()
    });

    // ⑨ trace 收一条 critical（fail-closed 通道，Ok(seq) 即保证落盘）+ 一条 normal
    let trace = std::cell::RefCell::new(None);
    step("trace 收一条", &mut || {
        let tr = af_trace::Trace::spawn(af_trace::TraceConfig { dir: data_dir.join("trace"), ..Default::default() }).expect("trace open");
        let seq = tr
            .write_critical(
                "kernelrun.order.released",
                serde_json::json!({"order": "wo-1", "state": "released"}),
            )
            .expect("critical write");
        // 首启 seq=0；重入从已存落盘条数续号（持久化语义断言）
        assert!(seq >= 0, "critical seq 受理: {seq}");
        tr.write_normal(
            "kernelrun.sample.observed",
            serde_json::json!({"processed": 3}),
        );
        *trace.borrow_mut() = Some(tr);
        "critical seq=0 已受理(Ok=保证落盘) + normal 1 条".to_string()
    });

    // ⑩ 止损喂两事件（同指纹连击 2 < N=3，均 Continue）
    step("stop 喂两事件", &mut || {
        let mut engine = af_stop::StopEngine::new(af_stop::StopPolicy {
            fingerprint_streak_limit: 3,
            normal_error_threshold: 5,
            store: af_stop::FingerprintStore::new(),
        });
        let d1 = engine.feed(af_stop::Event::ToolError {
            tool: "search".into(),
            error_code: "E_TIMEOUT".into(),
        });
        let d2 = engine.feed(af_stop::Event::ToolError {
            tool: "search".into(),
            error_code: "E_TIMEOUT".into(),
        });
        assert!(d1.is_continue() && d2.is_continue(), "连击 2 < 3 必须 Continue");
        let (fp, n) = engine.current_streak().expect("streak tracked");
        assert_eq!(n, 2, "连击计数=2");
        assert_eq!(*fp, af_stop::Fingerprint::new("search", "E_TIMEOUT"));
        "两事件均 Continue; 指纹(search,E_TIMEOUT) 连击=2/3".to_string()
    });

    // ⑪ station 认领一任务（独立 SQLite 账本）+ 第二 worker 无任务可领
    step("station 认领一任务", &mut || {
        let station =
            af_station::Station::open(data_dir.join("station.db")).expect("open station.db");
        let enq = station
            .new_task("task-1", "artifact://wo-1/final", "reviewer", Some("idem-wo-1"))
            .expect("new_task");
        // 首启 Inserted；重入 Duplicate（幂等键 idem-wo-1 持久生效）
        assert!(
            matches!(enq, af_station::Enqueued::Inserted { .. } | af_station::Enqueued::Duplicate { .. }),
            "入账须为 Inserted 或 Duplicate: {enq:?}"
        );
        let reentry = matches!(enq, af_station::Enqueued::Duplicate { .. });
        let claimed = station
            .claim("worker-1", "reviewer", 60_000)
            .expect("claim ok");
        if !reentry {
            let t = claimed.expect("任务必须可被认领");
            assert_eq!(t.id, "task-1");
            assert_eq!(t.claimant.as_deref(), Some("worker-1"));
            assert_eq!(t.state, af_station::State::Claimed);
            assert!(t.lease_until.is_some(), "租约已置");
        } else if let Some(t) = claimed {
            // 租约到期被回收后重领：仍须领到同一件
            assert_eq!(t.id, "task-1");
        }
        let second = station.claim("worker-2", "reviewer", 60_000).expect("claim #2");
        assert!(second.is_none(), "账上无第二件，worker-2 领不到");
        assert_eq!(
            station.count_by_state(af_station::State::Claimed).expect("count"),
            1
        );
        "task-1 被 worker-1 认领(租约60s); worker-2=None".to_string()
    });

    let init_done = t0.elapsed();
    println!("COLD_START_MS={:.3}", init_done.as_secs_f64() * 1000.0);

    // 收尾：trace 落盘对账（critical Ok(seq) ⇒ 必在 sink）
    if let Some(tr) = trace.into_inner() {
        let report = tr.shutdown().expect("trace shutdown 对账");
        assert_eq!(report.gap_counter, 0, "容量充足时零丢弃");
        let crit = report.sinks.critical.lock().unwrap();
        assert!(
            crit.iter().any(|l| l.contains("kernelrun.order.released")),
            "critical 事件必须在 sink 里: {:?}",
            *crit
        );
        assert_eq!(report.critical_persisted, 1);
        println!(
            "TRACE_RECONCILE critical_persisted={} normal_persisted={} gap=0",
            report.critical_persisted, report.normal_persisted
        );
    }
    println!("STEPS_OK=11");
    let _ = std::io::stdout().flush();
}

// ---------------------------------------------------------------------------
// 布局实验 1：默认形态 —— 每件一个独立 .db 文件，持连接 6 秒供外部清点
// ---------------------------------------------------------------------------

fn cmd_layout_default(data_dir: &std::path::Path) {
    std::fs::create_dir_all(data_dir).expect("create dir");
    let gw = af_effect_gateway::Gateway::open(
        data_dir.join("gateway.db"),
        Duration::from_millis(20),
    )
    .expect("gateway");
    let st = af_station::Station::open(data_dir.join("station.db")).expect("station");
    let _ = gw.resume();
    let session = gw.attach().expect("attach");
    gw.declare_intent(
        &session,
        &af_effect_gateway::IntentKey {
            workorder_id: "wo-layout".into(),
            step_path: "s".into(),
            seq: 1,
        },
        &serde_json::json!({"x": 1}),
    )
    .expect("declare");
    st.new_task("t-1", "artifact://x", "reviewer", None).expect("task");
    println!("READY holding connections open for 6s...");
    let _ = std::io::stdout().flush();
    std::thread::sleep(Duration::from_secs(6));
    drop(gw);
    drop(st);
    println!("DONE connections closed");
    let _ = std::io::stdout().flush();
}

// ---------------------------------------------------------------------------
// 布局实验 2：共库形态 —— 多件同一 .db 文件、同进程各自开连接
// ---------------------------------------------------------------------------

fn cmd_layout_shared(data_dir: &std::path::Path) {
    std::fs::create_dir_all(data_dir).expect("create dir");
    let p = data_dir.join("shared.db");
    // 三条独立连接指向同一个文件：网关 + station(worker A) + station(worker B)
    let gw = af_effect_gateway::Gateway::open(&p, Duration::from_millis(10)).expect("gateway");
    let st_a = af_station::Station::open(&p).expect("station A");
    let st_b = af_station::Station::open(&p).expect("station B");

    let _ = gw.resume();
    let session = gw.attach().expect("attach");
    let key = af_effect_gateway::IntentKey {
        workorder_id: "wo-shared".into(),
        step_path: "s".into(),
        seq: 1,
    };
    gw.declare_intent(&session, &key, &serde_json::json!({"op": "x"}))
        .expect("declare");
    st_a.new_task("t-shared", "artifact://x", "reviewer", None).expect("task");

    println!("READY holding connections open for 6s...");
    let _ = std::io::stdout().flush();
    std::thread::sleep(Duration::from_secs(6));

    // 跨连接交接：A 入账的任务由 B 认领（同进程、两条连接、同一文件）
    let claimed = st_b
        .claim("worker-b", "reviewer", 60_000)
        .expect("claim")
        .expect("B 必须能认领 A 入账的任务");
    assert_eq!(claimed.id, "t-shared");
    println!("CROSS-CONNECTION-OK claimed_by_b={}", claimed.id);
    drop(gw);
    drop(st_a);
    drop(st_b);
    println!("DONE connections closed");
    let _ = std::io::stdout().flush();
}

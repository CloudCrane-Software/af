//! P3-A1 最小接缝——策略库 → DSL → 编译 → 被跑 → 读数。
//!
//! 闭环断点（本批实测）：全仓消费 `strategies/` 的只有 af-dsl 装载测试
//! （load+validate）与 14d 门——零「编译到 plan」接线、零执行接线。本接缝
//! 补最后一环：把一条 compilable 条目的 `dsl_snippet` 经**真编译器四段全链**
//! （load→validate→lower→bind→plan，af-compiler 正本）产出 plan.json，再经
//! **真图执行器**（af-execline `GraphExecuteUnit` 正本）驱动到完成，注入
//! 零模型 ScriptRunner（终态消息 JSON=envelope 回退源，graph_unit.rs 契约
//! 面）与零容器 FakeBridge（docker 面脚本化 OK），读数落 FileRecordSink
//! （af.plan.record.v1 行）+ 工件面。
//!
//! 两个子命令：
//!   compile-all —— 61 条逐条过四段链（对真 af/packages 注册表），逐条
//!                  stage 级读数（load/validate/lower/bind/plan 哪段断）；
//!   run-one     —— 单条 compilable 全链编译（对覆盖层注册表）+ 真执行，
//!                  读数 JSON 落 stdout。
//!
//! 纪律：本工程是证据面工件（poc/），不修改 crates/ 任何既有件；模型零
//! 消耗（runner 打桩）、容器零起（bridge 打桩）、凭证零读取。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use af_compiler::{bind, generate_plan, lower, ComponentRegistry};
use af_execline::exec::{ExecContext, ExecuteUnit};
use af_execline::graph_unit::{GraphExecuteUnit, GraphUnitSpec};
use af_execline::record::FileRecordSink;
use serde_json::{json, Value};

// ============================================================
// 策略条目装载（YAML 面；三态判定与 R1 同尺）
// ============================================================

struct Entry {
    id: String,
    dsl_state: String,
    snippet: Option<String>,
    graph_version: String,
}

fn load_entry(repo: &Path, id: &str) -> Result<Entry, String> {
    let path = repo.join("strategies").join(format!("{id}.yaml"));
    let src = std::fs::read_to_string(&path).map_err(|e| format!("读 {path:?}: {e}"))?;
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&src).map_err(|e| format!("{id} YAML 解析: {e}"))?;
    let id = doc
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("缺 id")?
        .to_string();
    let dsl_state = doc
        .get("dsl_state")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let snippet = doc
        .get("dsl_snippet")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let graph_version = snippet
        .as_ref()
        .and_then(|s| serde_yaml::from_str::<serde_yaml::Value>(s).ok())
        .and_then(|d| {
            d.get("meta")
                .and_then(|m| m.get("graph_version"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    Ok(Entry { id, dsl_state, snippet, graph_version })
}

fn list_ids(repo: &Path) -> Vec<String> {
    let dir = repo.join("strategies");
    let mut ids: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("读 strategies/ 失败: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "yaml")
                && p.file_name().and_then(|n| n.to_str()) != Some("index.yaml")
        })
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    ids.sort();
    ids
}

// ============================================================
// 四段全链（对指定注册表根）——stage 级读数
// ============================================================

struct ChainReading {
    id: String,
    dsl_state: String,
    load: Result<(), String>,
    validate: Result<(), String>,
    lower: Result<(), String>,
    bind: Result<(), String>,
    plan: Result<(), String>,
    plan_id: Option<String>,
}

fn four_stage(_repo: &Path, entry: &Entry, registry_root: &Path) -> ChainReading {
    let mut r = ChainReading {
        id: entry.id.clone(),
        dsl_state: entry.dsl_state.clone(),
        load: Err("non-compilable（本批只对 compilable 跑四段链）".into()),
        validate: Err("-".into()),
        lower: Err("-".into()),
        bind: Err("-".into()),
        plan: Err("-".into()),
        plan_id: None,
    };
    let Some(snippet) = &entry.snippet else {
        return r;
    };
    let doc = match af_dsl::load_str(snippet) {
        Ok(d) => {
            r.load = Ok(());
            d
        }
        Err(e) => {
            r.load = Err(format!("{e}"));
            return r;
        }
    };
    let report = af_dsl::validate(&doc);
    if report.is_ok() {
        r.validate = Ok(());
    } else {
        r.validate = Err(report.to_string());
        return r;
    }
    let ir = match lower(&doc) {
        Ok(ir) => {
            r.lower = Ok(());
            ir
        }
        Err(e) => {
            r.lower = Err(e.to_string());
            return r;
        }
    };
    let registry = match ComponentRegistry::load(registry_root) {
        Ok(reg) => reg,
        Err(e) => {
            r.bind = Err(format!("注册表装载失败: {e:#}"));
            return r;
        }
    };
    let bound = bind(&ir, &registry);
    if bound.is_ok() {
        r.bind = Ok(());
    } else {
        r.bind = Err(bound
            .violations
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("; "));
        return r;
    }
    match generate_plan(&ir, &bound) {
        Ok(plan) => {
            r.plan = Ok(());
            r.plan_id = Some(plan.body.plan_id.clone());
        }
        Err(e) => r.plan = Err(format!("{e}")),
    }
    r
}

fn stage_cell(r: &Result<(), String>) -> Value {
    match r {
        Ok(()) => json!("pass"),
        Err(e) if e == "-" => json!("-"),
        Err(e) => json!(format!("{e}").chars().take(240).collect::<String>()),
    }
}

fn compile_all(repo: &Path, registry_root: &Path) -> Value {
    let mut rows = Vec::new();
    for id in list_ids(repo) {
        let entry = load_entry(repo, &id).unwrap_or_else(|e| panic!("{id}: {e}"));
        let r = four_stage(repo, &entry, registry_root);
        rows.push(json!({
            "id": r.id,
            "dsl_state": r.dsl_state,
            "stages": {
                "load": stage_cell(&r.load),
                "validate": stage_cell(&r.validate),
                "lower": stage_cell(&r.lower),
                "bind": stage_cell(&r.bind),
                "plan": stage_cell(&r.plan),
            },
            "plan_id": r.plan_id,
        }));
    }
    let compilable: Vec<_> = rows.iter().filter(|r| r["dsl_state"] == "compilable").collect();
    let full = compilable
        .iter()
        .filter(|r| r["stages"]["plan"] == "pass")
        .count();
    json!({
        "mode": "compile-all",
        "registry_root": registry_root,
        "total": rows.len(),
        "compilable": compilable.len(),
        "compilable_pass_four_stage": full,
        "per_entry": rows,
    })
}

// ============================================================
// 注入面：ScriptRunner（零模型）+ FakeBridge（零容器）
// ============================================================

/// 每节点一次 launch 的捕获面（读数：模型档位/turns 帽/提示词装配）。
#[derive(Clone)]
struct CapturedLaunch {
    run_id: String,
    node: String,
    model: String,
    max_turns: u32,
    spend_cap_cents: i64,
    system_head: String,
    user_head: String,
    tool_names: Vec<String>,
}

/// 节点名 → 终态 assistant 文本（envelope 载荷 JSON；graph_unit.rs 契约的
/// 回退交付源——EX4006 对其同尺校验）。
fn scripted_payload(instance: &str, node: &str) -> Option<String> {
    let _ = instance;
    let text = match node {
        // st-b-C037 explore_then_fix：explorer 发 locate_report，fixer 发 patch。
        "explorer" => json!({
            "report": {
                "hits": ["crates/af-dsl/src/lib.rs", "crates/af-compiler/src/plan.rs"],
                "confidence_milli": 812
            }
        }),
        "fixer" => json!({
            "patch": {
                "ref": "p3a1-min-seam-001",
                "diff_ref": "poc/poc-p3-a1-strategy-dsl#artifacts"
            }
        }),
        // st-b-C102 coder_role_task_graph：analyze→locate→patch_role→review（verify
        // 是 check 节点，unit 直调，不经 runner）。
        "analyze" => json!({
            "analysis": {
                "root_cause": "strategy→DSL loop seam missing: no plan-compile wiring",
                "suspect_files": ["poc/poc-p3-a1-strategy-dsl/seam/src/main.rs"]
            }
        }),
        "locate" => json!({
            "report": {
                "hits": ["poc/poc-p3-a1-strategy-dsl/seam/src/main.rs"]
            }
        }),
        "patch_role" => json!({
            "patch": {
                "ref": "p3a1-seam-st-b-C102-001",
                "diff_ref": "poc/poc-p3-a1-strategy-dsl#run/attempt/artifacts"
            }
        }),
        "review" => json!({
            "reviewed": {
                "ref": "p3a1-seam-st-b-C102-001",
                "diff_ref": "poc/poc-p3-a1-strategy-dsl#run/attempt/artifacts"
            }
        }),
        _ => return None,
    };
    Some(text.to_string())
}

struct ScriptRunner {
    instance: String,
    launches: Mutex<Vec<CapturedLaunch>>,
}

impl af_execline::AgentRunner for ScriptRunner {
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        let node = launch
            .run_id
            .strip_prefix(&format!("{}-", self.instance))
            .unwrap_or_default()
            .to_string();
        let final_text = scripted_payload(&self.instance, &node).ok_or_else(|| {
            af_execline::AgentRunFailure { message: format!("ScriptRunner 无节点 {node} 脚本"), http_retries: 0 }
        })?;
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(CapturedLaunch {
                run_id: launch.run_id.clone(),
                node: node.clone(),
                model: launch.spec.endpoint.model.clone(),
                max_turns: launch.spec.policy.max_turns,
                spend_cap_cents: launch.spend_cap_cents,
                system_head: launch.spec.context_template.system.chars().take(160).collect(),
                user_head: launch.user_message.chars().take(160).collect(),
                tool_names: launch.extra_tools.iter().map(|t| t.name.clone()).collect(),
            });
        let report = af_harness::RunReport {
            run_id: launch.run_id,
            status: af_harness::RunStatus::Completed {
                signal: "TASK_COMPLETE".into(),
            },
            turns: 3,
            model_calls: 5,
            effect_replays: 0,
            effect_fresh: 2,
            transcript: vec![
                af_harness::TranscriptItem::User {
                    text: launch.user_message.clone(),
                },
                af_harness::TranscriptItem::Assistant {
                    text: final_text,
                    tool_calls: Vec::new(),
                    usage: None,
                },
            ],
            whitelist: launch.extra_tools.iter().map(|t| t.name.clone()).collect(),
            tokens: af_harness::TokenTotals {
                input: 100,
                output: 50,
                cache_read: 10,
            },
        };
        Ok(af_execline::AgentRunOutcome { report, http_retries: 0 })
    }
}

struct FakeBridge;

impl af_execline::HostBridge for FakeBridge {
    fn run(&self, _host: &str, script: &str, _t: u32) -> Result<(bool, String), String> {
        if script.starts_with("docker rm -f") || script.starts_with("docker run") {
            Ok((true, String::new()))
        } else if script.contains("docker exec") {
            Ok((true, "Ran 2 checks in 1s\n\nOK\n__AF_RC=0".into()))
        } else {
            Ok((true, String::new()))
        }
    }
}

// ============================================================
// run-one：单条全链编译（覆盖层注册表）+ 真执行 + 读数
// ============================================================

fn run_one(repo: &Path, id: &str, registry_root: &Path, work: &Path, issue_text: &str) -> Value {
    let t0 = Instant::now();
    let entry = load_entry(repo, id).unwrap_or_else(|e| panic!("{id}: {e}"));
    if entry.dsl_state != "compilable" {
        return json!({ "error": format!("{id} 非 compilable（{}）", entry.dsl_state) });
    }
    // 四段全链（对覆盖层注册表——digest 与执行面同源）。
    let chain = four_stage(repo, &entry, registry_root);
    let chain_json = json!({
        "load": stage_cell(&chain.load),
        "validate": stage_cell(&chain.validate),
        "lower": stage_cell(&chain.lower),
        "bind": stage_cell(&chain.bind),
        "plan": stage_cell(&chain.plan),
    });
    if chain.plan.is_err() {
        return json!({ "id": id, "chain": chain_json, "error": "四段链未过，不执行" });
    }
    let t_compile = t0.elapsed();

    // 任务包（v1 约定 <data_root>/<task>/{plan.json, task.json}）。
    let snippet = entry.snippet.expect("compilable 必有 snippet");
    let doc = af_dsl::load_str(&snippet).expect("链已过");
    let registry = ComponentRegistry::load(registry_root).expect("链已过");
    let ir = lower(&doc).expect("链已过");
    let bound = bind(&ir, &registry);
    assert!(bound.is_ok(), "链已过（bind）");
    let plan = generate_plan(&ir, &bound).expect("链已过");
    let task_id = format!("p3a1-{id}");
    let task_dir = work.join("data").join(&task_id);
    std::fs::create_dir_all(&task_dir).expect("建任务包目录");
    let plan_src = serde_json::to_string_pretty(&plan).expect("plan 序列化");
    std::fs::write(task_dir.join("plan.json"), format!("{plan_src}\n")).expect("写 plan.json");
    let task_input = json!({
        "issue": issue_text,
        "task_id": task_id,
        // check 节点（test_unit）的容器判据命令（FakeBridge 打桩执行面，零真容器）。
        "test_command": "cd /testbed && echo p3a1-seam-check-ok",
    });
    std::fs::write(
        task_dir.join("task.json"),
        format!("{}\n", serde_json::to_string_pretty(&task_input).expect("task 序列化")),
    )
    .expect("写 task.json");
    // 判据面/计划面摘要钉死（生产 submit 面语义，红队 W1-C59/W2-C55）。
    let task_sha =
        af_execline::exec::file_sha256_hex(&task_dir.join("task.json")).expect("task sha");
    let plan_sha =
        af_execline::exec::file_sha256_hex(&task_dir.join("plan.json")).expect("plan sha");

    // 执行：真 GraphExecuteUnit + 注入零模型/零容器。
    let runner = Arc::new(ScriptRunner {
        instance: task_id.clone(),
        launches: Mutex::new(Vec::new()),
    });
    let spec = GraphUnitSpec::v1("glm-cp", registry_root);
    let unit = GraphExecuteUnit::new(spec, Arc::new(FakeBridge), runner.clone())
        .expect("图单元装载");
    let attempt = work.join("attempt");
    let out = work.join("out");
    std::fs::create_dir_all(&attempt).expect("建 attempt");
    std::fs::create_dir_all(&out).expect("建 out");
    let sink = FileRecordSink::open(&out).expect("record sink");
    let ctx = ExecContext {
        order_id: "wo-p3a1-seam-001",
        stable_id: "sid-p3a1-001",
        line_id: "line-p3a1-seam",
        out_dir: &out,
        task_id: &task_id,
        host_alias: "local",
        data_root: &work.join("data"),
        attempt_dir: &attempt,
        record_sink: &sink,
        graph_version: Some(&entry.graph_version),
        task_sha256: Some(&task_sha),
        plan_sha256: Some(&plan_sha),
        trial: false,
    };
    let t1 = Instant::now();
    let exec = unit.execute(&ctx);
    let t_exec = t1.elapsed();
    let launches: Vec<CapturedLaunch> = runner
        .launches
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let artifacts: Vec<Value> = match &exec {
        Ok(o) => o
            .artifacts
            .iter()
            .map(|a| serde_json::to_value(a).unwrap_or(json!("?")))
            .collect(),
        Err(_) => vec![],
    };
    json!({
        "id": id,
        "task_id": task_id,
        "chain": chain_json,
        "plan_id": plan.body.plan_id,
        "graph_version": entry.graph_version,
        "task_sha256": task_sha,
        "plan_sha256": plan_sha,
        "exec": match &exec {
            Ok(o) => json!({
                "ok": true,
                "verdict": o.verdict,
                "note": o.note,
                "record": o.record,
                "artifacts": artifacts,
                "evidence_files": o.evidence.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            }),
            Err(e) => json!({ "ok": false, "error": format!("{e}").chars().take(600).collect::<String>() }),
        },
        "node_launches": launches.iter().map(|l| json!({
            "run_id": l.run_id,
            "node": l.node,
            "model": l.model,
            "max_turns": l.max_turns,
            "spend_cap_cents": l.spend_cap_cents,
            "system_head": l.system_head,
            "user_head": l.user_head,
            "tools": l.tool_names,
        })).collect::<Vec<_>>(),
        "timings": {
            "compile_four_stage_ms": t_compile.as_millis() as u64,
            "execute_ms": t_exec.as_millis() as u64,
            "total_ms": t0.elapsed().as_millis() as u64,
        },
        "work_dir": work,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "用法：p3a1-seam compile-all <repo> <registry_root> | run-one <repo> <id> <registry_root> <work_dir>";
    if args.len() < 3 {
        eprintln!("{usage}");
        std::process::exit(2);
    }
    let mode = args[0].as_str();
    let repo = PathBuf::from(&args[1]);
    let out = match mode {
        "compile-all" => compile_all(&repo, Path::new(&args[2])),
        "run-one" if args.len() >= 5 => {
            let issue = std::env::var("P3A1_ISSUE")
                .unwrap_or_else(|_| "P3-A1 最小闭环实例：验证 explore→fix 两节点图可被真执行器驱动".into());
            run_one(&repo, &args[2], Path::new(&args[3]), Path::new(&args[4]), &issue)
        }
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    println!("{}", serde_json::to_string_pretty(&out).expect("读数序列化"));
}

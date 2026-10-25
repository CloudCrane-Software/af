//! 执行器对抗语料（农田模式，2026-10-25）——对合流后主线图执行器投喂
//! ≥40 份畸形 plan，期望全部**正规拒绝**（稳定 EX 码 + 点名诊断）或
//! （合法大值族）正常完成：无 panic / 无 crash / 无静默吞。
//!
//! 语料形态：以真编译器全链产出的 sop_full 计划为基线（load→validate→
//! plan_json，`af/packages` 仓库正本——digest/模板核对走真路径），逐例
//! 施加一族确定性篡改（Value 级 mutation + 少量 raw 串篡改），经
//! `GraphExecuteUnit::execute` 全链驱动（FakeBridge 零容器 + FakeRunner
//! 零模型——不耗真模型）。
//!
//! **可复现（seed 固定）**：一切随机性收口在 [`Lcg`]（seed=20261025，
//! 确定性线性同余），篡改位选择/垃圾串生成全经它驱动——同一 seed 两次
//! 跑产出逐位相同的语料与裁定。
//!
//! 畸形族（ask 口径）：信封类型不匹配 / registry_digest 篡改（应 EX4004）/
//! 缺节点引用（node_exec↔graph、边端点、entry、终态发射者）/ 预算字段
//! 非法（负数/零/超/非正典 id）/ 未知 unit 引用 / 线性链成环 / 空 node_exec /
//! 深嵌套 / 超长字段 / typed envelope 违例 / 重复投影 / graph_version 漂移。
//!
//! 判定记录：单测先打印**全量裁定分布表**再断言——一次跑即拿全量诊断
//!（红线批量暴露，不首错短路）。panic 经 catch_unwind 归并为 PANIC 裁定
//! （= 最严重级洞：crash 面）。

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use af_execline::error::ErrorCode;
use af_execline::exec::{ExecContext, ExecuteUnit};
use af_execline::graph_unit::{GraphExecuteUnit, GraphUnitSpec};
use af_execline::record::FileRecordSink;
use serde_json::{json, Value};

const TASK: &str = "django__django-11099";
const PROFILE: &str = "glm-cp";
/// 语料固定 seed（登记于文件头注释；换 seed = 换语料代际）。
const SEED: u64 = 20261025;

fn repo_packages() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn temp_root(tag: &str) -> PathBuf {
    let safe: String = tag.replace(['/', '\\', ' '], "-");
    let root = std::env::temp_dir().join(format!("af-adv-{safe}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn sop_full_src() -> String {
    std::fs::read_to_string(
        repo_packages()
            .join("sweng")
            .join("graphs")
            .join("sop_full@1.0.afb"),
    )
    .expect("sop_full@1.0.afb 仓库正本应在盘")
}

fn compile_plan(graph_src: &str) -> String {
    let doc = af_dsl::load_str(graph_src).expect("sop_full 应装载");
    let verdict = af_dsl::validate(&doc);
    assert!(verdict.is_ok(), "sop_full 应零违例：{verdict}");
    let reg = af_compiler::ComponentRegistry::load(repo_packages()).expect("仓库注册表应装载");
    af_compiler::plan_json(&doc, &reg).expect("sop_full 计划应生成")
}

// ============================================================
// 确定性随机源（seed 固定——语料可复现的收口点）
// ============================================================

/// 线性同余（常数取 MMIX Knuth 64-bit LCG；seed 固定 → 序列确定）。
struct Lcg(u64);

impl Lcg {
    fn seeded() -> Self {
        Self(SEED)
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ============================================================
// Fake 注入面（tests/graph_unit.rs 同款手法：零容器 / 零模型）
// ============================================================

struct FakeBridge {
    calls: Mutex<Vec<String>>,
}

impl FakeBridge {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl af_execline::HostBridge for FakeBridge {
    fn run(&self, _host: &str, script: &str, _t: u32) -> Result<(bool, String), String> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(script.to_string());
        if script.starts_with("docker rm -f") || script.starts_with("docker run") {
            Ok((true, String::new()))
        } else if script.contains("docker exec") {
            Ok((true, "Ran 12 tests in 3s\n\nOK\n__AF_RC=0".into()))
        } else {
            Ok((true, String::new()))
        }
    }
}

#[derive(Debug, Clone)]
struct CapturedLaunch {
    run_id: String,
    max_turns: u32,
}

struct GraphFakeRunner {
    launches: Mutex<Vec<CapturedLaunch>>,
    scripted: fn(&str) -> Option<String>,
    /// declare 契约族脚本（None = 纯 plan/task 篡改族——declare 以终态文本
    /// 解析入参的既有行为）。
    declare_script: Option<DeclareScript>,
}

impl GraphFakeRunner {
    fn new(scripted: fn(&str) -> Option<String>) -> Self {
        Self {
            launches: Mutex::new(Vec::new()),
            scripted,
            declare_script: None,
        }
    }
    fn with_declare_script(
        scripted: fn(&str) -> Option<String>,
        declare_script: DeclareScript,
    ) -> Self {
        Self {
            launches: Mutex::new(Vec::new()),
            scripted,
            declare_script: Some(declare_script),
        }
    }
    fn launches(&self) -> Vec<CapturedLaunch> {
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// sop_full 各节点脚本化 envelope 载荷（tests/graph_unit.rs 同款正例）。
fn sop_full_payloads(node: &str) -> Option<String> {
    let text = match node {
        "planner" => json!({"plan": {"instance_id": TASK, "steps": [
            {"id": "s1", "action": "locate", "target": "django/db/models"}
        ]}}),
        "coder" => json!({"files": [
            {"path": "django/db/models/foo.py", "content_ref": "workspace#1"}
        ]}),
        "reviewer" => json!({"review": {"approved": true, "issues": []}}),
        _ => return None,
    };
    Some(text.to_string())
}

impl af_execline::AgentRunner for GraphFakeRunner {
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        let node = launch
            .run_id
            .strip_prefix(&format!("{TASK}-"))
            .unwrap_or_default()
            .to_string();
        // declare 契约族覆写：目标节点的终态文本与 declare 入参可分别覆写。
        let script = self.declare_script.clone();
        let is_target = script.as_ref().is_some_and(|s| s.target == node);
        let final_text = if is_target {
            script.as_ref().unwrap().final_text.clone()
        } else {
            None
        }
        .or_else(|| (self.scripted)(&node))
        .ok_or_else(|| af_execline::AgentRunFailure {
            message: format!("FakeRunner 无节点 {node} 脚本"),
            http_retries: 0,
        })?;
        if let Some(binding) = launch.extra_tools.iter().find(|t| t.name == "declare") {
            let args = if is_target {
                script.as_ref().unwrap().args.clone()
            } else {
                serde_json::from_str(&final_text).unwrap_or(Value::Null)
            };
            let _ = (binding.handler)(&args);
        }
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(CapturedLaunch {
                run_id: launch.run_id.clone(),
                max_turns: launch.spec.policy.max_turns,
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
        Ok(af_execline::AgentRunOutcome {
            report,
            http_retries: 0,
        })
    }
}

// ============================================================
// 语料表
// ============================================================

/// 期待裁定：正规拒收（码 + 可选点名关键字）或正常完成（含对捕获面的
/// 附加断言——如 turns 帽饱和语义）。
enum Expect {
    Ok,
    /// 完成 + 断言给定节点的 launch max_turns == 给定值。
    OkTurns(&'static str, u32),
    Ex(ErrorCode),
    /// 拒收 + 码在集合内 + 文案点名关键字（诊断精度面）。
    ExAny(&'static [ErrorCode], &'static str),
}

/// 篡改函数：对 (plan Value, task Value) 施加确定性篡改；返回 Some(raw)
/// 时直接以 raw 串作为 plan.json 全文（信封层垃圾族）。
type Mutate = Box<dyn Fn(&mut Value, &mut Value, &mut Lcg) -> Option<String>>;

/// declare 契约面覆写（非可推导端口畸形载荷族，2026-10-25 立法批）：对目标
/// 节点以给定参数调 declare 工具（畸形载荷主注入面），终态 assistant 文本
/// 可选覆写（None = 维持脚本化正例；Some("") = 回退源缺席）。
#[derive(Clone)]
struct DeclareScript {
    /// 目标节点（覆写只对该节点生效）。
    target: &'static str,
    /// declare 工具调用参数。
    args: Value,
    /// 终态 assistant 文本覆写。
    final_text: Option<String>,
}

/// 单例语料。
struct Case {
    family: &'static str,
    name: &'static str,
    mutate: Mutate,
    expect: Expect,
    /// declare 契约族脚本（None = 纯 plan/task 篡改族）。
    declare: Option<DeclareScript>,
}

// ---- Value 寻位小工具（panic = 语料构造 bug，非被测面行为）----------------

fn node_exec_mut<'a>(plan: &'a mut Value, id: &str) -> &'a mut Value {
    plan["body"]["node_exec"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["node_id"] == json!(id))
        .unwrap_or_else(|| panic!("语料构造：node_exec {id} 应在基线计划"))
}

fn graph_node_mut<'a>(plan: &'a mut Value, id: &str) -> &'a mut Value {
    plan["body"]["graph"]["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["id"] == json!(id))
        .unwrap_or_else(|| panic!("语料构造：graph 节点 {id} 应在基线计划"))
}

fn budget_mut<'a>(plan: &'a mut Value, id: &str) -> &'a mut Value {
    plan["body"]["budget"]["budgets"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["id"] == json!(id))
        .unwrap_or_else(|| panic!("语料构造：预算 {id} 应在基线计划"))
}

fn edges_mut(plan: &mut Value) -> &mut Vec<Value> {
    plan["body"]["graph"]["edges"].as_array_mut().unwrap()
}

fn nested_object_schema(depth: usize) -> Value {
    let mut s = json!({"type": "object"});
    for _ in 0..depth {
        s = json!({"type": "object", "properties": {"p": s}});
    }
    s
}

fn nested_value(depth: usize) -> Value {
    let mut v = json!(1);
    for _ in 0..depth {
        v = json!({"n": v});
    }
    v
}

/// 全语料表（≥40 例；族序 = ask 列举序）。
fn corpus() -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    let mut c = |family: &'static str, name: &'static str, mutate: Mutate, expect: Expect| {
        cases.push(Case {
            family,
            name,
            mutate,
            expect,
            declare: None,
        });
    };

    // ---- 族 1：信封类型不匹配（decode 拒绝式面 → EX4005）------------------
    c(
        "env",
        "v-2",
        Box::new(|p, _, _| {
            p["v"] = json!(2);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "v-string",
        Box::new(|p, _, _| {
            p["v"] = json!("1");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "v-zero",
        Box::new(|p, _, _| {
            p["v"] = json!(0);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "v-missing",
        Box::new(|p, _, _| {
            p.as_object_mut().unwrap().remove("v");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "v-float",
        Box::new(|p, _, _| {
            p["v"] = json!(1.5);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "body-missing",
        Box::new(|p, _, _| {
            p.as_object_mut().unwrap().remove("body");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "unknown-top-field",
        Box::new(|p, _, _| {
            p["extra_field"] = json!(1);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "body-missing-types-block",
        Box::new(|p, _, _| {
            p["body"].as_object_mut().unwrap().remove("types");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "raw-truncated-json",
        Box::new(|_, _, _| Some(r#"{"v": 1, "body":"#.to_string())),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "env",
        "raw-empty-file",
        Box::new(|_, _, _| Some(String::new())),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );

    // ---- 族 2：registry_digest 篡改（E9004/EX4004 唯一正本拒）--------------
    c(
        "digest",
        "flip-one-hex-seeded",
        Box::new(|p, _, rng| {
            let d = p["body"]["compiled_from"]["registry_digest"]
                .as_str()
                .unwrap()
                .to_string();
            let pos = rng.below(d.len() as u64) as usize;
            let mut chars: Vec<char> = d.chars().collect();
            chars[pos] = if chars[pos].is_ascii_digit() {
                'a'
            } else {
                '0'
            };
            p["body"]["compiled_from"]["registry_digest"] =
                json!(chars.into_iter().collect::<String>());
            None
        }),
        Expect::Ex(ErrorCode::PlanDigestMismatch),
    );
    c(
        "digest",
        "empty",
        Box::new(|p, _, _| {
            p["body"]["compiled_from"]["registry_digest"] = json!("");
            None
        }),
        Expect::Ex(ErrorCode::PlanDigestMismatch),
    );
    c(
        "digest",
        "all-zero",
        Box::new(|p, _, _| {
            p["body"]["compiled_from"]["registry_digest"] = json!("0".repeat(64));
            None
        }),
        Expect::Ex(ErrorCode::PlanDigestMismatch),
    );
    c(
        "digest",
        "uppercase",
        Box::new(|p, _, _| {
            let d = p["body"]["compiled_from"]["registry_digest"]
                .as_str()
                .unwrap()
                .to_uppercase();
            p["body"]["compiled_from"]["registry_digest"] = json!(d);
            None
        }),
        Expect::Ex(ErrorCode::PlanDigestMismatch),
    );

    // ---- 族 3：缺节点引用（投影/边/入口/终态发射者）------------------------
    c(
        "ref",
        "node-exec-drop-tester",
        Box::new(|p, _, _| {
            p["body"]["node_exec"]
                .as_array_mut()
                .unwrap()
                .retain(|n| n["node_id"] != json!("tester"));
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "ref",
        "node-exec-typo-id",
        Box::new(|p, _, _| {
            node_exec_mut(p, "planner")["node_id"] = json!("planer");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "ref",
        "graph-drop-planner-node",
        Box::new(|p, _, _| {
            p["body"]["graph"]["nodes"]
                .as_array_mut()
                .unwrap()
                .retain(|n| n["id"] != json!("planner"));
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "ref",
        "edge-to-coder-removed",
        Box::new(|p, _, _| {
            edges_mut(p).retain(|e| !(e["from"] == json!("planner") && e["to"] == json!("coder")));
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );
    c(
        "ref",
        "output-from-ghost",
        Box::new(|p, _, _| {
            p["body"]["interface"]["output_from"] = json!("ghost-node");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "ref",
        "edge-output-port-ghost",
        Box::new(|p, _, _| {
            for e in edges_mut(p).iter_mut() {
                if e["from"] == json!("planner") && e["to"] == json!("coder") {
                    e["output"] = json!("planX");
                }
            }
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "ref",
        "entry-ghost",
        Box::new(|p, _, _| {
            p["body"]["graph"]["entry"] = json!("no-such-entry");
            None
        }),
        Expect::ExAny(&[ErrorCode::GraphShapeUnsupported], "entry"),
    );
    c(
        "ref",
        "check-node-unit-null",
        Box::new(|p, _, _| {
            node_exec_mut(p, "tester")["unit"] = Value::Null;
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );

    // ---- 族 4：预算字段非法（负/零/超/非正典 id/未知维）--------------------
    c(
        "budget",
        "cost-cap-negative",
        Box::new(|p, _, _| {
            budget_mut(p, "cost_cap")["cap"] = json!(-1);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "budget",
        "cost-cap-zero",
        Box::new(|p, _, _| {
            budget_mut(p, "cost_cap")["cap"] = json!(0);
            None
        }),
        Expect::Ex(ErrorCode::BudgetCapExceeded),
    );
    c(
        "budget",
        "token-cap-zero",
        Box::new(|p, _, _| {
            budget_mut(p, "token_cap")["cap"] = json!(0);
            None
        }),
        Expect::Ex(ErrorCode::BudgetCapExceeded),
    );
    c(
        "budget",
        "cost-cap-one-cent",
        Box::new(|p, _, _| {
            budget_mut(p, "cost_cap")["cap"] = json!(1);
            None
        }),
        Expect::Ex(ErrorCode::BudgetCapExceeded),
    );
    c(
        "budget",
        "token-cap-one",
        Box::new(|p, _, _| {
            budget_mut(p, "token_cap")["cap"] = json!(1);
            None
        }),
        Expect::Ex(ErrorCode::BudgetCapExceeded),
    );
    c(
        "budget",
        "dimension-unknown",
        Box::new(|p, _, _| {
            budget_mut(p, "cost_cap")["dimension"] = json!("quantum");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "budget",
        "cap-float",
        Box::new(|p, _, _| {
            budget_mut(p, "cost_cap")["cap"] = json!(600.5);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "budget",
        "cap-huge-legal",
        Box::new(|p, _, _| {
            budget_mut(p, "cost_cap")["cap"] = json!(10_000_000_000_000_000_000u64);
            None
        }),
        Expect::Ok,
    );
    c(
        "budget",
        "turns-cap-2pow32-overflow",
        Box::new(|p, _, _| {
            // 执行器消费面 = node_exec[].budget 投影（graph_unit.rs:401-406），
            // body.budget.budgets 台账同改保持计划自洽。
            node_exec_mut(p, "coder")["budget"][0]["cap"] = json!(4_294_967_296u64);
            budget_mut(p, "coder_turns")["cap"] = json!(4_294_967_296u64);
            None
        }),
        Expect::OkTurns("coder", u32::MAX),
    );
    c(
        "budget",
        "cost-cap-id-typo-unenforced",
        Box::new(|p, _, _| {
            budget_mut(p, "cost_cap")["id"] = json!("cost_caps");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );

    // ---- 族 5：线性链成环 + 悬边端点 --------------------------------------
    c(
        "cycle",
        "back-edge-reviewer-planner",
        Box::new(|p, _, _| {
            edges_mut(p).push(json!({
            "from": "reviewer", "output": "review", "to": "planner", "input": "plan", "checkpoint": false
        }));
            None
        }),
        Expect::ExAny(&[ErrorCode::GraphShapeUnsupported], "环"),
    );
    c(
        "cycle",
        "self-loop-planner",
        Box::new(|p, _, _| {
            edges_mut(p).push(json!({
            "from": "planner", "output": "plan", "to": "planner", "input": "plan", "checkpoint": false
        }));
            None
        }),
        Expect::ExAny(&[ErrorCode::GraphShapeUnsupported], "环"),
    );
    c(
        "cycle",
        "two-cycle-coder-reviewer",
        Box::new(|p, _, _| {
            edges_mut(p).push(json!({
            "from": "reviewer", "output": "review", "to": "coder", "input": "plan", "checkpoint": false
        }));
            None
        }),
        Expect::ExAny(&[ErrorCode::GraphShapeUnsupported], "环"),
    );
    c(
        "cycle",
        "dangling-edge-to-ghost",
        Box::new(|p, _, _| {
            edges_mut(p).push(json!({
            "from": "planner", "output": "plan", "to": "GHOST", "input": "plan", "checkpoint": false
        }));
            None
        }),
        Expect::ExAny(&[ErrorCode::GraphShapeUnsupported], "不在节点集"),
    );
    c(
        "cycle",
        "dangling-edge-from-ghost",
        Box::new(|p, _, _| {
            edges_mut(p).push(json!({
            "from": "GHOST", "output": "plan", "to": "tester", "input": "plan", "checkpoint": false
        }));
            None
        }),
        Expect::ExAny(&[ErrorCode::GraphShapeUnsupported], "不在节点集"),
    );

    // ---- 族 6：空面 -------------------------------------------------------
    c(
        "empty",
        "node-exec-empty",
        Box::new(|p, _, _| {
            p["body"]["node_exec"] = json!([]);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "empty",
        "graph-and-exec-both-empty",
        Box::new(|p, _, _| {
            p["body"]["node_exec"] = json!([]);
            p["body"]["graph"]["nodes"] = json!([]);
            p["body"]["graph"]["edges"] = json!([]);
            None
        }),
        Expect::Ex(ErrorCode::GraphShapeUnsupported),
    );
    c(
        "empty",
        "graph-empty-exec-intact",
        Box::new(|p, _, _| {
            p["body"]["graph"]["nodes"] = json!([]);
            p["body"]["graph"]["edges"] = json!([]);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );

    // ---- 族 7：深嵌套 -----------------------------------------------------
    c(
        "deep",
        "types-schema-200-levels",
        Box::new(|p, _, _| {
            p["body"]["types"]["swe_task_input"]["properties"]["deep"] = nested_object_schema(200);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "deep",
        "named-type-chain-cycle-20",
        Box::new(|p, _, _| {
            for i in 1..=20u32 {
                let next = if i == 20 {
                    "a1".to_string()
                } else {
                    format!("a{}", i + 1)
                };
                p["body"]["types"][format!("a{i}")] = json!({"type": next});
            }
            node_exec_mut(p, "coder")["consumes"]["plan"] = json!("a1");
            None
        }),
        Expect::ExAny(&[ErrorCode::EnvelopeMismatch], "16 层"),
    );
    c(
        "deep",
        "task-json-150-levels",
        Box::new(|_, t, _| {
            t["deep"] = nested_value(150);
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );
    c(
        "deep",
        "schema-30-levels-legal",
        Box::new(|p, _, _| {
            p["body"]["types"]["swe_task_input"]["properties"]["deep"] = nested_object_schema(30);
            None
        }),
        Expect::Ok,
    );

    // ---- 族 8：超长字段 ---------------------------------------------------
    c(
        "long",
        "template-ref-100k-garbage",
        Box::new(|p, _, _| {
            node_exec_mut(p, "planner")["context"][0]["template_ref"] = json!("x".repeat(100_000));
            None
        }),
        Expect::ExAny(
            &[
                ErrorCode::TemplateRefInvalid,
                ErrorCode::TemplateUnregistered,
            ],
            "",
        ),
    );
    c(
        "long",
        "model-route-100k",
        Box::new(|p, _, _| {
            node_exec_mut(p, "planner")["components"]["model_route"] =
                json!(format!("route.{}", "x".repeat(100_000)));
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );
    c(
        "long",
        "unit-id-100k",
        Box::new(|p, _, _| {
            node_exec_mut(p, "tester")["unit"]["unit_id"] =
                json!(format!("pkg.{}", "x".repeat(100_000)));
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );
    c(
        "long",
        "plan-id-1m",
        Box::new(|p, _, _| {
            p["body"]["plan_id"] = json!("p".repeat(1_000_000));
            None
        }),
        Expect::Ok,
    );
    c(
        "long",
        "problem-statement-1m",
        Box::new(|_, t, _| {
            t["problem_statement"] = json!("x".repeat(1_000_000));
            None
        }),
        Expect::Ok,
    );
    c(
        "long",
        "quota-tokens-u64max",
        Box::new(|p, _, _| {
            node_exec_mut(p, "planner")["context"][0]["quota_tokens"] = json!(u64::MAX);
            None
        }),
        Expect::Ok,
    );
    c(
        "long",
        "node-id-100k-consistent-rename",
        Box::new(|p, _, _| {
            let long = "t".repeat(100_000);
            graph_node_mut(p, "tester")["id"] = json!(long.clone());
            for e in edges_mut(p).iter_mut() {
                if e["to"] == json!("tester") {
                    e["to"] = json!(long.clone());
                }
            }
            node_exec_mut(p, "tester")["node_id"] = json!(long.clone());
            p["body"]["interface"]["output_from"] = json!(long);
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    ); // 文件名超 OS 限 → fail-loud 拒（非 crash）

    // ---- 族 9：typed envelope 违例（类型不匹配族）--------------------------
    c(
        "type",
        "consumes-base-not-in-types",
        Box::new(|p, _, _| {
            node_exec_mut(p, "tester")["consumes"]["files"] = json!("list<ghost_type>");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "consumes-wrong-named-type",
        Box::new(|p, _, _| {
            node_exec_mut(p, "tester")["consumes"]["review"] = json!("task_plan");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "emits-list-dropped",
        Box::new(|p, _, _| {
            node_exec_mut(p, "coder")["artifact_contract"][0]["type"] = json!("code_file");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "input-type-ghost",
        Box::new(|p, _, _| {
            p["body"]["interface"]["input_type"] = json!("ghost_input");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "output-type-ghost",
        Box::new(|p, _, _| {
            p["body"]["interface"]["output_type"] = json!("ghost_output");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "input-schema-string-garbage",
        Box::new(|p, _, _| {
            p["body"]["types"]["swe_task_input"] = json!("not-a-schema");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "output-schema-type-nonstring",
        Box::new(|p, _, _| {
            p["body"]["types"]["qa_report"] = json!({"type": 123});
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "port-renamed-in-contract",
        Box::new(|p, _, _| {
            node_exec_mut(p, "planner")["artifact_contract"][0]["name"] = json!("planX");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "artifact-contract-empty",
        Box::new(|p, _, _| {
            node_exec_mut(p, "planner")["artifact_contract"] = json!([]);
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    c(
        "type",
        "emits-expr-malformed-no-gt",
        Box::new(|p, _, _| {
            node_exec_mut(p, "coder")["artifact_contract"][0]["type"] = json!("list<code_file");
            None
        }),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );

    // ---- 族 10：kind / unit 分派面 ----------------------------------------
    c(
        "kind",
        "unknown-kind",
        Box::new(|p, _, _| {
            graph_node_mut(p, "planner")["kind"] = json!("frobnicate");
            None
        }),
        Expect::Ex(ErrorCode::GraphShapeUnsupported),
    );
    c(
        "kind",
        "task-node-as-check-no-unit",
        Box::new(|p, _, _| {
            graph_node_mut(p, "planner")["kind"] = json!("check");
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );
    c(
        "unit",
        "ghost-unit-id",
        Box::new(|p, _, _| {
            node_exec_mut(p, "tester")["unit"]["unit_id"] = json!("pkg.sweng.ghost");
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );
    c(
        "unit",
        "ghost-unit-version",
        Box::new(|p, _, _| {
            node_exec_mut(p, "tester")["unit"]["unit_version"] = json!("999");
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );

    // ---- 族 11：重复投影 + graph_version 漂移 ------------------------------
    c(
        "dup",
        "node-exec-planner-duplicated",
        Box::new(|p, _, _| {
            let dup = node_exec_mut(p, "planner").clone();
            p["body"]["node_exec"].as_array_mut().unwrap().push(dup);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "dup",
        "graph-node-planner-duplicated",
        Box::new(|p, _, _| {
            let dup = graph_node_mut(p, "planner").clone();
            p["body"]["graph"]["nodes"]
                .as_array_mut()
                .unwrap()
                .push(dup);
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "consist",
        "compiled-graph-version-drift",
        Box::new(|p, _, _| {
            p["body"]["compiled_from"]["graph_version"] = json!("9.9");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );
    c(
        "consist",
        "order-graph-version-drift",
        Box::new(|p, _, _| {
            p["body"]["order"]["graph_version"] = json!("9.9");
            None
        }),
        Expect::Ex(ErrorCode::PlanBindMismatch),
    );

    // ---- 族 12：模板 / context 段 -----------------------------------------
    c(
        "tmpl",
        "unregistered-ref",
        Box::new(|p, _, _| {
            node_exec_mut(p, "planner")["context"][0]["template_ref"] = json!("pkg.sweng.ghost@1");
            None
        }),
        Expect::ExAny(
            &[
                ErrorCode::TemplateUnregistered,
                ErrorCode::TemplateRefInvalid,
            ],
            "",
        ),
    );
    c(
        "ctx",
        "unknown-section-from",
        Box::new(|p, _, _| {
            node_exec_mut(p, "planner")["context"][0]["from"] = json!("quantum");
            None
        }),
        Expect::Ex(ErrorCode::UnitFailed),
    );

    // ---- 族 13：declare 契约面（非可推导端口畸形载荷族，2026-10-25 立法批）--
    // F4B/F4B2 判例（变更日志 ㉗）：非可推导端口的裸/畸形 declare 载荷必须
    // fail-loud（EX4006 家族）且不 panic/不静默通过；declare 闭环内拒收后
    // 合法回退源（终态消息 JSON）仍可交付（fumble-rescue 例）。
    let mut cd = |name: &'static str,
                  target: &'static str,
                  args: Value,
                  final_text: Option<String>,
                  expect: Expect| {
        cases.push(Case {
            family: "declare",
            name,
            mutate: Box::new(|_, _, _| None),
            expect,
            declare: Some(DeclareScript {
                target,
                args,
                final_text,
            }),
        });
    };
    // 裸 declare({})（F4B 判例原样）：闭环内拒 + 无回退源 → 跑后 EX4006。
    cd(
        "bare-empty-no-fallback",
        "planner",
        json!({}),
        Some(String::new()),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    // 裸 declare + 终态消息也是空对象 → 端口集空 ≠ 契约 → EX4006。
    cd(
        "bare-empty-empty-final",
        "planner",
        json!({}),
        Some("{}".to_string()),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    // 载荷直付（缺端口键嵌套——模型最常见混淆形）→ EX4006。
    let args = json!({"instance_id": TASK, "steps": [
        {"id": "s1", "action": "locate", "target": "django/db"}
    ]});
    cd(
        "payload-missing-port-key",
        "planner",
        args.clone(),
        Some(args.to_string()),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    // 未知端口键 → EX4006。
    let args = json!({"plan": {"instance_id": TASK, "steps": [
        {"id": "s1", "action": "locate", "target": "django/db"}
    ]}, "ghost": 1});
    cd(
        "unknown-extra-port",
        "planner",
        args.clone(),
        Some(args.to_string()),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    // 端口载荷缺 required 字段（steps）→ EX4006 点名。
    cd(
        "payload-missing-required-field",
        "planner",
        json!({"plan": {"instance_id": "x"}}),
        Some(r#"{"plan": {"instance_id": "x"}}"#.to_string()),
        Expect::ExAny(&[ErrorCode::EnvelopeMismatch], "steps"),
    );
    // steps 项缺 required 字段（plan_step.id，命名类型递归面）→ EX4006 点名。
    let args = json!({"plan": {"instance_id": TASK, "steps": [
        {"action": "locate", "target": "django/db"}
    ]}});
    cd(
        "steps-item-missing-id",
        "planner",
        args.clone(),
        Some(args.to_string()),
        Expect::ExAny(&[ErrorCode::EnvelopeMismatch], "id"),
    );
    // 非对象参数 + 无回退源 → EX4006。
    cd(
        "non-object-args-no-fallback",
        "planner",
        json!("garbage-not-an-object"),
        Some(String::new()),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    // 畸形 declare 被闭环拒收后，合法终态消息回退源仍交付（fumble-rescue：
    // 新契约行为——旧代码 declare 面无条件存储畸形载荷 → EX4006 死亡）。
    cd(
        "fumble-rescued-by-valid-final-text",
        "planner",
        json!({"plan": {"instance_id": "x"}}),
        None,
        Expect::Ok,
    );
    // 合形 declare + 终态消息留空（回退源缺席）→ declare 面单独交付。
    cd(
        "valid-declare-no-final-text",
        "planner",
        json!({"plan": {"instance_id": TASK, "steps": [
            {"id": "s1", "action": "locate", "target": "django/db"}
        ]}}),
        Some(String::new()),
        Expect::Ok,
    );
    // coder（list<code_file>）：非数组载荷 → EX4006。
    let args = json!({"files": {"path": "django/db/models/foo.py", "content_ref": "ws#1"}});
    cd(
        "coder-files-non-array",
        "coder",
        args.clone(),
        Some(args.to_string()),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );
    // reviewer（review_report）：approved 非布尔 → EX4006。
    let args = json!({"review": {"approved": "yes", "issues": []}});
    cd(
        "reviewer-approved-non-boolean",
        "reviewer",
        args.clone(),
        Some(args.to_string()),
        Expect::Ex(ErrorCode::EnvelopeMismatch),
    );

    cases
}

// ============================================================
// 单例驱动 + 裁定记录
// ============================================================

#[derive(Debug, Clone)]
enum Verdict {
    Completed,
    Ex(ErrorCode, String),
    Panic(String),
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 按字符截断（中文文案按字节切片会劈开 UTF-8 边界）。
        let clip = |s: &str| -> String { s.chars().take(72).collect() };
        match self {
            Verdict::Completed => write!(f, "OK(完成)"),
            Verdict::Ex(code, msg) => write!(f, "{code}: {}", clip(msg)),
            Verdict::Panic(msg) => write!(f, "PANIC!! {}", clip(msg)),
        }
    }
}

fn run_case(base_plan: &Value, base_task: &Value, case: &Case) -> (Verdict, Vec<CapturedLaunch>) {
    let root = temp_root(&format!("{}-{}", case.family, case.name));
    let mut plan = base_plan.clone();
    let mut task = base_task.clone();
    let mut rng = Lcg::seeded();
    let raw = (case.mutate)(&mut plan, &mut task, &mut rng);

    let data_root = root.join("data");
    let task_dir = data_root.join(TASK);
    std::fs::create_dir_all(&task_dir).unwrap();
    std::fs::write(
        task_dir.join("plan.json"),
        raw.unwrap_or_else(|| plan.to_string()),
    )
    .unwrap();
    std::fs::write(task_dir.join("task.json"), task.to_string()).unwrap();

    let attempt = root.join("attempt");
    let out = root.join("out");
    std::fs::create_dir_all(&attempt).unwrap();
    std::fs::create_dir_all(&out).unwrap();

    let bridge = Arc::new(FakeBridge::new());
    let runner = Arc::new(match &case.declare {
        Some(script) => GraphFakeRunner::with_declare_script(sop_full_payloads, script.clone()),
        None => GraphFakeRunner::new(sop_full_payloads),
    });
    let unit = GraphExecuteUnit::new(
        GraphUnitSpec::v1(PROFILE, repo_packages()),
        bridge.clone(),
        runner.clone(),
    )
    .expect("图单元装载");

    let sink = FileRecordSink::open(&out).unwrap();
    let ctx = ExecContext {
        order_id: "wo-adv-1",
        stable_id: "sid-adv-1",
        line_id: "line-A",
        out_dir: &out,
        task_id: TASK,
        host_alias: "local",
        data_root: &data_root,
        attempt_dir: &attempt,
        record_sink: &sink,
        trial: false,
    };

    let result = catch_unwind(AssertUnwindSafe(|| unit.execute(&ctx)));
    let verdict = match result {
        Ok(Ok(_)) => Verdict::Completed,
        Ok(Err(err)) => Verdict::Ex(err.code(), err.to_string()),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<非串 panic 载荷>".to_string());
            Verdict::Panic(msg)
        }
    };
    let _ = std::fs::remove_dir_all(&root);
    (verdict, runner.launches())
}

fn verdict_matches(
    case: &Case,
    verdict: &Verdict,
    launches: &[CapturedLaunch],
) -> Result<(), String> {
    match (&case.expect, verdict) {
        (Expect::Ok, Verdict::Completed) => Ok(()),
        (Expect::OkTurns(node, want), Verdict::Completed) => {
            let got = launches
                .iter()
                .find(|l| l.run_id.ends_with(&format!("-{node}")))
                .map(|l| l.max_turns)
                .ok_or_else(|| format!("期待断言节点 {node} 的 launch，但零捕获"))?;
            if got == *want {
                Ok(())
            } else {
                Err(format!(
                    "期待 {node} max_turns={want}（u64→u32 饱和语义），实得 {got}"
                ))
            }
        }
        (Expect::Ex(want), Verdict::Ex(got, _)) if want == got => Ok(()),
        (Expect::ExAny(wants, keyword), Verdict::Ex(got, msg)) => {
            if !wants.contains(got) {
                return Err(format!("期待码 {:?}，实得 {got}", wants));
            }
            if !keyword.is_empty() && !msg.contains(keyword) {
                return Err(format!("期待点名关键字 {keyword:?}，实得文案：{msg}"));
            }
            Ok(())
        }
        (Expect::Ok | Expect::OkTurns(..), Verdict::Ex(code, msg)) => {
            Err(format!("期待正常完成，实得拒收 {code}: {msg}"))
        }
        (Expect::Ok | Expect::OkTurns(..), Verdict::Panic(msg)) => {
            Err(format!("期待正常完成，实得 PANIC（crash 洞）：{msg}"))
        }
        (Expect::Ex(want), Verdict::Ex(got, msg)) => Err(format!(
            "期待拒收 {want}，实得 {got}（拒但错码/文案）：{msg}"
        )),
        (Expect::Ex(want), Verdict::Completed) => {
            Err(format!("期待拒收 {want}，实得静默通过（洞）"))
        }
        (Expect::Ex(want), Verdict::Panic(msg)) => {
            Err(format!("期待拒收 {want}，实得 PANIC（crash 洞）：{msg}"))
        }
        (Expect::ExAny(wants, _), Verdict::Completed) => {
            Err(format!("期待拒收 {:?}，实得静默通过（洞）", wants))
        }
        (Expect::ExAny(wants, _), Verdict::Panic(msg)) => Err(format!(
            "期待拒收 {:?}，实得 PANIC（crash 洞）：{msg}",
            wants
        )),
    }
}

/// 全量对抗语料单测：打印裁定分布表 → 逐一对照期待。任一违例 = 对抗失败
/// （首跑即批量暴露全部洞，不首错短路——分布表始终完整打印）。
#[test]
fn adversarial_plan_corpus_all_rejected_or_wellformed() {
    let base_plan_src = compile_plan(&sop_full_src());
    let base_plan: Value = serde_json::from_str(&base_plan_src).unwrap();
    let base_task = json!({
        "repo": "django/django",
        "instance_id": TASK,
        "problem_statement": "Fix the thing",
        "image": "swebench/sweb.eval.x86_64.django-11099:latest",
        "test_command": "cd /testbed && python -m pytest -q"
    });

    let cases = corpus();
    assert!(
        cases.len() >= 40,
        "语料规模 ≥40（ask 口径），实得 {}",
        cases.len()
    );

    let mut rows: Vec<(String, Verdict, Option<String>)> = Vec::new();
    for case in &cases {
        let (verdict, launches) = run_case(&base_plan, &base_task, case);
        let mismatch = verdict_matches(case, &verdict, &launches).err();
        rows.push((format!("{}/{}", case.family, case.name), verdict, mismatch));
    }

    // ---- 裁定分布表（先全量打印，再断言）--------------------------------
    println!(
        "== 执行器对抗语料裁定分布（seed={SEED}，{} 例）==",
        rows.len()
    );
    let mut current_family = "";
    let mut dist: std::collections::BTreeMap<String, usize> = Default::default();
    for (name, verdict, mismatch) in &rows {
        let family = name.split('/').next().unwrap_or("");
        if family != current_family {
            println!("-- 族 {family}");
            current_family = family;
        }
        let flag = match mismatch {
            Some(_) => "  <-- 违例",
            None => "",
        };
        println!("  {name:<44} => {verdict}{flag}");
        let key = match verdict {
            Verdict::Completed => "OK(完成)".to_string(),
            Verdict::Ex(code, _) => format!("{code}"),
            Verdict::Panic(_) => "PANIC".to_string(),
        };
        *dist.entry(key).or_default() += 1;
    }
    println!("== 裁定码分布：{dist:?} ==");

    let bad: Vec<&str> = rows
        .iter()
        .filter_map(|(name, _, m)| m.as_ref().map(|_| name.as_str()))
        .collect();
    // 逐一登记违例详情（全部收集后再断言——一次跑暴露全部）。
    let details: Vec<String> = rows
        .iter()
        .filter_map(|(name, _, m)| m.as_ref().map(|msg| format!("  [{name}] {msg}")))
        .collect();
    assert!(
        bad.is_empty(),
        "对抗语料违例 {} 例（语料即复现命令：cargo test -p af-execline \
         --test graph_corpus_adv -- --nocapture；seed={SEED}）：\n{}\n== 裁定详情 ==\n{}",
        bad.len(),
        bad.join("\n"),
        details.join("\n"),
    );
}

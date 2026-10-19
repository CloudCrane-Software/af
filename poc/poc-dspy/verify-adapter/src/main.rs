//! DSPy 适配层一次真实往返（Rust 下发 → Python 执行 → Rust 校验）。
//!
//! Rust 是 IR 真相源：定义 IR JSON schema、生成执行任务、调用 Python 薄适配层
//! （adapter_dspy.py，dspy 3.4.1 venv）、对回传 state 做 9 条规则校验；
//! 漂移实验走 dspy 真实改写 API（对照 DSPy delete 静默 no-op / 存档无字段名 / load 按位 zip 的坑）。
//!
//! 子命令：gen | base | chaos-delete-field | chaos-swap-fields | chaos-add-field |
//!         chaos-add-demo | twice | all

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

const PY: &str = "/home/pan-ding/workspace/临时工作区/temp2/af/poc/poc-dspy/.venv/bin/python";
const ADAPTER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/adapter_dspy.py");
const OUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/out");

// ===================== IR / 任务 类型（Rust = schema 真相源） =====================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FieldKind {
    Input,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Field {
    name: String,
    kind: FieldKind,
    #[serde(rename = "type")]
    ty: String,
    desc: String,
    prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignatureIr {
    instructions: String,
    fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Demo {
    inputs: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ModuleKind {
    #[serde(rename = "module")]
    Module,
    Predict,
    ChainOfThought,
}

/// IR 模块树节点：根节点(kind=module)本身不出现在状态键中，子节点路径从根的直接子节点开始。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleNode {
    id: String,
    kind: ModuleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature: Option<SignatureIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    demos: Vec<Demo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    children: Vec<ModuleNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProgramIr {
    ir_version: u32,
    name: String,
    root: ModuleNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecSpec {
    module: String,
    inputs: BTreeMap<String, String>,
    /// 每个 execute 项恰好一次 LM 调用；DummyLM 按序回放，键 = 字段名
    lm_script: Vec<BTreeMap<String, String>>,
    append_result_as_demo: bool,
}

/// 漂移注入：由 Python 适配层在执行后、dump 前用 dspy 真实改写 API 施加，
/// 模拟"Python 侧程序被漂移（优化器/适配层 bug）后回传"。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Chaos {
    DeleteField { module: String, field: String },
    SwapFields { module: String, a: String, b: String },
    AddField { module: String, name: String, kind: FieldKind, desc: String },
    AddDemo { module: String, demo: BTreeMap<String, String>, input_names: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    task_id: String,
    program: ProgramIr,
    execute: Vec<ExecSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chaos: Option<Chaos>,
}

// ===================== Python 回传 envelope（按 DSPy dump_state 实际形状） =====================

#[derive(Debug, Deserialize)]
struct DspyFieldState {
    description: String,
    prefix: String,
}

#[derive(Debug, Deserialize)]
struct DspySigState {
    fields: Vec<DspyFieldState>,
    instructions: String,
}

#[derive(Debug, Deserialize)]
struct DspyModuleState {
    demos: Vec<BTreeMap<String, serde_json::Value>>,
    signature: DspySigState,
}

#[derive(Debug, Deserialize)]
struct ExecOut {
    module: String,
    outputs: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Response {
    task_id: String,
    state: BTreeMap<String, DspyModuleState>,
    exec_outputs: Vec<ExecOut>,
}

// ===================== 基线 IR 与任务 =====================

fn qa_signature() -> SignatureIr {
    SignatureIr {
        instructions: "根据上下文回答问题。".into(),
        fields: vec![
            Field { name: "context".into(), kind: FieldKind::Input, ty: "str".into(), desc: "相关背景资料".into(), prefix: "背景:".into() },
            Field { name: "question".into(), kind: FieldKind::Input, ty: "str".into(), desc: "用户的问题".into(), prefix: "问题:".into() },
            Field { name: "answer".into(), kind: FieldKind::Output, ty: "str".into(), desc: "一句话答案".into(), prefix: "答案:".into() },
        ],
    }
}

fn base_program() -> ProgramIr {
    ProgramIr {
        ir_version: 1,
        name: "qa_program".into(),
        root: ModuleNode {
            id: "prog".into(),
            kind: ModuleKind::Module,
            signature: None,
            demos: vec![],
            children: vec![
                ModuleNode {
                    id: "answerer".into(),
                    kind: ModuleKind::Predict,
                    signature: Some(qa_signature()),
                    demos: vec![Demo {
                        inputs: BTreeMap::from([("context".into(), "cA".into()), ("question".into(), "qA".into())]),
                        outputs: BTreeMap::from([("answer".into(), "aA".into())]),
                    }],
                    children: vec![],
                },
                ModuleNode {
                    id: "solver".into(),
                    kind: ModuleKind::ChainOfThought,
                    signature: Some(qa_signature()),
                    demos: vec![Demo {
                        inputs: BTreeMap::from([("context".into(), "cB".into()), ("question".into(), "qB".into())]),
                        outputs: BTreeMap::from([("reasoning".into(), "rB".into()), ("answer".into(), "aB".into())]),
                    }],
                    children: vec![],
                },
            ],
        },
    }
}

fn base_task(chaos: Option<Chaos>, task_id: &str) -> Task {
    Task {
        task_id: task_id.into(),
        program: base_program(),
        execute: vec![
            ExecSpec {
                module: "answerer".into(),
                inputs: BTreeMap::from([("context".into(), "C1".into()), ("question".into(), "Q1".into())]),
                lm_script: vec![BTreeMap::from([("answer".into(), "A-EXEC-1".into())])],
                append_result_as_demo: true,
            },
            ExecSpec {
                module: "solver".into(),
                inputs: BTreeMap::from([("context".into(), "C2".into()), ("question".into(), "Q2".into())]),
                lm_script: vec![BTreeMap::from([
                    ("reasoning".into(), "R-EXEC-2".into()),
                    ("answer".into(), "A-EXEC-2".into()),
                ])],
                append_result_as_demo: false,
            },
        ],
        chaos,
    }
}

// ===================== 期望模型（Rust 侧由 IR 推导，含 dspy 派生规则） =====================

#[derive(Debug, Clone)]
struct ExpField {
    name: String,
    prefix: String,
    desc: String,
}

#[derive(Debug)]
struct ExpModule {
    path: String,
    state_key: String, // ChainOfThought 的状态键 = 路径 + ".predict"
    instructions: String,
    fields: Vec<ExpField>, // 生效字段（CoT 含派生 reasoning）
    output_names: Vec<String>,
    base_demos: usize,
    appended_demos: usize,
}

/// dspy 派生规则：ChainOfThought 在最后一个 output 字段之前内建插入
/// reasoning（prefix "Reasoning:"、desc "${reasoning}"）——已在本会话实测确认。
fn eff_fields(kind: ModuleKind, sig: &SignatureIr) -> Vec<ExpField> {
    let mut v: Vec<ExpField> = sig
        .fields
        .iter()
        .map(|f| ExpField { name: f.name.clone(), prefix: f.prefix.clone(), desc: f.desc.clone() })
        .collect();
    if kind == ModuleKind::ChainOfThought {
        let pos = sig.fields.iter().rposition(|f| f.kind == FieldKind::Output).expect("CoT 至少要有一个 output 字段");
        v.insert(
            pos,
            ExpField { name: "reasoning".into(), prefix: "Reasoning:".into(), desc: "${reasoning}".into() },
        );
    }
    v
}

fn walk(node: &ModuleNode, prefix: &str, task: &Task, out: &mut Vec<ExpModule>) {
    for ch in &node.children {
        let path = if prefix.is_empty() { ch.id.clone() } else { format!("{prefix}.{}", ch.id) };
        match ch.kind {
            ModuleKind::Module => walk(ch, &path, task, out),
            ModuleKind::Predict | ModuleKind::ChainOfThought => {
                let sig = ch.signature.as_ref().unwrap_or_else(|| panic!("模块 {path} 缺 signature"));
                let mut output_names: Vec<String> =
                    sig.fields.iter().filter(|f| f.kind == FieldKind::Output).map(|f| f.name.clone()).collect();
                if ch.kind == ModuleKind::ChainOfThought {
                    let p = output_names.len() - 1;
                    output_names.insert(p, "reasoning".into());
                }
                let state_key = if ch.kind == ModuleKind::ChainOfThought { format!("{path}.predict") } else { path.clone() };
                out.push(ExpModule {
                    appended_demos: task
                        .execute
                        .iter()
                        .filter(|e| e.module == path && e.append_result_as_demo)
                        .count(),
                    path,
                    state_key,
                    instructions: sig.instructions.clone(),
                    fields: eff_fields(ch.kind, sig),
                    output_names,
                    base_demos: ch.demos.len(),
                });
            }
        }
    }
}

// ===================== 校验器（Rust 侧 9 条规则） =====================

struct RuleOut {
    id: &'static str,
    ok: bool,
    detail: String,
}

fn set_of(demo: &BTreeMap<String, serde_json::Value>) -> BTreeSet<&str> {
    demo.keys().map(|k| k.as_str()).collect()
}

fn val_str(v: &serde_json::Value) -> String {
    v.as_str().unwrap_or_else(|| panic!("demo 值非字符串: {v}")).to_string()
}

fn validate(task: &Task, resp: &Response) -> Vec<RuleOut> {
    let mut rules = vec![];

    // R0: envelope 回显
    rules.push(RuleOut {
        id: "R0_envelope",
        ok: resp.task_id == task.task_id,
        detail: format!("task_id 回显 {:?}（下发 {:?}）", resp.task_id, task.task_id),
    });

    let mut exp = vec![];
    walk(&task.program.root, "", task, &mut exp);

    // R1: 模块集合恰等（不多不少）
    let exp_keys: BTreeSet<&str> = exp.iter().map(|m| m.state_key.as_str()).collect();
    let got_keys: BTreeSet<&str> = resp.state.keys().map(|s| s.as_str()).collect();
    let missing: Vec<_> = exp_keys.difference(&got_keys).collect();
    let unknown: Vec<_> = got_keys.difference(&exp_keys).collect();
    rules.push(RuleOut {
        id: "R1_module_set",
        ok: missing.is_empty() && unknown.is_empty(),
        detail: format!(
            "期望 {} 个模块 {:?}；缺失 {:?}，未知 {:?}",
            exp_keys.len(),
            Vec::from_iter(exp_keys.iter().copied()),
            missing,
            unknown
        ),
    });

    // R2/R3/R4/R5/R6/R9：逐模块
    let mut r2 = RuleOut { id: "R2_field_count", ok: true, detail: String::new() };
    let mut r3 = RuleOut { id: "R3_field_position", ok: true, detail: String::new() };
    let mut r4 = RuleOut { id: "R4_unknown_field", ok: true, detail: String::new() };
    let mut r5 = RuleOut { id: "R5_demo_count", ok: true, detail: String::new() };
    let mut r6 = RuleOut { id: "R6_demo_keys", ok: true, detail: String::new() };
    let mut r9 = RuleOut { id: "R9_instructions", ok: true, detail: String::new() };

    for m in &exp {
        let Some(st) = resp.state.get(&m.state_key) else { continue };

        if st.signature.instructions != m.instructions {
            r9.ok = false;
            r9.detail.push_str(&format!(
                "[{}] instructions {:?} ≠ 期望 {:?}；",
                m.path, st.signature.instructions, m.instructions
            ));
        }

        if st.signature.fields.len() != m.fields.len() {
            r2.ok = false;
            let got: Vec<_> = st.signature.fields.iter().map(|f| f.prefix.as_str()).collect();
            let want: Vec<_> = m.fields.iter().map(|f| f.prefix.as_str()).collect();
            let missing_f: Vec<_> = want.iter().filter(|p| !got.contains(p)).collect();
            r2.detail.push_str(&format!(
                "[{}] 字段数 {} ≠ 期望 {}（实得{got:?} 期望{want:?} 缺{missing_f:?}）；",
                m.path,
                st.signature.fields.len(),
                m.fields.len(),
                got = got,
                want = want,
                missing_f = missing_f
            ));
        }

        // 位置对齐：DSPy 存档无字段名，字段名只能靠 Rust IR + 位置 + prefix/description 映射恢复
        for (i, (got, want)) in st.signature.fields.iter().zip(&m.fields).enumerate() {
            if got.prefix != want.prefix {
                r3.ok = false;
                r3.detail.push_str(&format!(
                    "[{}] 位置{i} prefix {:?} ≠ 期望 {:?}(字段 {:?})；",
                    m.path, got.prefix, want.prefix, want.name
                ));
            }
            if got.description != want.desc {
                r3.ok = false;
                r3.detail.push_str(&format!(
                    "[{}] 位置{i} description {:?} ≠ 期望 {:?}(字段 {:?})；",
                    m.path, got.description, want.desc, want.name
                ));
            }
        }

        // 无未知字段：回传 prefix 集合 超出 期望集合 的部分
        let want_prefixes: BTreeSet<_> = m.fields.iter().map(|f| f.prefix.as_str()).collect();
        let extra: Vec<_> = st
            .signature
            .fields
            .iter()
            .map(|f| f.prefix.as_str())
            .filter(|p| !want_prefixes.contains(p))
            .collect();
        if !extra.is_empty() {
            r4.ok = false;
            r4.detail.push_str(&format!("[{}] 未知字段 prefix {extra:?}；", m.path));
        }

        // demos 数量一致
        let want_demos = m.base_demos + m.appended_demos;
        if st.demos.len() != want_demos {
            r5.ok = false;
            r5.detail.push_str(&format!(
                "[{}] demos {} ≠ 期望 {}（IR {} + 执行追加 {}）；",
                m.path,
                st.demos.len(),
                want_demos,
                m.base_demos,
                m.appended_demos
            ));
        }

        // demos 键 = 生效字段名集合（demo 是字段名唯一能流回 Rust 的通道）
        let want_names: BTreeSet<_> = m.fields.iter().map(|f| f.name.as_str()).collect();
        for (i, d) in st.demos.iter().enumerate() {
            let ks = set_of(d);
            if ks != want_names {
                r6.ok = false;
                let miss: Vec<_> = want_names.difference(&ks).collect();
                let unk: Vec<_> = ks.difference(&want_names).collect();
                r6.detail.push_str(&format!(
                    "[{}] demo#{i} 键 ≠ 字段名集（缺{miss:?} 多{unk:?}）；",
                    m.path
                ));
            }
        }
    }
    rules.push(r2);
    rules.push(r3);
    rules.push(r4);
    rules.push(r5);
    rules.push(r6);
    rules.push(r9);

    // R7: 执行输出 == 下发 lm_script 的确定性预期（键=字段名、值逐字相等）
    let mut r7ok = resp.exec_outputs.len() == task.execute.len();
    let mut r7d = format!(
        "exec_outputs {} 项 vs 下发 {} 项；",
        resp.exec_outputs.len(),
        task.execute.len()
    );
    for (i, (spec, out)) in task.execute.iter().zip(&resp.exec_outputs).enumerate() {
        if out.module != spec.module {
            r7ok = false;
            r7d.push_str(&format!("第{i}项模块 {:?} ≠ {:?}；", out.module, spec.module));
        }
        let expected = &spec.lm_script[0];
        if out.outputs != *expected {
            r7ok = false;
            r7d.push_str(&format!(
                "[{}] 输出 {:?} ≠ lm_script 预期 {expected:?}；",
                out.module, out.outputs
            ));
        }
        let want_out: BTreeSet<_> = {
            let m = exp.iter().find(|m| m.path == spec.module).expect("exec 模块不在 IR");
            m.output_names.iter().map(|s| s.as_str()).collect()
        };
        let got_out: BTreeSet<_> = out.outputs.keys().map(|s| s.as_str()).collect();
        if got_out != want_out {
            r7ok = false;
            r7d.push_str(&format!("[{}] 输出键 {got_out:?} ≠ 生效输出字段 {want_out:?}；", out.module));
        }
    }
    rules.push(RuleOut { id: "R7_exec_outputs", ok: r7ok, detail: r7d });

    // R8: append_result_as_demo ⇒ 执行产物 (inputs ∪ outputs) 必须出现在该模块 demos 中
    let mut r8 = RuleOut { id: "R8_appended_demo", ok: true, detail: String::new() };
    for spec in &task.execute {
        if !spec.append_result_as_demo {
            continue;
        }
        let m = exp.iter().find(|m| m.path == spec.module).unwrap();
        let mut want: BTreeMap<String, String> = spec.inputs.clone();
        for (k, v) in &spec.lm_script[0] {
            want.insert(k.clone(), v.clone());
        }
        let Some(st) = resp.state.get(&m.state_key) else { continue };
        let found = st.demos.iter().any(|d| {
            d.len() == want.len() && d.iter().all(|(k, v)| want.get(k).map(|w| w == &val_str(v)).unwrap_or(false))
        });
        if !found {
            r8.ok = false;
            r8.detail.push_str(&format!("[{}] 执行产物 demo {want:?} 未出现在回传 demos 中；", m.path));
        }
    }
    rules.push(r8);

    rules
}

// ===================== 运行器 =====================

fn run_python(task_path: &str) -> Result<(Vec<u8>, Response), String> {
    let out = Command::new(PY).arg(ADAPTER).arg(task_path).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "python 退出码 {:?}\n--stderr--\n{}\n--stdout--\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        ));
    }
    let resp: Response = serde_json::from_slice(&out.stdout).map_err(|e| {
        format!(
            "响应 JSON 解析失败: {e}\nstdout 前 500 字节: {}",
            String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(500)])
        )
    })?;
    Ok((out.stdout, resp))
}

fn write_task(name: &str, task: &Task) -> String {
    let p = format!("{OUT}/task_{name}.json");
    fs::write(&p, serde_json::to_string_pretty(task).unwrap()).unwrap();
    p
}

fn print_rules(rules: &[RuleOut]) {
    for r in rules {
        if r.ok {
            println!("  [ ok ] {}", r.id);
        } else {
            println!("  [CATCH] {} | {}", r.id, r.detail);
        }
    }
}

enum Expect {
    Valid,
    Invalid(&'static str), // 必须被抓住的主规则 id
}

fn case(name: &str, chaos: Option<Chaos>, expect: Expect) -> bool {
    println!("== case {name} ==");
    let task = base_task(chaos, &format!("roundtrip-{name}"));
    let path = write_task(name, &task);
    let (raw, resp) = match run_python(&path) {
        Ok(x) => x,
        Err(e) => {
            println!("  [FAIL] 适配层调用失败:\n{e}");
            return false;
        }
    };
    let rules = validate(&task, &resp);
    print_rules(&rules);
    let fired: Vec<_> = rules.iter().filter(|r| !r.ok).map(|r| r.id).collect();
    let ok = match expect {
        Expect::Valid => {
            let ok = fired.is_empty();
            println!("  VERDICT: {}（期望 VALID）", if ok { "VALID" } else { "INVALID" });
            ok
        }
        Expect::Invalid(primary) => {
            let caught = fired.contains(&primary);
            println!(
                "  VERDICT: {}（期望 INVALID 且抓到 {primary}）fired={fired:?}",
                if !fired.is_empty() { "INVALID" } else { "VALID" }
            );
            caught
        }
    };
    println!("  raw 响应 {} 字节 → out/resp_{name}.json", raw.len());
    fs::write(format!("{OUT}/resp_{name}.json"), &raw).unwrap();
    println!("  case {name}: {}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn twice() -> bool {
    println!("== case twice（同一 IR 两次往返，字节级比对） ==");
    let task = base_task(None, "roundtrip-twice");
    let path = write_task("twice", &task);
    let (b1, _) = match run_python(&path) {
        Ok(x) => x,
        Err(e) => {
            println!("  [FAIL] 第一次往返失败:\n{e}");
            return false;
        }
    };
    let (b2, _) = match run_python(&path) {
        Ok(x) => x,
        Err(e) => {
            println!("  [FAIL] 第二次往返失败:\n{e}");
            return false;
        }
    };
    fs::write(format!("{OUT}/roundtrip_1.json"), &b1).unwrap();
    fs::write(format!("{OUT}/roundtrip_2.json"), &b2).unwrap();
    let eq = b1 == b2;
    println!("  第 1 次响应 {} 字节；第 2 次响应 {} 字节；字节级相等 = {eq}", b1.len(), b2.len());
    println!("  case twice: {}", if eq { "PASS" } else { "FAIL" });
    eq
}

fn gen() {
    let ir = base_program();
    fs::write(format!("{OUT}/ir_base.json"), serde_json::to_string_pretty(&ir).unwrap()).unwrap();
    let task = base_task(None, "roundtrip-base");
    fs::write(format!("{OUT}/task_base.json"), serde_json::to_string_pretty(&task).unwrap()).unwrap();
    println!("写出 {OUT}/ir_base.json 与 {OUT}/task_base.json");
}

fn main() {
    fs::create_dir_all(OUT).unwrap();
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let mut results: Vec<(&str, bool)> = vec![];
    match cmd.as_str() {
        "gen" => gen(),
        "base" => results.push(("base", case("base", None, Expect::Valid))),
        "chaos-delete-field" => results.push((
            "chaos-delete-field",
            case(
                "chaos-delete-field",
                Some(Chaos::DeleteField { module: "answerer".into(), field: "answer".into() }),
                Expect::Invalid("R2_field_count"),
            ),
        )),
        "chaos-swap-fields" => results.push((
            "chaos-swap-fields",
            case(
                "chaos-swap-fields",
                Some(Chaos::SwapFields { module: "answerer".into(), a: "context".into(), b: "question".into() }),
                Expect::Invalid("R3_field_position"),
            ),
        )),
        "chaos-add-field" => results.push((
            "chaos-add-field",
            case(
                "chaos-add-field",
                Some(Chaos::AddField {
                    module: "answerer".into(),
                    name: "bonus".into(),
                    kind: FieldKind::Output,
                    desc: "越权字段".into(),
                }),
                Expect::Invalid("R4_unknown_field"),
            ),
        )),
        "chaos-add-demo" => results.push((
            "chaos-add-demo",
            case(
                "chaos-add-demo",
                Some(Chaos::AddDemo {
                    module: "answerer".into(),
                    demo: BTreeMap::from([
                        ("context".to_string(), "cx".into()),
                        ("question".to_string(), "qx".into()),
                        ("answer".to_string(), "ax".into()),
                    ]),
                    input_names: vec!["context".into(), "question".into()],
                }),
                Expect::Invalid("R5_demo_count"),
            ),
        )),
        "twice" => results.push(("twice", twice())),
        "all" => {
            gen();
            results.push(("base", case("base", None, Expect::Valid)));
            results.push((
                "chaos-delete-field",
                case(
                    "chaos-delete-field",
                    Some(Chaos::DeleteField { module: "answerer".into(), field: "answer".into() }),
                    Expect::Invalid("R2_field_count"),
                ),
            ));
            results.push((
                "chaos-swap-fields",
                case(
                    "chaos-swap-fields",
                    Some(Chaos::SwapFields { module: "answerer".into(), a: "context".into(), b: "question".into() }),
                    Expect::Invalid("R3_field_position"),
                ),
            ));
            results.push((
                "chaos-add-field",
                case(
                    "chaos-add-field",
                    Some(Chaos::AddField {
                        module: "answerer".into(),
                        name: "bonus".into(),
                        kind: FieldKind::Output,
                        desc: "越权字段".into(),
                    }),
                    Expect::Invalid("R4_unknown_field"),
                ),
            ));
            results.push((
                "chaos-add-demo",
                case(
                    "chaos-add-demo",
                    Some(Chaos::AddDemo {
                        module: "answerer".into(),
                        demo: BTreeMap::from([
                            ("context".to_string(), "cx".into()),
                            ("question".to_string(), "qx".into()),
                            ("answer".to_string(), "ax".into()),
                        ]),
                        input_names: vec!["context".into(), "question".into()],
                    }),
                    Expect::Invalid("R5_demo_count"),
                ),
            ));
            results.push(("twice", twice()));
        }
        other => {
            eprintln!("未知子命令: {other}");
            std::process::exit(2);
        }
    }
    if !results.is_empty() {
        println!("\n==== 汇总 ====");
        for (n, ok) in &results {
            println!("{} {n}", if *ok { "PASS" } else { "FAIL" });
        }
        let npass = results.iter().filter(|(_, ok)| *ok).count();
        println!("PASS {}/总 {}", npass, results.len());
        if npass != results.len() {
            std::process::exit(1);
        }
        println!("ALL CASES PASSED");
    }
}

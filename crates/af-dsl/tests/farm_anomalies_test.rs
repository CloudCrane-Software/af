//! 农田异常样本回归（FV-1 第一波裁决 §三；语料 = `testdata/farm-anomalies/`
//! 36 份，逐样本根因/裁定见同目录 ANOMALIES.md 副本）。
//!
//! 期望裁定按裁决书 §三 的修复分项组织；本文件随修复逐项增长（TDD：每项
//! 先 RED 后 GREEN）。文档化边界样本（B-5/B-6 类，裁决「入档不立案」）钉
//! 现行为不变，防修复批误伤。

use af_dsl::{load_str, validate, Report};
use std::path::PathBuf;

/// 装载并校验一份农田样本，返回 Report（load 失败即 panic——这些样本都
/// 是信封/形状合法、语义层或崩溃面的案子）。
fn farm(name: &str) -> Report {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/farm-anomalies")
        .join(name);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc = load_str(&src).unwrap_or_else(|e| panic!("load_str {name}: {e}"));
    validate(&doc)
}

fn codes(r: &Report) -> Vec<&'static str> {
    r.codes()
}

/// 深嵌套过闸样本的判定须在大栈线程上跑：antlr4rust 递归下降每层括号吃
/// 数 KB 栈（农田实测 8MB 主栈深 1000 过/2000 溢出），测试线程栈不定，
/// 32MB 显式栈把「过闸即真过」的判定与环境解耦（RESULT-农田修复 §阈值）。
fn run_on_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("spawn 256MB-stack thread")
        .join()
        .expect("big-stack thread panicked")
}

// ---------- P0-PANIC：nodes:[] 不得 panic，应报 E2001（裁决 §三-P0①） ----------

/// B60（`nodes: []` + 其余合法最小文档）：validate 契约「违例一次收齐」
/// （lib.rs `validate(&Document) -> Report`）——零节点时 `meta.entry`/`meta.
/// output.from` 悬空应各报一条 E2001，而非 `refs.rs` `scopes["root"]` 索引
/// panic（ANOMALIES.md A-1：`no entry found for key`）。
#[test]
fn b60_empty_nodes_reports_e2001_not_panic() {
    let report = farm("B60_nodes_empty.yaml");
    assert_eq!(
        codes(&report),
        vec!["E2001", "E2001"],
        "零节点文档：entry 与 output.from 双悬空各一条 E2001，一次收齐：\n{report}"
    );
}

// ---------- P0-CEL：深嵌套/超长 CEL 前置闸（裁决 §三-P0②） ----------

/// 4 份 CRASH 样本（ANOMALIES.md A-2：括号深 2000/100000、20 万段加链、
/// 90 万段加链——antlr4rust 栈溢出 SIGABRT，`catch_unwind` 兜不住）必须
/// 转正规错误码 E1054（表达式段 E1xxx 前置闸：嵌套深度/字节长双闸），
/// 不再 abort 进程。
#[test]
fn cel_crashers_report_e1054_not_sigabrt() {
    for name in [
        "C_CE15_parens_2000.yaml",
        "C_CE16_parens_100000.yaml",
        "C_CE19_plus_chain_200k.yaml",
        "B89_cel_900kb_chain.yaml",
    ] {
        let report = farm(name);
        assert_eq!(
            codes(&report),
            vec!["E1054"],
            "{name} 应以 E1054 前置闸拒绝（深/长超限），而非栈溢出 abort：\n{report}"
        );
    }
}

/// 阈值回归钉（ANOMALIES.md A-2 实测：深 500/深 1000 优雅 PASS）：前置闸
/// 不得误伤过闸样本——在 256MB 大栈线程上完整走 parse→判定。
#[test]
fn cel_threshold_passers_still_pass() {
    for name in ["C_CE28_parens_500.yaml", "C_CE29_parens_1000.yaml"] {
        let ok = run_on_big_stack(|| {
            let report = farm(name);
            (report.is_ok(), format!("{report}"))
        });
        assert!(ok.0, "{name} 深 500/1000 在阈值内，应保持 PASS：\n{}", ok.1);
    }
}


// ---------- P1-REF-SHORTCIRCUIT：output.ref 未登记类型名不得静默（裁决 §三-③） ----------

/// M001（最小化探针：output.ref = "Ghost"，types 表无此名）：refs.rs 对
/// `!type_resolves(output.ref)` 静默 return——全链路无一处核对 output.ref
/// 类型名（E2205 只对 meta.input/stores/consumes/emits 上报），零违例过审
/// （ANOMALIES.md B-2）。应报既有码 E2205 并维持 E2202 抑制（比较对象不可信
/// 不重复定罪）。
#[test]
fn m001_output_ref_unregistered_reports_e2205() {
    let report = farm("M001_output_ref_unregistered_min.yaml");
    assert_eq!(
        codes(&report),
        vec!["E2205"],
        "output.ref 未登记类型名：报 E2205，E2202 因比较对象不可信抑制：\n{report}"
    );
}

/// D_209（爆破命中样本，output.ref = "map" 未登记）：同根因的原始样本，
/// E2205 必须在场（其余违例按文档实际一次收齐）。
#[test]
fn d209_blast_reports_e2205() {
    let report = farm("D_209_blast.yaml");
    assert!(
        codes(&report).contains(&"E2205"),
        "D_209 output.ref=\"map\" 未登记，E2205 必须在场：\n{report}"
    );
}

// ---------- P1-HAS-MACRO：has() 宏不得逃过 E1052 白名单（裁决 §三-④） ----------

/// C_CE04（policies.stop[].when: "has(input.q)"）：cel-parser 0.10.1 将
/// has() 宏展开为 Select{test:true}——AST 不登记函数名，references().
/// functions() 为空，E1052 白名单面拦不住（ANOMALIES.md B-3，文档-代码
/// 分歧：cel.rs 注记「宏不在保守子集内」但宏面无检查）。词法补位后以
/// E1052 拒（码位不变，宏即非保守子集）。
#[test]
fn ce04_has_macro_rejected_as_e1052() {
    let report = farm("C_CE04_has_macro.yaml");
    assert_eq!(
        codes(&report),
        vec!["E1052"],
        "has() 宏应按非保守子集拒（E1052）：\n{report}"
    );
}

// ---------- P1-SCOPE-LEAK：failures.route 根图限定（裁决 §三-⑤ route 半项） ----------

/// C_AL08（failures[].route: "c.b"——c 的 loop 体内节点路径）：E2204 判定
/// 用 ix.nodes.contains_key（含全部体内路径键），失败重路由可指向体内深层
/// 节点（ANOMALIES.md B-4）。路由目标从根图 policies 发出，须止步于根图
/// 节点（关键词 same_node/next_iteration 或根图节点 id）。
#[test]
fn al08_failures_route_into_body_rejected_as_e2204() {
    let report = farm("C_AL08_failures_route_to_body_node.yaml");
    assert_eq!(
        codes(&report),
        vec!["E2204"],
        "failures.route 指向体内节点：按根图限定拒（E2204）：\n{report}"
    );
}

// ---------- P1-ALIAS：点号 id 词法拒 E1008 + entry/output.from 口径统一（裁决 §三-⑥） ----------

/// 路径别名族（ANOMALIES.md B-1）：节点 id 无字符集限制，「根节点 a.b」与
/// 「a 的 loop 体节点 b」点号路径同形 → 语义索引 BTreeMap 键覆盖无侦测，
/// 单文档致盲 INV-2 多写者/E2312/E2321 自环/E2331 越权读者/E2322 安全点
/// 五防线（六份 PASS 样本）。提案-B 文法中点号始终是路径分隔符（§1.2
/// writer「节点 id **或**点号路径」两类），id 段本不含点号——词法层 E1008
/// 拒绝即同时解漏网与误杀，0 语法演进。
#[test]
fn alias_family_dotted_ids_rejected_as_e1008() {
    for name in [
        "C_AL01_writer_alias_root_over_body.yaml",
        "C_AL02_writer_alias_body_wins.yaml",
        "C_AL03_map_writer_hidden.yaml",
        "C_AL06_verifier_selfloop_hidden.yaml",
        "C_AL07_eval_reader_hidden.yaml",
        "C_AL13_safetypoint_fake_pass.yaml",
    ] {
        let report = farm(name);
        assert!(
            codes(&report).contains(&"E1008"),
            "{name} 点号 id 应词法拒（E1008），不得因索引别名静默过审：\n{report}"
        );
    }
}

/// 误杀 4 份（ANOMALIES.md C 类）：点号 id 节点被 entry 精确 id / output.from
/// 点号分段双口径分裂（C_AL04 E2001）、安全点/from_body 命中双胞胎
/// （C_AL12 E2322 / C_AL14 E2106）、CEL 词表整名登记 vs 分段解析根
/// （C_AL10 E2203）——同根因另一面。词法拒后四份均以 E1008 定罪，幽灵
/// 语义误杀随之消解（不再是拒绝理由）。
#[test]
fn alias_false_kills_now_reject_for_e1008() {
    for name in [
        "C_AL04_entry_output_asym.yaml",
        "C_AL10_cel_dotted_id_unreachable.yaml",
        "C_AL12_safetypoint_wrong_target_fk.yaml",
        "C_AL14_frombody_shadow_fk.yaml",
    ] {
        let report = farm(name);
        assert!(
            codes(&report).contains(&"E1008"),
            "{name} 应以 E1008（点号 id）定罪，而非幽灵语义误杀：\n{report}"
        );
    }
}

/// 口径分裂消亡钉：entry=根图精确 id；output.from=根图 id 或点号体内
/// 路径（inv1_test 既有契约，n1.lp.leaf 形）。两口径的分裂面（同一节点
/// 可作入口、不可作输出来源）只在**点号 id** 场景成立——E1008 词法拒后
/// 合法文档 id 无点号，单段 from 与 entry 同命中、多段 from 是真体内
/// 路径，分裂不再可达（农田 C_AL04 的幽灵分裂随 E1008 消解）。
#[test]
fn entry_and_output_from_calibers_cannot_diverge_post_e1008() {
    // 体内路径 from: a.b 合法（既定契约照旧）；同一字符串作 entry 则按
    // 根图精确 id 判 E2001——两口径各自稳定，且点号 id 已不可声明。
    let base = "\
version: 1
meta:
  name: caliber
  graph_version: '1'
  entry: n1
  input: T_in
  output: { ref: T_out, from: a.b }
types:
- name: T_in
  schema: { type: object, properties: { q: { type: string } } }
- name: T_out
  schema: { type: object, properties: { r: { type: string } } }
nodes:
- id: n1
  kind: task
  uses: { model: m@1 }
  emits: { out: T_out }
  context:
    sections: [{ from: input, fields: [q], quota_tokens: 10 }]
- id: a
  kind: task
  loop:
    body:
      nodes:
      - id: b
        kind: check
        emits: { o: T_out }
        uses: { unit: g@1 }
      edges: []
    max_iterations: 1
    safety_points: [{ node: b, kind: check }]
edges: []
policies: {}
";
    let doc = af_dsl::load_str(base).unwrap();
    assert!(
        validate(&doc).is_ok(),
        "from: a.b（a 的体内 check 节点）按既定路径契约合法：\n{}",
        validate(&doc)
    );

    // entry 换成同字符串：根图精确 id 无 "a.b" → E2001（entry 只认根图）。
    let src = base.replace("entry: n1", "entry: a.b");
    let doc = af_dsl::load_str(&src).unwrap();
    let report = validate(&doc);
    assert_eq!(
        codes(&report),
        vec!["E2001"],
        "entry 只认根图节点 id：\n{report}"
    );
}

// ---------- 文档化边界回归钉（裁决 §三：「5 项文档化边界入档不立案」+ B-5 复核存档） ----------

/// 裁决明确不改码的样本钉现裁定（PASS），防本修复批误伤：零写者不报
/// （inv2 注记）、间接自验证 v0.1 不查（inv3 注记）、求值错误不判（cel
/// 注记）、阈值/口径类留人工裁决（B-5）。修复批只动裁决点名的六项。
#[test]
fn documented_boundaries_stay_pass() {
    for name in [
        "B10_type_schema_str.yaml",
        "B23_ops_param_unclosed.yaml",
        "B50_retention_zero.yaml",
        "B69_zero_writer.yaml",
        "C_MW03_zero_writer.yaml",
        "C_IV35_indirect_selfverify_documented.yaml",
        "C_CE10_steps_badfield.yaml",
        "C_CE21_div_zero.yaml",
        "C_CE02_list_literal.yaml",
        "C_CE03_map_literal.yaml",
        "C_CX20_two_edges_one_section.yaml",
        "C_IV48_writer_port_forward_leak.yaml",
        "C_IV49_evaluator_port_forward_leak.yaml",
        "C_RT06_exit_named_node.yaml",
    ] {
        let report = farm(name);
        assert!(
            report.is_ok(),
            "{name} 属文档化边界/复核存档（不在修复面），应保持 PASS：\n{report}"
        );
    }
}

// ---------- C_AL09 语义钉死（编排者裁决 2026-10-25：农田误报，FALSE-POSITIVE-SEMANTIC） ----------

/// 裁决要点：预算 scope 允许点号路径穿体（「构造节点.体内节点」）——
/// G3 放行裁决锁定的过稿语义（11/15 对齐稿 14 处在用：ADAS
/// node:mas_loop.meta_agent、AFlow map:search.exec_map 等），体内节点按迭代
/// 计费是预算面核心用途；提案-B §1.2 budgets.scope `<id>` 记法的歧义消解
/// 记入规范 v0.2 议程。C_AL09（node:c.b）与过稿用法结构同形——农场误报，
/// 改判文档化边界，语义钉死防回归。
#[test]
fn al09_budget_scope_into_body_is_confirmed_semantics() {
    // C_AL09 原样本：点号穿体预算 scope 必须过审（既定语义）。
    let report = farm("C_AL09_budget_scope_into_body.yaml");
    assert!(
        report.is_ok(),
        "预算 scope 点号穿体 = G3 锁定语义（裁决 2026-10-25 a），C_AL09 为误报：\n{report}"
    );

    // 相邻反例：穿体路径悬空（体内无该节点）仍须 E2207——确认语义 ≠ 不查。
    let src = "\
version: 1
meta:
  name: pin
  graph_version: '1'
  entry: n1
  input: T_in
  output: { ref: T_out, from: n1 }
types:
- name: T_in
  schema: { type: object, properties: { q: { type: string } } }
- name: T_out
  schema: { type: object, properties: { r: { type: string } } }
nodes:
- id: n1
  kind: task
  uses: { model: m@1 }
  emits: { out: T_out }
  context:
    sections: [{ from: input, fields: [q], quota_tokens: 10 }]
- id: c
  kind: task
  loop:
    body:
      nodes:
      - id: b
        kind: check
        uses: { unit: g@1 }
      edges: []
    max_iterations: 1
    safety_points: [{ node: b, kind: check }]
edges: []
policies:
  budgets:
  - { id: bad, dimension: turns, scope: node:c.ghost, cap: 3 }
";
    let doc = af_dsl::load_str(src).unwrap();
    let report = validate(&doc);
    assert_eq!(
        codes(&report),
        vec!["E2207"],
        "穿体路径悬空照报 E2207（确认的是解析语义，不是免检）：\n{report}"
    );
}

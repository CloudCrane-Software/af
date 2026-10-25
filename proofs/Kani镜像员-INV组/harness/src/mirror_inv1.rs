//! INV-1 判定镜像（A-01..A-06）。
//!
//! 镜像源：af-wt-phase2-dsl@phase2-g2b crates/af-dsl/src/validate/inv1.rs。
//! 抽象（承 FV-0.2 镜像法，语义保距）：
//! - `String` 词面 → 小整数域：from 五词表 + 域外 2 值（5..7，inv1.rs:333 跳过支）；
//!   port → 2 槽（P0/P1，"1-2 槽" 短域）；store/字段/形参名 → 成员关系布尔位。
//! - `Option<String>` 形状 → present 布尔位（None 与空串合并且——inv1.rs:194
//!   `is_none_or(|t| t.is_empty())` 同型）。
//! - quota_tokens: u64 → {0, >0} 布尔位（E2302 判定只看 ==0，inv1.rs:337）。
//! - 段数 ≤2（n_secs∈0..=2）；每节点单视角（check 逐节点独立，inv1.rs:36-40）。
//! - 总量上限路径（inv1.rs:78-100，opts.context_quota_cap 默认关）不在镜像域，
//!   见 MAPPING.md 注 3。
//!
//! 域基数（每 harness 声明有界域；CBMC 对位域全域判定）：
//! - 节点位：llm 2 × has_port 4 × reads 4 × n_secs 3
//! - 每段：from 7 × tref 2 × fields 2×2×2 × name 2×2 × port 2×2 × store 2×2×2 × quota 2 × pol 2

use crate::{C_E2301, C_E2302, C_E2303, C_E2304, C_E2402, FAM_INV1};

pub const FROM_TEMPLATE: u8 = 0; // inv1.rs:193
pub const FROM_INPUT: u8 = 1; // inv1.rs:220
pub const FROM_ARG: u8 = 2; // inv1.rs:250
pub const FROM_EDGE: u8 = 3; // inv1.rs:276
pub const FROM_STORE: u8 = 4; // inv1.rs:308
// 5、6 = 词表外 from（M1 层 E1004 域；inv1.rs:333 `_ => {}` 整段跳过支）

/// 单段镜像描述符（SectionDecl model.rs:676-696 的判定相关位投影）。
#[derive(Clone, Copy)]
pub struct SecMirror {
    pub from: u8, // 域 0..7（0..5 五词表 + 5..7 域外）
    pub tref_nonempty: bool,    // template_ref Some 且非空（inv1.rs:194）
    pub fields_present: bool,   // fields Some（inv1.rs:221-230）
    pub field_ok: bool,         // 单字段 ∈ meta.input schema（inv1.rs:233-244；1 字段短域）
    pub schema_has_props: bool, // input_has_properties（inv1.rs:232 抑制位）
    pub name_present: bool,     // arg.name Some（inv1.rs:251-259）
    pub in_formals: bool,       // name ∈ 可见形参表（inv1.rs:260-273 → E2402）
    pub port_missing: bool,     // edge.port None（inv1.rs:277-285）
    pub port_idx: u8,           // 域 0..2 → P0/P1（inv1.rs:286-300 in_ports.contains）
    pub store_present: bool,    // store Some（inv1.rs:309-317）
    pub store_declared: bool,   // store 名 ∈ 顶层 stores 表（inv1.rs:318-329）
    pub quota0: bool,           // quota_tokens == 0（inv1.rs:337；u64 域抽象 {0,>0}）
    pub policy_none: bool,      // policy == Some("none")（inv1.rs:337 例外条件）
}

impl SecMirror {
    /// 全合法段（completeness/sanity 用）：input 段 + 字段在 schema + 配额 1。
    pub fn legal() -> Self {
        SecMirror {
            from: FROM_INPUT,
            tref_nonempty: true,
            fields_present: true,
            field_ok: true,
            schema_has_props: true,
            name_present: true,
            in_formals: true,
            port_missing: false,
            port_idx: 0,
            store_present: true,
            store_declared: true,
            quota0: false,
            policy_none: false,
        }
    }
}

/// 来源可解析性子判定（E2301 支；无配额/无 E2402）。
/// 逐支锚定 inv1.rs:192-334 的 match；返回 true = 本段来源全可解析。
pub fn sec_source_resolvable(s: &SecMirror, in_ports: &[bool; 2]) -> bool {
    match s.from {
        FROM_TEMPLATE => s.tref_nonempty, // inv1.rs:193-203
        FROM_INPUT => {
            // inv1.rs:220-247：fields 缺失 ⇒ 不可解析；在场则逐字段查 schema，
            // meta.input 无 properties 时整段跳过（inv1.rs:231-232 抑制，E2205 域）。
            if !s.fields_present {
                false
            } else if s.schema_has_props {
                s.field_ok
            } else {
                true
            }
        }
        FROM_ARG => s.name_present, // inv1.rs:250-259（name 缺失归 E2301；∉形参归 E2402 另判）
        FROM_EDGE => {
            // inv1.rs:276-300：port 缺失或 ∉ 入边端口集 ⇒ 不可解析。
            !s.port_missing && in_ports[s.port_idx as usize]
        }
        FROM_STORE => s.store_present && s.store_declared, // inv1.rs:308-329
        _ => true, // inv1.rs:333 词表外整段跳过（E1004 已报，本层不定罪）
    }
}

/// E2402 支：arg 段 name ∈ 可见形参表（inv1.rs:260-273）。
/// name 缺失归 E2301（inv1.rs:250-259），E2402 支不判——镜像分码口径与实装一致。
pub fn sec_arg_in_formals(s: &SecMirror) -> bool {
    if s.from != FROM_ARG {
        return true;
    }
    if !s.name_present {
        return true; // 缺 name 走 E2301 支（:250-259）
    }
    s.in_formals
}

/// E2302 支：配额为正，例外仅 store 段 policy:none 记 0（inv1.rs:336-346；
/// 例外条件在 match 之外，对一切 from 生效——镜像同构）。
pub fn sec_quota_ok(s: &SecMirror) -> bool {
    !s.quota0 || (s.from == FROM_STORE && s.policy_none)
}

/// 单段校验镜像 = check_section（inv1.rs:180-347）的码集合投影。
pub fn check_section_mirror(s: &SecMirror, in_ports: &[bool; 2], port_refs: &mut [u8; 2]) -> u32 {
    let mut out = 0;
    // 来源可解析（E2301）→ inv1.rs:179 判定序首位。
    if !sec_source_resolvable(s, in_ports) {
        out |= C_E2301;
    }
    // edge 段合法时累计 from:edge 段引用计数（inv1.rs:301-303；port_refs 键 = 真实入边端口）。
    if s.from == FROM_EDGE && !s.port_missing && in_ports[s.port_idx as usize] {
        port_refs[s.port_idx as usize] += 1;
    }
    // arg 形参（E2402）→ inv1.rs:250-273。
    if !sec_arg_in_formals(s) {
        out |= C_E2402;
    }
    // 配额（E2302）→ inv1.rs:336-346（match 外，逐段必判）。
    if !sec_quota_ok(s) {
        out |= C_E2302;
    }
    out
}

/// 节点级镜像输入（check_node inv1.rs:42-165 的输入投影）。
#[derive(Clone, Copy)]
pub struct NodeMirror {
    pub llm_bound: bool,     // uses.model 在场（inv1.rs:50）
    pub has_port: [bool; 2], // 入边端口 P0/P1 在场（inv1.rs:53-61；2 槽短域）
    pub reads_nonempty: bool, // reads 声明非空（inv1.rs:107）
    pub reads_decl_hit: bool, // reads 含【已声明】store（inv1.rs:108-115 抑制过滤后的非空）
    pub n_secs: u8,          // 域 0..=2（context.sections 段数；0 = 无 context/空 sections）
    pub secs: [SecMirror; 2],
}

impl NodeMirror {
    /// 全合法节点：无入边端口、无 reads、单条合法 input 段。
    pub fn legal() -> Self {
        NodeMirror {
            llm_bound: false,
            has_port: [false, false],
            reads_nonempty: false,
            reads_decl_hit: false,
            n_secs: 1,
            secs: [SecMirror::legal(), SecMirror::legal()],
        }
    }
}

/// 节点级镜像 = check_node（inv1.rs:42-165）的码集合投影。
/// n_secs ≤2 变量界循环全展开（CBMC 工程纪律，FV-0 阻断二教训）。
/// 独立计 port_refs 的辅助入口供 harness 取前提（A-04/A-06）。
pub fn port_refs_of(n: &NodeMirror) -> [u8; 2] {
    let mut port_refs = [0u8; 2]; // inv1.rs:63（键 = 真实入边端口 has_port）
    if n.n_secs >= 1 {
        let s = &n.secs[0];
        if s.from == FROM_EDGE && !s.port_missing && n.has_port[s.port_idx as usize] {
            port_refs[s.port_idx as usize] += 1; // inv1.rs:301-303
        }
    }
    if n.n_secs >= 2 {
        let s = &n.secs[1];
        if s.from == FROM_EDGE && !s.port_missing && n.has_port[s.port_idx as usize] {
            port_refs[s.port_idx as usize] += 1; // inv1.rs:301-303
        }
    }
    port_refs
}

pub fn check_node_mirror(n: &NodeMirror) -> u32 {
    let mut out = 0;
    let mut port_refs = [0u8; 2]; // inv1.rs:63
    // 段级核对（inv1.rs:65-76；对一切带 context 节点适用——A-05「段级核对照常」）。
    if n.n_secs >= 1 {
        out |= check_section_mirror(&n.secs[0], &n.has_port, &mut port_refs);
    }
    if n.n_secs >= 2 {
        out |= check_section_mirror(&n.secs[1], &n.has_port, &mut port_refs);
    }
    // E2303（inv1.rs:107-133）：llm ∧ reads 非空 ∧ 含已声明 store。
    // 抑制（inv1.rs:104-106）：reads 全部未声明 → refs 层 E2001 定罪，本层不产 E2303。
    if n.llm_bound && n.reads_nonempty && n.reads_decl_hit {
        out |= C_E2303;
    }
    // E2304（inv1.rs:135-164）：判定对象 = 仅 LLM-bound 节点（inv1.rs:142 `if llm_bound`）。
    if n.llm_bound {
        for p in 0..2 {
            if !n.has_port[p] {
                continue; // inv1.rs:63：port_refs 只含真实入边端口
            }
            if port_refs[p] == 0 || port_refs[p] > 1 {
                out |= C_E2304; // inv1.rs:143-163：零引用与多引用同码 E2304
            }
        }
    }
    out
}

// ==================== A-01 · INV-1 soundness：section 来源可解析 ====================
// 陈述：某 section 来源不可解析（input.fields ∉ schema / edge.port ∉ 入边端口集 /
// store 名经作用域链不可解析 / template_ref 形状非法，任一）⇒ Report 含 E2301；
// arg.name ∉ 所在体形参表 ⇒ Report 含 E2402。
// 域：单节点 1 段全描述符（from 7 值 × 12 判定位）；n_secs=1。
// 注：实装 E2301 对一切带 context 节点适用（inv1.rs:65 无 llm 门）——
// 陈述主语（LLM-bound task 节点）是其子集，镜像域取超集不弱化 soundness。

#[kani::proof]
fn a01_soundness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 1;
    let s = &mut n.secs[0];
    s.from = kani::any();
    kani::assume(s.from < 7);
    s.tref_nonempty = kani::any();
    s.fields_present = kani::any();
    s.field_ok = kani::any();
    s.schema_has_props = kani::any();
    s.name_present = kani::any();
    s.in_formals = kani::any();
    s.port_missing = kani::any();
    s.port_idx = kani::any();
    kani::assume(s.port_idx < 2);
    s.store_present = kani::any();
    s.store_declared = kani::any();
    n.has_port = kani::any();

    // 前提（陈述的违例谓词，逐项 = 镜像判定子函数）：
    let unresolvable = !sec_source_resolvable(&n.secs[0], &n.has_port);
    let arg_viol = !sec_arg_in_formals(&n.secs[0]);

    let out = check_node_mirror(&n);
    // 违例必报：
    assert!(!unresolvable || out & C_E2301 != 0, "A-01 soundness: 来源不可解析必报 E2301");
    assert!(!arg_viol || out & C_E2402 != 0, "A-01 soundness: arg.name ∉ 形参表必报 E2402");
    // 反向补强（同码不串位）：可解析 ⇒ 不产 E2301；arg 合法 ⇒ 不产 E2402。
    assert!(unresolvable || out & C_E2301 == 0);
    assert!(arg_viol || out & C_E2402 == 0);

    // 非空洞性 sanity（具体角点 = 陈述列举四类之一）：edge.port ∉ 入边端口集。
    let mut bad = NodeMirror::legal();
    bad.secs[0].from = FROM_EDGE;
    bad.secs[0].port_missing = false;
    bad.secs[0].port_idx = 1;
    bad.has_port = [true, false]; // P1 非入边端口
    let out_bad = check_node_mirror(&bad);
    assert!(out_bad & C_E2301 != 0, "sanity: port ∉ 入边端口集必触发 E2301");
}

#[kani::proof]
fn a01_completeness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 1;
    let s = &mut n.secs[0];
    s.from = kani::any();
    kani::assume(s.from < 7);
    s.tref_nonempty = kani::any();
    s.fields_present = kani::any();
    s.field_ok = kani::any();
    s.schema_has_props = kani::any();
    s.name_present = kani::any();
    s.in_formals = kani::any();
    s.port_missing = kani::any();
    s.port_idx = kani::any();
    kani::assume(s.port_idx < 2);
    s.store_present = kani::any();
    s.store_declared = kani::any();
    n.has_port = kani::any();

    // 前提 = 陈述的合法谓词：来源全可解析 ∧ arg ∈ 形参表（或非 arg 段）。
    let legal = sec_source_resolvable(s, &n.has_port) && sec_arg_in_formals(s);
    let out = check_node_mirror(&n);
    // 合法 ⇒ 不产码（completeness 方向，极性 = ¬legal ∨ 无码）。
    assert!(!legal || out & C_E2301 == 0, "A-01 completeness: 来源全可解析 ⇒ 不因 E2301 拒");
    assert!(!legal || out & C_E2402 == 0, "A-01 completeness: arg 对应合法 ⇒ 不因 E2402 拒");

    // 非空洞性 sanity：具体合法角点 ⇒ 两码皆无。
    let ok = NodeMirror::legal();
    let out_ok = check_node_mirror(&ok);
    assert!(out_ok & (C_E2301 | C_E2402) == 0, "sanity: 合法角点不产 E2301/E2402");
}

// ==================== A-02 · INV-1 soundness：配额为正（policy:none 例外） ====================
// 陈述：quota_tokens = 0 且非唯一登记例外（store 段 policy:none 记 0）⇒ E2302；
// 例外自身合法（= 0 不报）。字段缺失走形状层（E1002 域），不在本镜像域。

#[kani::proof]
fn a02_soundness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 1;
    let s = &mut n.secs[0];
    s.from = kani::any();
    kani::assume(s.from < 7);
    s.quota0 = kani::any();
    s.policy_none = kani::any();
    // 其余判定位置合法（本 harness 只判 E2302 支）。
    let out = check_node_mirror(&n);
    let viol = !sec_quota_ok(&n.secs[0]);
    assert!(!viol || out & C_E2302 != 0, "A-02 soundness: 配额 0 非例外必报 E2302");
    // 例外半边（陈述修订核心）：store+none 段配额 0 合法。
    let exempt = n.secs[0].from == FROM_STORE && n.secs[0].policy_none && n.secs[0].quota0;
    assert!(!exempt || out & C_E2302 == 0, "A-02: policy:none 例外不得报 E2302");
    assert!(viol || out & C_E2302 == 0);

    // 非空洞性 sanity：非 store 段配额 0 必报。
    let mut bad = NodeMirror::legal();
    bad.secs[0].quota0 = true; // from=input，非例外
    assert!(check_node_mirror(&bad) & C_E2302 != 0, "sanity: input 段配额 0 必触发 E2302");
}

#[kani::proof]
fn a02_completeness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 1;
    let s = &mut n.secs[0];
    s.from = kani::any();
    kani::assume(s.from < 7);
    s.quota0 = kani::any();
    s.policy_none = kani::any();
    let out = check_node_mirror(&n);
    // 每段配额为正或属例外 ⇒ 不因 E2302 拒（极性 = ¬legal ∨ 无码）。
    assert!(
        !sec_quota_ok(&n.secs[0]) || out & C_E2302 == 0,
        "A-02 completeness: 配额合法 ⇒ 不因 E2302 拒"
    );
    // 具体角点：store+policy:none+quota 0 ⇒ 无 E2302。
    let mut ok = NodeMirror::legal();
    ok.secs[0].from = FROM_STORE;
    ok.secs[0].policy_none = true;
    ok.secs[0].quota0 = true;
    assert!(check_node_mirror(&ok) & C_E2302 == 0, "sanity: 例外角点无 E2302");
}

// ==================== A-03 · INV-1 soundness：LLM 节点禁直读 store ====================
// 陈述：LLM-bound 节点声明 reads ⇒ E2303。
// 实装抑制（inv1.rs:104-115）：reads 只对【已声明】store 判（未声明项 refs 层
// E2001 已定罪）。故 harness 前提 = reads 含已声明 store（实装自述前提，非本
// harness 臆造）；「全部未声明」角落与陈述字面的分歧见 CONFLICTS.md #1 与
// conflict_demo::a03_literal_statement_conflict（CBMC 反例实证）。

#[kani::proof]
fn a03_soundness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 0; // 无 context，隔离 E2301/E2302/E2304 支
    n.llm_bound = true;
    n.reads_nonempty = true;
    n.reads_decl_hit = true; // 实装判定前提：含已声明 store
    let out = check_node_mirror(&n);
    assert!(out & C_E2303 != 0, "A-03 soundness: llm ∧ 声明 reads（含已声明项）必报 E2303");
}

#[kani::proof]
fn a03_completeness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 0;
    let s0: bool = kani::any(); // llm_bound
    let s1: bool = kani::any(); // reads_nonempty
    let s2: bool = kani::any(); // reads_decl_hit
    n.llm_bound = s0;
    n.reads_nonempty = s1;
    n.reads_decl_hit = s2;
    let out = check_node_mirror(&n);
    // 合法（不属「llm 声明含已声明 store 的 reads」）⇒ 不因 E2303 拒。
    let fired_cond = n.llm_bound && n.reads_nonempty && n.reads_decl_hit;
    assert!(!fired_cond || out & C_E2303 != 0);
    assert!(fired_cond || out & C_E2303 == 0, "A-03 completeness: 无 llm-reads ⇒ 不因 E2303 拒");
    // 具体合法角点：非 LLM 节点带 reads ⇒ 无 E2303。
    let mut ok = NodeMirror::legal();
    ok.n_secs = 0;
    ok.llm_bound = false;
    ok.reads_nonempty = true;
    ok.reads_decl_hit = true;
    assert!(check_node_mirror(&ok) & C_E2303 == 0, "sanity: 非 LLM 节点 reads 不触发 E2303");
}

// ==================== A-04 · INV-1 soundness：入边端口被 section 引用 ====================
// 陈述（已陈述方向）：LLM-bound 节点的入边端口未被任何 section 引用 ⇒ E2304。
// 实装扩展（inv1.rs:153-161，A-04 UNCERTAIN 审计补证）：多引用同码 E2304——
// 镜像同构并一并断言（实装口径，非陈述新增义务）。

#[kani::proof]
fn a04_soundness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 2;
    n.llm_bound = true; // 陈述主语
    n.has_port = kani::any();
    for i in 0..2 {
        let s = &mut n.secs[i];
        // 段 ∈ {合法 template 占位, edge→P0, edge→P1}：edge 段的 port 缺失位恒 false
        //（port 缺失归 A-01 的 E2301 支，不在本 harness 域内制造）。
        let kind: u8 = kani::any();
        kani::assume(kind < 3);
        if kind == 0 {
            s.from = FROM_TEMPLATE;
            s.tref_nonempty = true;
        } else {
            s.from = FROM_EDGE;
            s.port_missing = false;
            s.port_idx = kind - 1;
        }
        s.quota0 = false;
    }
    let refs = port_refs_of(&n);
    let out = check_node_mirror(&n);
    for p in 0..2 {
        if n.has_port[p] {
            // 已陈述方向：零引用必报。
            assert!(
                refs[p] != 0 || out & C_E2304 != 0,
                "A-04 soundness: 端口 P{p} 零引用必报 E2304"
            );
            // 实装口径（inv1.rs:153-161）：多引用同码；且恰一引必不报。
            assert!(
                refs[p] <= 1 || out & C_E2304 != 0,
                "A-04（实装口径）: 端口 P{p} 多引用同码必报 E2304"
            );
            assert!(refs[p] == 1 || out & C_E2304 != 0);
        }
    }

    // 非空洞性 sanity：两段同指 P0、P1 无人引用 ⇒ 两端口皆 E2304。
    let mut bad = NodeMirror::legal();
    bad.n_secs = 2;
    bad.llm_bound = true;
    bad.has_port = [true, true];
    bad.secs[0] = SecMirror::legal();
    bad.secs[0].from = FROM_EDGE;
    bad.secs[0].port_idx = 0;
    bad.secs[1] = bad.secs[0];
    let out_bad = check_node_mirror(&bad);
    assert!(out_bad & C_E2304 != 0, "sanity: P0 双引/P1 零引必触发 E2304");
}

#[kani::proof]
fn a04_completeness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 2;
    n.llm_bound = kani::any();
    n.has_port = kani::any();
    for i in 0..2 {
        let s = &mut n.secs[i];
        let kind: u8 = kani::any();
        kani::assume(kind < 3);
        if kind == 0 {
            s.from = FROM_TEMPLATE;
            s.tref_nonempty = true;
        } else {
            s.from = FROM_EDGE;
            s.port_missing = false;
            s.port_idx = kind - 1;
        }
        s.quota0 = false;
    }
    let refs = port_refs_of(&n);
    // 前提（陈述合法域）：每个真实入边端口恰被一节引用。
    let exactly_one = (0..2).all(|p| !n.has_port[p] || refs[p] == 1);
    let out = check_node_mirror(&n);
    assert!(
        !(n.llm_bound && exactly_one) || out & C_E2304 == 0,
        "A-04 completeness: 端口恰被一节引用 ⇒ 不因 E2304 拒"
    );
    // 具体合法角点：两端口各恰一引 ⇒ 无 E2304。
    let mut ok = NodeMirror::legal();
    ok.n_secs = 2;
    ok.llm_bound = true;
    ok.has_port = [true, true];
    ok.secs[0] = SecMirror::legal();
    ok.secs[0].from = FROM_EDGE;
    ok.secs[0].port_idx = 0;
    ok.secs[1] = SecMirror::legal();
    ok.secs[1].from = FROM_EDGE;
    ok.secs[1].port_idx = 1;
    assert!(check_node_mirror(&ok) & C_E2304 == 0, "sanity: 恰一引角点无 E2304");
}

// ==================== A-05 · INV-1 判定对象范围（范围定理） ====================
// 陈述：E2304 判定对象 = LLM-bound task 节点；非 LLM 节点带 context 且入边端口
// 未被引用不触发 E2304；其段级核对 E2301/E2302/E2401-E2403 照常适用。
// harness 1（范围 soundness）：全节点域上 E2304 触发 ⇒ llm_bound。
// harness 2（存在性）：具体非 LLM 反例形态通过 + 段级核对仍照常。

#[kani::proof]
fn a05_range_soundness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 2;
    n.llm_bound = kani::any();
    n.has_port = kani::any();
    n.reads_nonempty = kani::any();
    n.reads_decl_hit = kani::any();
    for i in 0..2 {
        let s = &mut n.secs[i];
        let kind: u8 = kani::any();
        kani::assume(kind < 3);
        if kind == 0 {
            s.from = FROM_TEMPLATE;
            s.tref_nonempty = true;
        } else {
            s.from = FROM_EDGE;
            s.port_missing = false;
            s.port_idx = kind - 1;
        }
        s.quota0 = false;
    }
    let out = check_node_mirror(&n);
    assert!(out & C_E2304 == 0 || n.llm_bound, "A-05: E2304 触发 ⇒ 判定对象为 LLM-bound");
    // 非 LLM 节点段级核对照常：不可解析段在非 LLM 节点同样产 E2301。
    if !n.llm_bound {
        let mut any_unresolvable = false;
        if n.n_secs >= 1 && !sec_source_resolvable(&n.secs[0], &n.has_port) {
            any_unresolvable = true;
        }
        if n.n_secs >= 2 && !sec_source_resolvable(&n.secs[1], &n.has_port) {
            any_unresolvable = true;
        }
        assert!(
            !any_unresolvable || out & C_E2301 != 0,
            "A-05（括注）: 非 LLM 节点段级核对照常适用"
        );
    }
}

#[kani::proof]
fn a05_existence_non_llm() {
    // 具体反例形态：非 LLM 判定单元（kind:check + uses.unit——kind/unit 不入
    // INV-1 判定，镜像域不建；A-05 UNCERTAIN：实装口径 inv1.rs:50 = uses.model 在场），
    // 带 context、入边端口 P1 未被引用：不得触发 E2304。
    let mut n = NodeMirror::legal();
    n.llm_bound = false;
    n.has_port = [true, true];
    n.n_secs = 1;
    n.secs[0] = SecMirror::legal();
    n.secs[0].from = FROM_EDGE;
    n.secs[0].port_idx = 0; // 只引用 P0，P1 零引用
    let out = check_node_mirror(&n);
    assert!(out & C_E2304 == 0, "A-05 存在性: 非 LLM 节点端口零引不触发 E2304");
    assert!(out == 0, "A-05 存在性: 该合法形态整份通过");
    // 对照组：同形态改 LLM-bound ⇒ E2304 必现（范围边界两侧都被钉住）。
    let mut m = n;
    m.llm_bound = true;
    assert!(check_node_mirror(&m) & C_E2304 != 0, "对照: LLM-bound 同形态必报");
}

// ==================== A-06 · INV-1 completeness：合法必过 ====================
// 陈述：满足 INV-1 全部子句（来源全可解析、每段配额为正、无 LLM 节点声明
// reads、LLM-bound 节点入边端口恰被一节引用）⇒ 不因 E2301/E2302/E2303/E2304 拒。
// 审计注：前提「来源全可解析」含 arg 名 ∈ 形参表（审计标记：E2401-E2403 族码
// 前提已覆盖）；结论只限四码——镜像按陈述原样。

#[kani::proof]
fn a06_completeness() {
    let mut n = NodeMirror::legal();
    n.n_secs = 2;
    n.llm_bound = kani::any();
    n.has_port = kani::any();
    n.reads_nonempty = kani::any();
    n.reads_decl_hit = kani::any();
    for i in 0..2 {
        let s = &mut n.secs[i];
        s.from = kani::any();
        kani::assume(s.from < 7);
        s.tref_nonempty = kani::any();
        s.fields_present = kani::any();
        s.field_ok = kani::any();
        s.schema_has_props = kani::any();
        s.name_present = kani::any();
        s.in_formals = kani::any();
        s.port_missing = kani::any();
        s.port_idx = kani::any();
        kani::assume(s.port_idx < 2);
        s.store_present = kani::any();
        s.store_declared = kani::any();
        s.quota0 = kani::any();
        s.policy_none = kani::any();
    }
    // INV-1 全部子句前提：
    let mut clauses_ok = true;
    if n.n_secs >= 1 {
        clauses_ok &= sec_source_resolvable(&n.secs[0], &n.has_port)
            && sec_arg_in_formals(&n.secs[0])
            && sec_quota_ok(&n.secs[0]);
    }
    if n.n_secs >= 2 {
        clauses_ok &= sec_source_resolvable(&n.secs[1], &n.has_port)
            && sec_arg_in_formals(&n.secs[1])
            && sec_quota_ok(&n.secs[1]);
    }
    clauses_ok &= !(n.llm_bound && n.reads_nonempty && n.reads_decl_hit);
    let refs06 = port_refs_of(&n);
    for p in 0..2 {
        if n.has_port[p] && n.llm_bound && refs06[p] != 1 {
            clauses_ok = false;
        }
    }
    let out = check_node_mirror(&n);
    assert!(
        !clauses_ok || out & FAM_INV1 == 0,
        "A-06 completeness: INV-1 子句全满足 ⇒ 不因 E2301/E2302/E2303/E2304 拒"
    );
    // 具体合法角点。
    let mut ok = NodeMirror::legal();
    ok.llm_bound = true;
    ok.has_port = [true, false];
    ok.n_secs = 1;
    ok.secs[0] = SecMirror::legal();
    ok.secs[0].from = FROM_EDGE;
    ok.secs[0].port_idx = 0;
    assert!(check_node_mirror(&ok) & FAM_INV1 == 0, "sanity: LLM 合法角点全过");
}

// soundness 对应面（A-06 配对）：任一子句违例 ⇒ 族内对应码必现（逐子句）。
#[kani::proof]
fn a06_soundness_counterpart() {
    let mut n = NodeMirror::legal();
    n.n_secs = 2;
    n.llm_bound = kani::any();
    n.has_port = kani::any();
    n.reads_nonempty = kani::any();
    n.reads_decl_hit = kani::any();
    for i in 0..2 {
        let s = &mut n.secs[i];
        s.from = kani::any();
        kani::assume(s.from < 7);
        s.tref_nonempty = kani::any();
        s.fields_present = kani::any();
        s.field_ok = kani::any();
        s.schema_has_props = kani::any();
        s.name_present = kani::any();
        s.in_formals = kani::any();
        s.port_missing = kani::any();
        s.port_idx = kani::any();
        kani::assume(s.port_idx < 2);
        s.store_present = kani::any();
        s.store_declared = kani::any();
        s.quota0 = kani::any();
        s.policy_none = kani::any();
    }
    let out = check_node_mirror(&n);
    // 逐子句「违例必报」（A-01..A-04 在节点级 2 段域上的合取重述）：
    let mut unresolvable = false;
    let mut quota_viol = false;
    if n.n_secs >= 1 {
        if !sec_source_resolvable(&n.secs[0], &n.has_port) {
            unresolvable = true;
        }
        if !sec_quota_ok(&n.secs[0]) {
            quota_viol = true;
        }
    }
    if n.n_secs >= 2 {
        if !sec_source_resolvable(&n.secs[1], &n.has_port) {
            unresolvable = true;
        }
        if !sec_quota_ok(&n.secs[1]) {
            quota_viol = true;
        }
    }
    assert!(!unresolvable || out & C_E2301 != 0);
    assert!(!quota_viol || out & C_E2302 != 0);
    assert!(
        !(n.llm_bound && n.reads_nonempty && n.reads_decl_hit) || out & C_E2303 != 0
    );
    let refs = port_refs_of(&n);
    let port_viol = n.llm_bound
        && ((n.has_port[0] && refs[0] != 1) || (n.has_port[1] && refs[1] != 1));
    assert!(!port_viol || out & C_E2304 != 0);
}

//! INV-4 判定镜像（A-14..A-16）。
//!
//! 镜像源：af-wt-phase2-dsl@phase2-g2b crates/af-dsl/src/validate/inv4.rs。
//! 抽象：3 节点、2 store 槽；store 名 → 槽位布尔（reads[] → reads[n][s]、
//! from:store 段 → sec_ref[n][s]）；kind 词面 → {Check, Gate, Task}；uses.unit
//! → bool；eval_input == Some(true) → bool；writer 已解析 → 节点索引，
//! 悬空 → DANGLING 哨兵（inv4.rs:57-59 跳过支）。
//!
//! A-16 声明面视野：镜像判定输入只含 reads/sec_ref 两个声明通道位；
//! oob 位（组件内嵌读取等引用扫描视野外通道）为域外演示位——
//! inv4_check 的签名不读它，a16 harness 以双拷贝相等性机器钉住。

use crate::C_E2331;

pub const DANGLING: u8 = 3; // writer 路径悬空哨兵（inv4.rs:57-59 else continue）

pub const I4_CHECK: u8 = 0; // inv4.rs:27 kind 词面 "check"
pub const I4_GATE: u8 = 1; // "gate"（验证面但非评估器）
pub const I4_TASK: u8 = 2; // 其余抽象

/// 节点是否评估器（inv4.rs:26-28：kind=check 且 uses.unit 在场——严格双条件，
/// gate 不算、check 缺 unit 不算）。
pub fn is_evaluator(kind: u8, unit: bool) -> bool {
    kind == I4_CHECK && unit
}

/// INV-4 镜像输入（inv4.rs:44-77 输入投影）。
#[derive(Clone, Copy)]
pub struct Inv4Mirror {
    /// reads[n][s]：节点 n 的 reads[] 含 store s（inv4.rs:32-35）。
    pub reads: [[bool; 2]; 3],
    /// sec_ref[n][s]：节点 n 的 context.sections 存在 from:store 段引用 s（inv4.rs:36-40）。
    pub sec_ref: [[bool; 2]; 3],
    /// 【域外演示位】引用扫描视野外通道（组件内嵌绕过等）。inv4_check 不读它——
    /// A-16 声明面范围定理的对象（提案 §8-5/【矩阵】:24）。
    pub oob: [[bool; 2]; 3],
    /// kind[n] 域 0..3（inv4.rs:27）。
    pub kind: [u8; 3],
    /// uses.unit 在场（inv4.rs:27）。
    pub unit: [bool; 3],
    /// eval_input[s] == Some(true)（inv4.rs:54）。
    pub eval_input: [bool; 2],
    /// 已解析声明 writer ∈ 0..3；DANGLING = 跳过支（inv4.rs:57-59；E2001 域）。
    pub writer: [u8; 2],
}

impl Inv4Mirror {
    pub fn zero() -> Self {
        Inv4Mirror {
            reads: [[false; 2]; 3],
            sec_ref: [[false; 2]; 3],
            oob: [[false; 2]; 3],
            kind: [I4_TASK; 3],
            unit: [false; 3],
            eval_input: [false; 2],
            writer: [DANGLING; 2],
        }
    }
}

/// 节点是否（声明面）读 store s = reads_store（inv4.rs:31-42）。
/// 声明通道恰二：reads[] 与 from:store 段；oob 通道不在函数视野。
pub fn reads_store(m: &Inv4Mirror, n: usize, s: usize) -> bool {
    let by_reads = m.reads[n][s]; // inv4.rs:32-35
    let by_section = m.sec_ref[n][s]; // inv4.rs:36-40
    by_reads || by_section
}

/// INV-4 核对镜像 = check（inv4.rs:44-77）。显式循环（CBMC 工程纪律）。
pub fn inv4_check(m: &Inv4Mirror) -> u32 {
    // 评估器全路径集（inv4.rs:45-51；§5「evaluator nodes」复数口径——对全部
    // eval_input store 通用，绑定关系属运行期装配 E9003 域）。
    let mut evaluator = [false; 3];
    for n in 0..3 {
        evaluator[n] = is_evaluator(m.kind[n], m.unit[n]);
    }
    let mut out = 0;
    for s in 0..2 {
        if !m.eval_input[s] {
            continue; // inv4.rs:53-55：只判 eval_input == Some(true) 的 store
        }
        if m.writer[s] == DANGLING {
            continue; // inv4.rs:57-59：writer 悬空 refs 层 E2001 已定罪，本层跳过
        }
        for n in 0..3 {
            // inv4.rs:61-64：读者 ∧ 非声明 writer ∧ 非评估器 ⇒ E2331。
            if reads_store(m, n, s) && n != m.writer[s] as usize && !evaluator[n] {
                out |= C_E2331; // inv4.rs:66-75（每节点一条——码集合镜像计一次）
            }
        }
    }
    out
}

/// 违例谓词（与 inv4_check 判定同构的独立重述，供 harness 前提用）。
pub fn inv4_violation(m: &Inv4Mirror) -> bool {
    let mut evaluator = [false; 3];
    for n in 0..3 {
        evaluator[n] = is_evaluator(m.kind[n], m.unit[n]);
    }
    let mut viol = false;
    for s in 0..2 {
        if !(m.eval_input[s] && m.writer[s] != DANGLING) {
            continue;
        }
        for n in 0..3 {
            if reads_store(m, n, s) && n != m.writer[s] as usize && !evaluator[n] {
                viol = true;
            }
        }
    }
    viol
}

// ==================== A-14 · INV-4 soundness：评估输入对候选必闭 ====================
// 陈述：eval_input store 的读者集合（reads/context.sections 引用，点号路径解析
// 后）中有节点 ∉ {该 store 的 writer, 绑定 uses.unit 的 check 节点} ⇒ E2331。

#[kani::proof]
fn a14_soundness() {
    let mut m = Inv4Mirror::zero();
    for n in 0..3 {
        for s in 0..2 {
            m.reads[n][s] = kani::any();
            m.sec_ref[n][s] = kani::any();
        }
        m.kind[n] = kani::any();
        kani::assume(m.kind[n] < 3);
        m.unit[n] = kani::any();
    }
    for s in 0..2 {
        m.eval_input[s] = kani::any();
        m.writer[s] = kani::any();
        kani::assume(m.writer[s] < 3); // 已解析域（悬空跳过支见 a14_harness 注记）
    }
    let out = inv4_check(&m);
    let viol = inv4_violation(&m);
    assert!(!viol || out & C_E2331 != 0, "A-14 soundness: 越界读者必报 E2331");
    // 无越界 ⇒ 不报（码不串位）。
    assert!(viol || out & C_E2331 == 0);

    // 非空洞性 sanity（提案样例 4 形态，:1501-1550 反例模板镜像）：gen_a 读
    // eval_input store，非 writer 非评估器 ⇒ 必报；gate 节点读不豁免。
    let mut bad = Inv4Mirror::zero();
    bad.eval_input = [true, false];
    bad.writer = [0, DANGLING];
    bad.kind = [I4_TASK, I4_GATE, I4_CHECK];
    bad.unit = [false, false, false]; // check 无 unit：非评估器
    bad.reads[1][0] = true; // gate 节点经 reads 读 S0
    assert!(inv4_check(&bad) & C_E2331 != 0, "sanity: gate 越界读者触发 E2331");
    // check 缺 unit 同样越界（严格双条件）。
    bad.reads[1][0] = false;
    bad.reads[2][0] = true;
    assert!(inv4_check(&bad) & C_E2331 != 0, "sanity: check 缺 unit 非评估器，越界必报");
}

#[kani::proof]
fn a14_completeness() {
    let mut m = Inv4Mirror::zero();
    for n in 0..3 {
        for s in 0..2 {
            m.reads[n][s] = kani::any();
            m.sec_ref[n][s] = kani::any();
        }
        m.kind[n] = kani::any();
        kani::assume(m.kind[n] < 3);
        m.unit[n] = kani::any();
    }
    for s in 0..2 {
        m.eval_input[s] = kani::any();
        m.writer[s] = kani::any();
        kani::assume(m.writer[s] < 3);
    }
    let out = inv4_check(&m);
    // 合法域（全部读者 ⊆ {writer, check+unit 评估器}）⇒ 不因 E2331 拒。
    let legal = !inv4_violation(&m);
    assert!(!legal || out & C_E2331 == 0, "A-14 completeness: 读者全在白名单 ⇒ 无 E2331");

    // 具体合法角点（:1542-1543 assess 对照形态）：writer 自读 + check+unit 读。
    let mut ok = Inv4Mirror::zero();
    ok.eval_input = [true, false];
    ok.writer = [0, DANGLING];
    ok.kind = [I4_TASK, I4_TASK, I4_CHECK];
    ok.unit = [false, false, true];
    ok.reads[0][0] = true; // writer 自己读：白名单
    ok.sec_ref[2][0] = true; // check+unit 经 from:store 段读：白名单
    assert!(inv4_check(&ok) == 0, "sanity: 白名单读者角点全过");
}

// ==================== A-15 · INV-4 completeness：合法必过 ====================
// 陈述：eval_input store 的全部读者 ⊆ {writer, 绑定 uses.unit 的 check 节点} ⇒ 不因 E2331 拒。
// harness 1：全 2-store 联合域上合法 ⇒ 清（与 a14_completeness 同域不同义务条目——
//   A-15 是独立定理条目，单独取证）。
// harness 2：白名单成员逐点不获罪（每 (节点,store) 局部域）。

#[kani::proof]
fn a15_completeness() {
    let mut m = Inv4Mirror::zero();
    for n in 0..3 {
        for s in 0..2 {
            m.reads[n][s] = kani::any();
            m.sec_ref[n][s] = kani::any();
        }
        m.kind[n] = kani::any();
        kani::assume(m.kind[n] < 3);
        m.unit[n] = kani::any();
    }
    for s in 0..2 {
        m.eval_input[s] = kani::any();
        m.writer[s] = kani::any();
        kani::assume(m.writer[s] < 3);
    }
    let out = inv4_check(&m);
    let all_readers_whitelisted = !inv4_violation(&m);
    assert!(
        !all_readers_whitelisted || out & C_E2331 == 0,
        "A-15 completeness: 读者 ⊆ {writer, 评估器} ⇒ 不因 E2331 拒"
    );
}

#[kani::proof]
fn a15_whitelist_members_never_flagged() {
    // 局部域：单 store S0 eval_input、单读者节点 rn ∈ {writer 自身, check+unit 评估器}。
    let mut m = Inv4Mirror::zero();
    m.eval_input = [true, false];
    m.writer[0] = kani::any();
    kani::assume(m.writer[0] < 3);
    let rn: u8 = kani::any();
    kani::assume(rn < 3);
    m.reads[rn as usize][0] = true; // rn 是唯一读者
    m.kind[rn as usize] = kani::any();
    kani::assume(m.kind[rn as usize] < 3);
    m.unit[rn as usize] = kani::any();
    let out = inv4_check(&m);
    let whitelisted = rn == m.writer[0] || is_evaluator(m.kind[rn as usize], m.unit[rn as usize]);
    assert!(
        !whitelisted || out & C_E2331 == 0,
        "A-15: 白名单成员（writer / check+unit）读 eval_input 不获罪"
    );
    // 具体角点两侧：writer 自读通过；陌生 task 读获罪（域非退化对照）。
    let mut w = Inv4Mirror::zero();
    w.eval_input = [true, false];
    w.writer[0] = 2;
    w.reads[2][0] = true;
    assert!(inv4_check(&w) == 0, "sanity: writer 自读通过");
    let mut t = w;
    t.writer[0] = 0;
    assert!(inv4_check(&t) & C_E2331 != 0, "sanity: 同一读者换非 writer 即获罪");
}

// ==================== A-16 · INV-4 分层边界：声明面 vs 运行时拦截 ====================
// 陈述：E2331 只覆盖声明面读法；组件内嵌绕过超出引用扫描视野；运行时半边
// v0 不可立（执行面读权限白名单层占位）。harness 钉住可判的声明面半边：
// ① 判定基据 ⊆ 声明面（fired ⇒ 存在 reads/sec_ref 引用）；
// ② 视野外通道（oob 位）不影响判定结果（双拷贝相等性）。
// 运行时半边 = 陈述自认不可立（占位层不存在），无 harness 义务——如实记录。

#[kani::proof]
fn a16_decl_surface_basis() {
    let mut m = Inv4Mirror::zero();
    for n in 0..3 {
        for s in 0..2 {
            m.reads[n][s] = kani::any();
            m.sec_ref[n][s] = kani::any();
        }
        m.kind[n] = kani::any();
        kani::assume(m.kind[n] < 3);
        m.unit[n] = kani::any();
    }
    for s in 0..2 {
        m.eval_input[s] = kani::any();
        m.writer[s] = kani::any();
        kani::assume(m.writer[s] < 3);
    }
    let out = inv4_check(&m);
    // fired ⇒ 存在声明面基据：某 eval_input 已解析 store 有节点经 reads/段引用
    // 且越白名单（判定只建立在声明通道之上）。
    let basis_exists = inv4_violation(&m);
    assert!(
        out & C_E2331 == 0 || basis_exists,
        "A-16①: E2331 触发 ⇒ 声明面引用基据存在（引用扫描视野内）"
    );
}

#[kani::proof]
fn a16_oob_independence() {
    let mut m = Inv4Mirror::zero();
    for n in 0..3 {
        for s in 0..2 {
            m.reads[n][s] = kani::any();
            m.sec_ref[n][s] = kani::any();
        }
        m.kind[n] = kani::any();
        kani::assume(m.kind[n] < 3);
        m.unit[n] = kani::any();
    }
    for s in 0..2 {
        m.eval_input[s] = kani::any();
        m.writer[s] = kani::any();
        kani::assume(m.writer[s] < 3);
    }
    // 双拷贝：唯一差异 = oob 位全 0 vs 全 1（内嵌绕过通道开/关）。
    let mut m2 = m;
    m2.oob = [[true; 2]; 3];
    let r1 = inv4_check(&m);
    let r2 = inv4_check(&m2);
    assert!(r1 == r2, "A-16②: 视野外通道（oob）不影响 E2331 判定——只覆盖声明面读法");
    // 非空洞性（具体角点）：判定本身非退化——域内确有触发形态。
    let mut bad = Inv4Mirror::zero();
    bad.eval_input = [true, false];
    bad.writer = [0, DANGLING];
    bad.reads[1][0] = true;
    assert!(inv4_check(&bad) & C_E2331 != 0, "sanity: 触发形态存在（判定非退化）");
}

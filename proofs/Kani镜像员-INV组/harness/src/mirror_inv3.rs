//! INV-3 判定镜像（A-11..A-13）。
//!
//! 镜像源：af-wt-phase2-dsl@phase2-g2b crates/af-dsl/src/validate/inv3.rs。
//! 抽象：3 节点单作用域；kind 开放词表 → 4 抽象值 {Check, Gate, Task, Transform}
//!（非 {check,gate} 词面在 is_verifier/E2322 判定中行为同型）；human → bool；
//! 边 → 3×3 布尔邻接阵（edge[i][j] = i 生产 → j 消费，同作用域本地 id——
//! inv3.rs:37-39「所在体作用域内的边」；跨体同本地 id 干扰属 E2003 域，域外）。
//! safety point 解析失败（E2107 域）→ resolved 布尔位显式化（inv3.rs:64-66 跳过支）。

use crate::{C_E2321, C_E2322};

pub const K_CHECK: u8 = 0; // inv3.rs:24 kind 词面 "check"
pub const K_GATE: u8 = 1; // inv3.rs:24 kind 词面 "gate"
pub const K_TASK: u8 = 2; // 开放词表其余值抽象 1
pub const K_TRANSFORM: u8 = 3; // 开放词表其余值抽象 2

pub const SP_CHECK: u8 = 0; // SafetyPointKind::Check（model.rs:481）
pub const SP_GATE: u8 = 1; // SafetyPointKind::Gate（model.rs:482）
pub const SP_HUMAN: u8 = 2; // SafetyPointKind::Human（model.rs:483）

/// 3 节点描述（inv3.rs is_verifier 输入投影）。
#[derive(Clone, Copy)]
pub struct Node3 {
    pub kind: [u8; 3], // 域 0..4
    pub human: [bool; 3],
}

impl Node3 {
    pub fn zero() -> Self {
        Node3 { kind: [K_TASK; 3], human: [false; 3] }
    }
}

/// 是否验证面节点（inv3.rs:23-25：kind ∈ {check, gate} ∪ human 标记）。
pub fn is_verifier(kind: u8, human: bool) -> bool {
    matches!(kind, K_CHECK | K_GATE) || human
}

/// 3 节点同作用域邻接阵（inv3.rs:39 边扫描输入投影）。
/// edge[i][j] = 节点 i 生产 → 节点 j 消费的一条边。
pub type Graph3 = [[bool; 3]; 3];

/// 直接自环判定镜像 = check_self_verification（inv3.rs:32-52）。
/// 范围（inv3.rs:5-8 模块注记）：仅直接自环；间接自验证 v0.1 不查（A-12）。
pub fn self_verification_check(nodes: &Node3, g: &Graph3) -> u32 {
    let mut out = 0;
    for i in 0..3 {
        if !is_verifier(nodes.kind[i], nodes.human[i]) {
            continue; // inv3.rs:34-36
        }
        if g[i][i] {
            out |= C_E2321; // inv3.rs:39-50：from == to == 本节点
        }
    }
    out
}

/// 单个安全点条目（SafetyPointDecl model.rs:473-476 投影）。
#[derive(Clone, Copy)]
pub struct SpMirror {
    pub kind: u8, // 域 0..3 → SP_CHECK/SP_GATE/SP_HUMAN
    pub target: u8, // 域 0..3 → 指向节点
    pub resolved: bool, // 路径可解析（不可解析 = inv3.rs:64-66 跳过，E2107 域）
}

/// 安全点对应性判定镜像 = check_safety_points（inv3.rs:57-89）。
/// 三值原样对应（inv3.rs:67-71；【规范】:28）：check→kind=check 节点、
/// gate→kind=gate 节点、human→human 标记在场。
pub fn safety_points_check(nodes: &Node3, sps: &[SpMirror; 2]) -> u32 {
    let mut out = 0;
    for sp in sps {
        if !sp.resolved {
            continue; // inv3.rs:64-66：路径不可解析不重复定罪
        }
        let t = sp.target as usize;
        let matches = match sp.kind {
            SP_CHECK => nodes.kind[t] == K_CHECK, // inv3.rs:68
            SP_GATE => nodes.kind[t] == K_GATE,   // inv3.rs:69
            _ => nodes.human[t],                  // inv3.rs:70
        };
        if !matches {
            out |= C_E2322; // inv3.rs:72-88
        }
    }
    out
}

// ==================== A-11 · INV-3 soundness：直接自验证必拒 ====================
// 陈述：验证面节点（kind ∈ {check,gate} ∪ human）存在生产者是自己的入边 ⇒ E2321。

#[kani::proof]
fn a11_soundness() {
    let nodes = Node3 {
        kind: kani::any(),
        human: kani::any(),
    };
    for k in nodes.kind {
        kani::assume(k < 4);
    }
    let g: Graph3 = kani::any();
    let out = self_verification_check(&nodes, &g);
    let mut viol = false;
    for i in 0..3 {
        if is_verifier(nodes.kind[i], nodes.human[i]) && g[i][i] {
            viol = true;
        }
    }
    assert!(!viol || out & C_E2321 != 0, "A-11 soundness: 验证面直接自环必报 E2321");
    // 码不串位：无验证面自环 ⇒ 无 E2321。
    assert!(viol || out & C_E2321 == 0);

    // 非空洞性 sanity：check 节点自环必报；task 节点自环不报（主语限定）。
    let mut bad = Node3::zero();
    bad.kind = [K_CHECK, K_TASK, K_TASK];
    let selfloop: Graph3 = [[false; 3], [false; 3], [false; 3]];
    let mut bad_g = selfloop;
    bad_g[0][0] = true;
    assert!(self_verification_check(&bad, &bad_g) & C_E2321 != 0, "sanity: check 自环触发 E2321");
    let mut task_g = selfloop;
    task_g[1][1] = true; // 非验证面节点自环
    assert!(self_verification_check(&bad, &task_g) & C_E2321 == 0, "sanity: task 自环不触发");
}

#[kani::proof]
fn a11_completeness() {
    // 陈述主语的补方向：自环只出现在非验证面节点 ⇒ 无 E2321（产出即验收
    // 规则只约束验证面；非验证面自环如 transform 自载不在判定范围）。
    let nodes = Node3 {
        kind: kani::any(),
        human: kani::any(),
    };
    for k in nodes.kind {
        kani::assume(k < 4);
    }
    let g: Graph3 = kani::any();
    let out = self_verification_check(&nodes, &g);
    let mut selfloops_on_nonverifier_only = true;
    for i in 0..3 {
        if g[i][i] && is_verifier(nodes.kind[i], nodes.human[i]) {
            selfloops_on_nonverifier_only = false;
        }
    }
    assert!(
        !selfloops_on_nonverifier_only || out & C_E2321 == 0,
        "A-11 completeness: 无验证面自环（自环仅在非验证面）⇒ 无 E2321"
    );
    // 具体合法角点：gate + human 均无自环的稠密图。
    let mut ok = Node3::zero();
    ok.kind = [K_GATE, K_CHECK, K_TASK];
    ok.human = [false, false, true];
    let mut ok_g: Graph3 = [[false; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            ok_g[i][j] = i != j; // 全连接但无自环
        }
    }
    assert!(self_verification_check(&ok, &ok_g) & C_E2321 == 0, "sanity: 稠密无自环图无 E2321");
}

// ==================== A-12 · INV-3 范围边界：间接自验证不查（诚实边界定理） ====================
// 陈述：判定范围 = 直接自环；间接自验证形态不在 v0.1 判定范围——合法文档
// 编译通过、不算漏判（提案 §8-4 正本登记缺口；inv3.rs:5-8 注记）。

#[kani::proof]
fn a12_range_soundness() {
    // 范围 soundness：全域上「无验证面直接自环」⇒ 无 E2321——即任何间接
    // 回环形态（经中间节点的数据流可达）都不可能触发 E2321（域含一切 3 节点
    // 有向图，含全部间接循环形态）。
    let nodes = Node3 {
        kind: kani::any(),
        human: kani::any(),
    };
    for k in nodes.kind {
        kani::assume(k < 4);
    }
    let g: Graph3 = kani::any();
    let out = self_verification_check(&nodes, &g);
    let mut no_direct = true;
    for i in 0..3 {
        if g[i][i] && is_verifier(nodes.kind[i], nodes.human[i]) {
            no_direct = false;
        }
    }
    assert!(!no_direct || out & C_E2321 == 0, "A-12: 无直接自环 ⇒ 无 E2321（间接形态不在判定范围）");
}

#[kani::proof]
fn a12_existence_indirect_ok() {
    // 存在性（正本 §8-4 正例 indirect_self_verification_not_flagged_v01 镜像）：
    // 3 节点间接数据流回环 N0(check) → N1 → N2 → N0：N0 经中间节点消费自己
    // 的产出——含间接自验证形态的文档编译通过（无 E2321）。
    let mut nodes = Node3::zero();
    nodes.kind = [K_CHECK, K_TASK, K_TASK];
    let mut g: Graph3 = [[false; 3]; 3];
    g[0][1] = true; // N0 生产 → N1 消费
    g[1][2] = true; // N1 → N2
    g[2][0] = true; // N2 → N0（回环闭合：N0 间接消费自己产出）
    assert!(
        self_verification_check(&nodes, &g) == 0,
        "A-12 存在性: 间接自验证形态不触发 E2321（编译通过）"
    );
    // 对照：同一节点加一条直接自环 ⇒ 立即被判（范围边界两侧钉住）。
    let mut g2 = g;
    g2[0][0] = true;
    assert!(
        self_verification_check(&nodes, &g2) & C_E2321 != 0,
        "对照: 直接自环必被判"
    );
}

// ==================== A-13 · INV-3 completeness + 安全点白名单（对应性） ====================
// ① 无直接自环 ⇒ 不因 E2321 拒。
// ② safety_points 条目 kind 三值与指向节点验证面形态不原样对应 ⇒ E2322；
//   对应（check→check、gate→gate、human→human 标记）⇒ 不报。

#[kani::proof]
fn a13_completeness_e2321() {
    let nodes = Node3 {
        kind: kani::any(),
        human: kani::any(),
    };
    for k in nodes.kind {
        kani::assume(k < 4);
    }
    let g: Graph3 = kani::any();
    let out = self_verification_check(&nodes, &g);
    let mut no_direct = true;
    for i in 0..3 {
        if is_verifier(nodes.kind[i], nodes.human[i]) && g[i][i] {
            no_direct = false;
        }
    }
    assert!(!no_direct || out & C_E2321 == 0, "A-13① completeness: 无直接自环 ⇒ 无 E2321");
}

#[kani::proof]
fn a13_soundness_e2322() {
    let nodes = Node3 {
        kind: kani::any(),
        human: kani::any(),
    };
    for k in nodes.kind {
        kani::assume(k < 4);
    }
    let mut sps = [SpMirror { kind: 0, target: 0, resolved: false }; 2];
    for sp in sps.iter_mut() {
        sp.kind = kani::any();
        sp.target = kani::any();
        sp.resolved = kani::any();
        kani::assume(sp.kind < 3);
        kani::assume(sp.target < 3);
    }
    let out = safety_points_check(&nodes, &sps);
    // 陈述 ②：任一已解析条目不原样对应 ⇒ E2322（含指向节点整体非验证面的情形）。
    let mut viol = false;
    for sp in sps.iter() {
        if !sp.resolved {
            continue;
        }
        let t = sp.target as usize;
        let mismatch = match sp.kind {
            SP_CHECK => nodes.kind[t] != K_CHECK,
            SP_GATE => nodes.kind[t] != K_GATE,
            _ => !nodes.human[t],
        };
        if mismatch {
            viol = true;
        }
    }
    assert!(!viol || out & C_E2322 != 0, "A-13② soundness: 安全点三值不对应必报 E2322");
    // 对应 ⇒ 不报（白名单半边）。
    assert!(viol || out & C_E2322 == 0);

    // 非空洞性 sanity（审计修订核心案例）：sp.kind=check 指向 gate 节点——
    // 宽松读法（kind ∉ {check,gate,human} 才拒）会放过，对应性读法必拒。
    let mut g_only = Node3::zero();
    g_only.kind = [K_GATE, K_CHECK, K_TASK];
    let sps_bad = [
        SpMirror { kind: SP_CHECK, target: 0, resolved: true }, // check → gate 节点：不对应
        SpMirror { kind: SP_GATE, target: 1, resolved: true },  // gate → check 节点：不对应
    ];
    assert!(
        safety_points_check(&g_only, &sps_bad) & C_E2322 != 0,
        "sanity: check→gate 交叉指向必报（宽松读法漏判的形态）"
    );
    // 全对应角点：无 E2322。
    let sps_ok = [
        SpMirror { kind: SP_CHECK, target: 1, resolved: true },  // check → check 节点
        SpMirror { kind: SP_HUMAN, target: 2, resolved: false }, // 未解析：跳过支
    ];
    let mut h_ok = Node3::zero();
    h_ok.kind = [K_TASK, K_CHECK, K_TASK];
    h_ok.human = [false, false, true];
    assert!(safety_points_check(&h_ok, &sps_ok) == 0, "sanity: 对应条目 + 未解析跳过 ⇒ 无 E2322");
}

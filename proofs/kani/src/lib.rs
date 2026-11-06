//! FV-0.2 Kani PoC harness 集（af-dsl 工作树分支 phase2-fv0）。
//!
//! ## 运行（默认集 = 本文件；全部 VERIFIED-SUCCESS 为验收①②之②）
//! ```sh
//! cargo kani            # 全部默认 harness
//! ```
//!
//! ## 能跑 / 不能跑的边界（2026-10-24 实测，证据见同目录 EVIDENCE.md）
//! - ✅ `smoke_u8`：纯 Rust 数值逻辑 —— 0.02s。
//! - ✅ `mirror_*`：normalize **镜像算法**（Vec/整数域，无 BTree/无串构造）
//!   的有界穷尽（27 种 goto 分配 × 4 种赋值）——覆盖三性质 **加** Verus 未证的
//!   E2102/E2103/E2101 判定逻辑与「E2101 抑制」规则。
//! - ❌ 真实 `route::normalize`：编译通过，但 CBMC 对单条**具体**用例 >5 分钟
//!   未完（BTreeMap/BTreeSet + `format!` 守卫串机器）。证据 EVIDENCE.md §G。
//! - ❌ `validate` 链（INV-2 直接构造 Document）：kani-compiler 0.68.0 ICE
//!   （intrinsics.rs:243，chrono/cel-interpreter 可达链触发；空文档亦炸）。
//!   证据 EVIDENCE.md §C/§E。受阻 harness 保留在 `pending` 模块
//!   （feature `blocked-harnesses` 门控，默认不编入）。

#[cfg(feature = "blocked-harnesses")]
pub mod pending;

// ───────── 默认 harness：镜像穷尽 + 冒烟 ─────────

/// 目标域编码：0/1 = 出边目标 t0/t1；2 = exit（route.rs:28 的 EXIT 镜像）。
const EXIT: u8 = 2;
const N_TARGETS: u8 = 2;

/// guard_i 谓词（§1.5）：val[i] ∧ ¬(val[0] ∨ … ∨ val[i-1])。
/// 与 proofs/verus/route_norm.rs 的 `guard_holds` 同构。
fn guard_holds(val: [bool; 2], i: usize) -> bool {
    val[i] && (0..i).all(|k| !val[k])
}

/// default guard（§1.5）：¬(全序析取)；无规则恒真（本镜像恒有 2 规则，故不退化）。
fn default_holds(val: [bool; 2]) -> bool {
    !val[0] && !val[1]
}

/// normalize 判定镜像（route.rs:86-199 的结构保距重写，数据结构换 Vec/整数）：
/// 返回 (e2102 数, e2103 数, e2101 数, 边目标序列（Ok 时长度 = N_TARGETS）,
/// 每边守卫分支（Some(i)=规则 i / None=default）, exit 联合守卫存在性)。
struct MirrorOut {
    e2102: u8,
    e2103: u8,
    e2101: u8,
    edges: Option<[(u8, Option<usize>); N_TARGETS as usize]>,
    exit_guard_present: bool,
}

fn mirror_normalize(gotos: [u8; 3]) -> MirrorOut {
    let [r0, r1, d] = gotos;
    let valid = |g: u8| g == EXIT || g < N_TARGETS;
    let mut e2102 = 0;
    if !valid(r0) { e2102 += 1; }
    if !valid(r1) { e2102 += 1; }
    if !valid(d) { e2102 += 1; }
    // E2103：非 exit 同目标多分支（first-hit 表用线性扫描镜像 BTreeMap）。
    let mut e2103 = 0;
    let non_exit = |g: u8| g != EXIT && g < N_TARGETS;
    if non_exit(r0) && non_exit(r1) && r0 == r1 { e2103 += 1; }
    if non_exit(d) && ((non_exit(r0) && r0 == d) || (non_exit(r1) && r1 == d)) {
        e2103 += 1;
    }
    // E2101：仅在无 E2102/E2103 时判定（route.rs:137-139 的抑制规则）。
    let mut e2101 = 0;
    if e2102 == 0 && e2103 == 0 {
        for t in 0..N_TARGETS {
            if r0 != t && r1 != t && d != t {
                e2101 += 1;
            }
        }
    }
    if e2102 + e2103 + e2101 > 0 {
        return MirrorOut { e2102, e2103, e2101, edges: None, exit_guard_present: false };
    }
    // 边构造：目标升序（BTreeSet 迭代 = 0..N_TARGETS），first-hit 规则否则 default。
    let first_hit = |t: u8| -> Option<usize> {
        if r0 == t { Some(0) } else if r1 == t { Some(1) } else { None }
    };
    let edges = [
        (0u8, first_hit(0)),
        (1u8, first_hit(1)),
    ];
    let exit_guard_present = r0 == EXIT || r1 == EXIT;
    MirrorOut { e2102, e2103, e2101, edges: Some(edges), exit_guard_present }
}

/// **穷尽空间**：goto 分配 3×3×3 = 27（两规则 + default，各 ∈ {t0,t1,exit}，
/// 域内即全枚举——「非法 goto」在编码域外，由下方
/// mirror_normalize_illegal_goto_e2102_exact_count（红队 W3-C28 补，含编码 3
/// 的符号域精确断言 E2102 计数）与 af-dsl route 单测覆盖；pending 模块无
/// route 探针，原文「由 pending 模块…覆盖」系误记，W3-C28 勘误）。
/// 验证：
/// - 违例⇒Err 映射精确（E2102=同目标重复分支；E2101=覆盖缺失且仅在其他两零时判）；
/// - 合法⇒边集恰 2 条、目标严格升序 0<1、守卫=first-hit 规则或 default；
/// - exit 联合守卫存在 ⇔ 存在 exit 规则。
#[kani::proof]
fn mirror_normalize_all_gotos_exhaustive() {
    let g0: u8 = kani::any();
    let g1: u8 = kani::any();
    let gd: u8 = kani::any();
    kani::assume(g0 <= 2 && g1 <= 2 && gd <= 2);
    let out = mirror_normalize([g0, g1, gd]);
    let err_free = out.e2102 == 0 && out.e2103 == 0 && out.e2101 == 0;
    if err_free {
        let edges = out.edges.expect("合法必须有边集");
        // T3 确定序：长度保持 + 严格升序 + 逐位目标。
        assert_eq!(edges[0].0, 0);
        assert_eq!(edges[1].0, 1);
        // 守卫归属：first-hit 或 default。
        assert_eq!(edges[0].1, if g0 == 0 { Some(0) } else if g1 == 0 { Some(1) } else { None });
        assert_eq!(edges[1].1, if g0 == 1 { Some(0) } else if g1 == 1 { Some(1) } else { None });
        // exit 联合守卫存在性。
        assert_eq!(out.exit_guard_present, g0 == EXIT || g1 == EXIT);
    } else {
        assert!(out.edges.is_none(), "任一违例 → Err（不产边）");
        // E2103 精确条件：非 exit 同目标 ≥2 分支。
        let dup = (g0 == g1 && g0 != EXIT)
            || (gd != EXIT && (gd == g0 || gd == g1));
        assert_eq!(out.e2103 > 0, dup);
        // E2101 抑制规则：仅当 E2102=E2103=0 时可非零；且非零 ⇔ 存在未覆盖目标。
        if out.e2102 == 0 && out.e2103 == 0 {
            let uncovered = (g0 != 0 && g1 != 0 && gd != 0) || (g0 != 1 && g1 != 1 && gd != 1);
            assert_eq!(out.e2101 > 0, uncovered);
        } else {
            assert_eq!(out.e2101, 0);
        }
    }
}

/// **穷尽空间（非法编码域，红队 W3-C28）**：goto 分配含非法编码 3 的符号域
/// （3 既非出边目标也非 exit——即 E2102 域）。合法域 27 分配（上
/// harness）内 `valid` 恒真、:55-57 的 E2102 分支不可达——B-16 的 E2102
/// soundness/completeness 半边此前零内核证据（上游正本「已机器穷尽
/// E2102/E2103 计数语义」对 E2102 半边不成立，红队 W3-C28）。本 harness
/// 在含非法编码的域上精确断言 E2102 计数语义：
/// - **双向精确**：E2102 计数 = 非法 goto（非出边目标且非 exit）恰现次数
///   （soundness：非法必计数；completeness：计数恰等于非法次数，无多判）；
/// - **抑制规则**：E2102 > 0 ⇒ E2101 恒 0（覆盖判定仅在 E2102=E2103=0 时
///   进行——route.rs:137-139 实装的抑制语义镜像面）；
/// - **fail-closed**：任一违例 ⇒ 不产边集。
#[kani::proof]
fn mirror_normalize_illegal_goto_e2102_exact_count() {
    let g0: u8 = kani::any();
    let g1: u8 = kani::any();
    let gd: u8 = kani::any();
    kani::assume(g0 <= 3 && g1 <= 3 && gd <= 3);
    kani::assume(g0 == 3 || g1 == 3 || gd == 3);
    let out = mirror_normalize([g0, g1, gd]);
    let illegal = |g: u8| u8::from(g != EXIT && g >= N_TARGETS);
    assert_eq!(
        out.e2102,
        illegal(g0) + illegal(g1) + illegal(gd),
        "E2102 计数 = 非法 goto 恰现次数（soundness + completeness 双向）"
    );
    assert!(out.e2102 >= 1, "域过滤保证至少一个非法编码在场");
    assert_eq!(
        out.e2101, 0,
        "E2102 在场 ⇒ E2101 抑制（覆盖判定被短路）"
    );
    assert!(out.edges.is_none(), "任一违例 → Err（不产边集，fail-closed）");
}

/// **穷尽空间**：赋值 2×2 = 4（两规则 when 的全部真值组合）。
/// T1 互斥 + T2 恰一：任一赋值下，{guard_0, guard_1, default} 恰有一个成立。
/// （与 Verus 的 lemma_exactly_one_branch 同对象、不同机器检查。）
#[kani::proof]
fn mirror_branch_exclusion_exactly_one_all_valuations() {
    let b0: bool = kani::any();
    let b1: bool = kani::any();
    let val = [b0, b1];
    let g0 = guard_holds(val, 0);
    let g1 = guard_holds(val, 1);
    let gd = default_holds(val);
    let count = g0 as u8 + g1 as u8 + gd as u8;
    assert_eq!(count, 1, "恰一个分支成立（互斥 + 覆盖）");
}

/// 冒烟：Kani 在本库依赖树上的基线可用性（2026-10-24 实测 0.017s）。
#[kani::proof]
fn smoke_u8() {
    let x: u8 = kani::any();
    let y = x.wrapping_add(1);
    assert!(y >= x || y < x);
}
